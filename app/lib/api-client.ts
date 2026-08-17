const API_BASE_URL = normalizeBaseUrl(process.env.EXPO_PUBLIC_API_BASE_URL);
const MAX_TIMER_DELAY_MS = 2_147_483_647;
const MAX_ACCESS_TOKEN_REFRESH_LEEWAY_MS = 60_000;
const REFRESH_RETRY_DELAY_MS = 15_000;

let accessToken: string | null = null;
let accessTokenExpiresAtMs: number | null = null;
let accessTokenRefreshAtMs: number | null = null;
let accessTokenExpirationTimer: ReturnType<typeof setTimeout> | null = null;
let refreshAccessTokenHandler: AccessTokenRefreshHandler | null = null;
let clearRefreshSessionHandler: ClearRefreshSessionHandler | null = null;
let accessTokenRefreshPromise: Promise<AccessTokenRefreshResult> | null = null;
let authenticationExpirationNotified = false;

export type AccessTokenRefreshReason =
  | "timer"
  | "request"
  | "app_foreground"
  | "server_unauthorized";

export type AuthenticationExpirationReason =
  | "refresh_failed"
  | "server_unauthorized_after_refresh";

export type AccessTokenRefreshResult =
  | "ready"
  | "temporarily_unavailable"
  | "session_expired";

export class ApiRequestError extends Error {
  constructor(
    message: string,
    public readonly status: number | null,
  ) {
    super(message);
    this.name = "ApiRequestError";
  }
}

export class RefreshSessionExpiredError extends Error {
  constructor(message = "refresh token is missing or expired") {
    super(message);
    this.name = "RefreshSessionExpiredError";
  }
}

export class AuthenticationSessionChangedError extends Error {
  constructor(message = "authentication session changed") {
    super(message);
    this.name = "AuthenticationSessionChangedError";
  }
}

type AccessTokenRefreshHandler = (
  reason: AccessTokenRefreshReason,
) => Promise<void>;
type ClearRefreshSessionHandler = () => Promise<void>;
type AuthenticationExpirationListener = (
  reason: AuthenticationExpirationReason,
) => void;

const authenticationExpirationListeners =
  new Set<AuthenticationExpirationListener>();

export function setAccessToken(
  token: string | null,
  expiresInSeconds?: number | string | bigint,
) {
  clearAccessTokenExpirationTimer();
  accessToken = null;
  accessTokenExpiresAtMs = null;
  accessTokenRefreshAtMs = null;

  if (!token) {
    console.info("[api-client] access token cleared");
    return;
  }

  const ttlSeconds = normalizeAccessTokenTtl(expiresInSeconds);
  const ttlMs = ttlSeconds * 1000;
  const refreshLeewayMs = Math.min(
    MAX_ACCESS_TOKEN_REFRESH_LEEWAY_MS,
    Math.max(250, Math.floor(ttlMs * 0.1)),
  );
  accessToken = token;
  accessTokenExpiresAtMs = Date.now() + ttlMs;
  accessTokenRefreshAtMs = accessTokenExpiresAtMs - refreshLeewayMs;
  authenticationExpirationNotified = false;
  console.info("[api-client] access token stored in memory", {
    ttlSeconds,
    expiresAt: new Date(accessTokenExpiresAtMs).toISOString(),
    refreshAt: new Date(accessTokenRefreshAtMs).toISOString(),
  });
  scheduleAccessTokenExpiration();
}

export function hasAccessToken(): boolean {
  return Boolean(accessToken);
}

/** 长连接建立前也必须经过同一个 single-flight 刷新通道，不能读取过期 token。 */
export async function getRealtimeAccessToken(): Promise<string> {
  const result = await refreshAccessTokenIfNeeded("request");
  if (result !== "ready" || !accessToken) {
    throw new Error("登录凭证已过期，请重新登录");
  }
  return accessToken;
}

export function configureAuthSessionHandlers(handlers: {
  refreshAccessToken: AccessTokenRefreshHandler;
  clearRefreshSession: ClearRefreshSessionHandler;
}): void {
  refreshAccessTokenHandler = handlers.refreshAccessToken;
  clearRefreshSessionHandler = handlers.clearRefreshSession;
  console.info("[api-client] auth session handlers configured");
}

export async function refreshAccessTokenIfNeeded(
  reason: AccessTokenRefreshReason,
): Promise<AccessTokenRefreshResult> {
  if (isAccessTokenFresh()) {
    return "ready";
  }
  return refreshAccessToken(reason, false);
}

export function subscribeAuthenticationExpired(
  listener: AuthenticationExpirationListener,
): () => void {
  authenticationExpirationListeners.add(listener);
  return () => {
    authenticationExpirationListeners.delete(listener);
  };
}

async function refreshAccessToken(
  reason: AccessTokenRefreshReason,
  force: boolean,
): Promise<AccessTokenRefreshResult> {
  if (!force && isAccessTokenFresh()) {
    return "ready";
  }
  if (accessTokenRefreshPromise) {
    console.info("[api-client] joining in-flight access token refresh", {
      reason,
    });
    return accessTokenRefreshPromise;
  }

  accessTokenRefreshPromise = (async () => {
    try {
      if (!refreshAccessTokenHandler) {
        throw new Error("refresh token handler is not configured");
      }
      console.info("[api-client] refreshing access token", { reason });
      await refreshAccessTokenHandler(reason);
      if (!accessToken || accessTokenExpiresAtMs === null) {
        throw new Error("refresh completed without a new access token");
      }
      console.info("[api-client] access token refresh completed", { reason });
      return "ready";
    } catch (error) {
      console.warn("[api-client] access token refresh failed", {
        reason,
        error: error instanceof Error ? error.message : String(error),
      });
      if (error instanceof AuthenticationSessionChangedError) {
        // 用户主动注销或建立了新会话时，在途刷新结果必须被丢弃。这不是凭证
        // 过期，不触发全局“登录已过期”提示，也不安排后台重试。
        console.info("[api-client] obsolete token refresh discarded", {
          reason,
        });
        return "session_expired";
      }
      if (isRefreshCredentialRejection(error)) {
        invalidateAuthentication("refresh_failed");
        return "session_expired";
      }

      // 网络中断或服务端 5xx 不代表 refresh token 已失效。保留安全存储中的
      // 会话并安排短间隔重试，避免用户在临时离线时被错误登出。
      scheduleAccessTokenRefreshRetry();
      // 普通的提前刷新失败时，尚未真正过期的 access token 仍可继续使用；但如果
      // 本次刷新是由服务端 401 触发，则服务端已经明确拒绝了旧 token，不能再拿
      // 同一个 token 重试原请求，否则第二个 401 会被错误解释为整个会话失效。
      return !force && isAccessTokenNotExpired()
        ? "ready"
        : "temporarily_unavailable";
    } finally {
      accessTokenRefreshPromise = null;
    }
  })();

  return accessTokenRefreshPromise;
}

function invalidateAuthentication(reason: AuthenticationExpirationReason): void {
  clearAccessTokenExpirationTimer();
  accessToken = null;
  accessTokenExpiresAtMs = null;
  accessTokenRefreshAtMs = null;
  void clearRefreshSessionHandler?.().catch((error) => {
    console.warn("[api-client] refresh session cleanup failed", {
      error: error instanceof Error ? error.message : String(error),
    });
  });

  if (authenticationExpirationNotified) {
    return;
  }
  authenticationExpirationNotified = true;
  console.info("[api-client] authentication expired", { reason });
  for (const listener of authenticationExpirationListeners) {
    try {
      listener(reason);
    } catch (error) {
      console.warn("[api-client] authentication expiration listener failed", {
        reason,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }
}

function scheduleAccessTokenExpiration(): void {
  if (!accessToken || accessTokenRefreshAtMs === null) {
    return;
  }
  const remainingMs = accessTokenRefreshAtMs - Date.now();
  if (remainingMs <= 0) {
    void refreshAccessToken("timer", false);
    return;
  }

  accessTokenExpirationTimer = setTimeout(() => {
    accessTokenExpirationTimer = null;
    // 超长 TTL 会被拆成多个平台允许的最大定时器，避免 32 位延迟溢出。
    if (isAccessTokenFresh()) {
      scheduleAccessTokenExpiration();
    } else {
      void refreshAccessToken("timer", false);
    }
  }, Math.min(remainingMs, MAX_TIMER_DELAY_MS));
}

function isAccessTokenFresh(): boolean {
  return Boolean(
    accessToken &&
      accessTokenRefreshAtMs !== null &&
      Date.now() < accessTokenRefreshAtMs,
  );
}

function isAccessTokenNotExpired(): boolean {
  return Boolean(
    accessToken &&
      accessTokenExpiresAtMs !== null &&
      Date.now() < accessTokenExpiresAtMs,
  );
}

function scheduleAccessTokenRefreshRetry(): void {
  clearAccessTokenExpirationTimer();
  accessTokenExpirationTimer = setTimeout(() => {
    accessTokenExpirationTimer = null;
    void refreshAccessToken("timer", true);
  }, REFRESH_RETRY_DELAY_MS);
}

function isRefreshCredentialRejection(error: unknown): boolean {
  return (
    error instanceof RefreshSessionExpiredError ||
    (error instanceof ApiRequestError &&
      (error.status === 400 || error.status === 401))
  );
}

function clearAccessTokenExpirationTimer(): void {
  if (accessTokenExpirationTimer) {
    clearTimeout(accessTokenExpirationTimer);
    accessTokenExpirationTimer = null;
  }
}

function normalizeAccessTokenTtl(
  value: number | string | bigint | undefined,
): number {
  const ttlSeconds = Number(value);
  if (!Number.isSafeInteger(ttlSeconds) || ttlSeconds <= 0) {
    throw new Error("登录响应中的 access token 有效期无效");
  }
  return ttlSeconds;
}

function normalizeBaseUrl(value: string | null | undefined): string | null {
  const trimmed = value?.trim();
  if (!trimmed) {
    return null;
  }
  return trimmed.replace(/\/+$/, "");
}

function requireApiBaseUrl(): string {
  if (!API_BASE_URL) {
    throw new Error("缺少 EXPO_PUBLIC_API_BASE_URL，请按当前调试环境手动配置 API 地址");
  }
  return API_BASE_URL;
}

function networkErrorMessage(
  error: unknown,
  path: string,
  apiBaseUrl: string,
): string {
  const message = error instanceof Error ? error.message : String(error);
  console.warn("[api-client] network request failed", {
    apiBaseUrl,
    path,
    reason: message,
  });
  return `网络请求失败：无法连接到 ${apiBaseUrl}`;
}

async function readResponseText(resp: Response): Promise<string> {
  try {
    return await resp.text();
  } catch {
    return "";
  }
}

async function readError(resp: Response): Promise<string> {
  const text = await readResponseText(resp);
  if (!text.trim()) {
    return `请求失败 (${resp.status})`;
  }

  const contentType = resp.headers.get("content-type") ?? "";
  if (contentType.includes("application/json")) {
    try {
      const data = JSON.parse(text) as unknown;
      if (typeof data === "string" && data.trim()) {
        return data.trim();
      }
      if (data && typeof data === "object") {
        const candidate = data as Record<string, unknown>;
        const msgKeys = ["message", "msg", "error", "reason"];
        for (const key of msgKeys) {
          const value = candidate[key];
          if (typeof value === "string" && value.trim()) {
            return value.trim();
          }
        }
      }
    } catch {
      // Fall through to the raw response text below.
    }
  }

  return text.trim();
}

function parseJsonResponse<TResp>(text: string): TResp {
  if (!text.trim()) {
    return undefined as TResp;
  }

  try {
    return JSON.parse(text) as TResp;
  } catch {
    throw new Error("响应内容不是有效 JSON");
  }
}

export type RequestJsonOptions<TReq> = {
  method?: "GET" | "POST" | "PUT" | "PATCH" | "DELETE";
  body?: TReq;
  headers?: Record<string, string>;
  auth?: boolean;
};

export async function requestJson<TReq, TResp>(
  path: string,
  options: RequestJsonOptions<TReq> = {},
): Promise<TResp> {
  return requestJsonAttempt<TReq, TResp>(path, options, false);
}

async function requestJsonAttempt<TReq, TResp>(
  path: string,
  options: RequestJsonOptions<TReq>,
  retriedAfterRefresh: boolean,
): Promise<TResp> {
  const { method = "GET", body, headers: extraHeaders = {}, auth = false } = options;
  const apiBaseUrl = requireApiBaseUrl();
  const headers: Record<string, string> = { ...extraHeaders };
  let requestAccessToken: string | null = null;

  if (body !== undefined) {
    headers["Content-Type"] = "application/json";
  }
  if (auth) {
    const refreshResult = await refreshAccessTokenIfNeeded("request");
    if (refreshResult === "temporarily_unavailable") {
      throw new Error("暂时无法刷新登录凭证，请检查网络后重试");
    }
    if (refreshResult === "session_expired" || !accessToken) {
      throw new Error("登录凭证已过期，请重新登录");
    }
    requestAccessToken = accessToken;
    headers["Authorization"] = `Bearer ${requestAccessToken}`;
  }

  console.info("[api-client] request", {
    method,
    path,
    auth,
    retriedAfterRefresh,
    apiBaseUrl,
  });

  let resp: Response;
  try {
    const init: RequestInit = {
      method,
      headers,
    };
    if (body !== undefined) {
      init.body = JSON.stringify(body, (_key, value) =>
        typeof value === "bigint" ? value.toString() : value,
      );
    }
    resp = await fetch(`${apiBaseUrl}${path}`, init);
  } catch (error) {
    throw new Error(networkErrorMessage(error, path, apiBaseUrl));
  }

  if (!resp.ok) {
    const reason = await readError(resp);
    console.warn("[api-client] request failed", {
      method,
      path,
      status: resp.status,
      reason,
    });
    if (auth && resp.status === 401 && !retriedAfterRefresh) {
      // 如果另一个并发请求已经完成刷新，直接使用新 access token 重试；否则加入
      // 全局单飞刷新，避免多个 401 同时消费同一个一次性 refresh token。
      const anotherRequestRefreshed =
        Boolean(accessToken) && accessToken !== requestAccessToken;
      const refreshResult = anotherRequestRefreshed
        ? "ready"
        : await refreshAccessToken("server_unauthorized", true);
      if (refreshResult === "ready") {
        console.info("[api-client] retrying request after token refresh", {
          method,
          path,
          anotherRequestRefreshed,
        });
        return requestJsonAttempt<TReq, TResp>(path, options, true);
      }
    } else if (auth && resp.status === 401 && retriedAfterRefresh) {
      // 新 token 仍被服务端拒绝时不再循环刷新，立即终止整个登录会话。
      invalidateAuthentication("server_unauthorized_after_refresh");
    }
    throw new ApiRequestError(reason, resp.status);
  }

  if (resp.status === 204 || resp.status === 205) {
    return undefined as TResp;
  }

  const text = await readResponseText(resp);
  return parseJsonResponse<TResp>(text);
}
