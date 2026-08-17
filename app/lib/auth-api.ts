import type {
  CurrentUserResp,
  LoginRequest,
  LoginResp,
  RefreshTokenRequest,
  RegisterRequest,
  UpdateUserAvatarRequest,
  VerifyCodeRequest,
  VerifyCodeResp,
} from "@/lib/dto";
import {
  configureAuthSessionHandlers,
  hasAccessToken,
  refreshAccessTokenIfNeeded,
  RefreshSessionExpiredError,
  requestJson,
  setAccessToken,
  type AccessTokenRefreshReason,
} from "@/lib/api-client";
import {
  clearStoredRefreshSession,
  loadStoredRefreshSession,
  saveStoredRefreshSession,
  type StoredRefreshSession,
} from "@/lib/auth-session-storage";

let currentRefreshSession: StoredRefreshSession | null = null;
let refreshSessionLoaded = false;
let refreshSessionLoadPromise: Promise<StoredRefreshSession | null> | null = null;

function toBigInt(value: number | string | bigint, field: string): bigint {
  try {
    return BigInt(value);
  } catch {
    throw new Error(`响应字段 ${field} 不是有效整数`);
  }
}

function normalizeLoginResp(resp: LoginResp): LoginResp {
  return {
    ...resp,
    expires_in: toBigInt(resp.expires_in, "expires_in"),
    refresh_exp: toBigInt(resp.refresh_exp, "refresh_exp"),
  };
}

export async function login(params: LoginRequest): Promise<LoginResp> {
  const resp = await requestJson<LoginRequest, LoginResp>("/user/login", {
    method: "POST",
    body: params,
  });
  const normalized = normalizeLoginResp(resp);
  await applyTokenPair(normalized);
  return normalized;
}

export async function refreshToken(
  params: RefreshTokenRequest,
): Promise<LoginResp> {
  const resp = await requestJson<RefreshTokenRequest, LoginResp>(
    "/user/refresh_token",
    {
      method: "POST",
      body: params,
    },
  );
  const normalized = normalizeLoginResp(resp);
  await applyTokenPair(normalized);
  return normalized;
}

export async function restoreAuthSession(): Promise<boolean> {
  if (hasAccessToken()) {
    return true;
  }

  const session = await ensureRefreshSessionLoaded();
  if (!session) {
    console.info("[auth-api] no refresh session to restore");
    return false;
  }
  if (session.refreshExpiresAtMs <= Date.now()) {
    console.info("[auth-api] persisted refresh session expired locally");
    await clearRefreshSession();
    return false;
  }

  // 冷启动恢复、前台唤醒、定时刷新和接口 401 必须共用 api-client 中的
  // single-flight 通道，否则两个并发刷新会重复消费同一枚一次性 refresh token。
  const refreshResult = await refreshAccessTokenIfNeeded("app_foreground");
  if (refreshResult === "ready") {
    console.info("[auth-api] auth session restored by refresh token");
    return true;
  }

  if (refreshResult === "session_expired") {
    // api-client 已触发异步清理；此处等待同一个幂等清理完成，避免冷启动流程
    // 返回后磁盘上仍短暂保留已失效的 refresh token。
    await clearRefreshSession();
  } else {
    // 冷启动遇到离线或 5xx 时保留 refresh token；后续回到前台会再次尝试，
    // 不把临时网络故障当成会话失效。
    console.info("[auth-api] refresh session retained for later retry");
  }
  return false;
}

export function hasRefreshSession(): boolean {
  return Boolean(
    currentRefreshSession &&
      currentRefreshSession.refreshExpiresAtMs > Date.now(),
  );
}

async function refreshAccessTokenFromSession(
  reason: AccessTokenRefreshReason,
): Promise<void> {
  const session = await ensureRefreshSessionLoaded();
  if (!session || session.refreshExpiresAtMs <= Date.now()) {
    throw new RefreshSessionExpiredError();
  }
  console.info("[auth-api] rotating refresh token", { reason });
  await refreshToken({ refresh_token: session.refreshToken });
}

async function applyTokenPair(resp: LoginResp): Promise<void> {
  const refreshTtlSeconds = normalizePositiveTtl(
    resp.refresh_exp,
    "refresh_exp",
  );
  currentRefreshSession = {
    refreshToken: resp.refresh_token,
    refreshExpiresAtMs: Date.now() + refreshTtlSeconds * 1000,
  };
  refreshSessionLoaded = true;
  setAccessToken(resp.access_token, resp.expires_in);

  try {
    // refresh token 每次使用都会轮换，必须用新值覆盖安全存储中的旧值。
    await saveStoredRefreshSession(currentRefreshSession);
  } catch (error) {
    console.warn("[auth-api] rotated refresh session persistence failed", {
      reason: error instanceof Error ? error.message : String(error),
    });
    // 旧 refresh token 已被服务端消费，保存失败时清除磁盘旧值，避免应用重启后
    // 把旧 token 当成重放请求并触发整个 token family 撤销。
    await clearStoredRefreshSession().catch((clearError) => {
      console.warn("[auth-api] stale refresh session cleanup failed", {
        reason:
          clearError instanceof Error ? clearError.message : String(clearError),
      });
    });
  }
}

async function ensureRefreshSessionLoaded(): Promise<StoredRefreshSession | null> {
  if (refreshSessionLoaded) {
    return currentRefreshSession;
  }
  if (refreshSessionLoadPromise) {
    return refreshSessionLoadPromise;
  }

  refreshSessionLoadPromise = loadStoredRefreshSession()
    .then((session) => {
      currentRefreshSession = session;
      refreshSessionLoaded = true;
      return session;
    })
    .finally(() => {
      refreshSessionLoadPromise = null;
    });
  return refreshSessionLoadPromise;
}

async function clearRefreshSession(): Promise<void> {
  currentRefreshSession = null;
  refreshSessionLoaded = true;
  setAccessToken(null);
  await clearStoredRefreshSession();
}

function normalizePositiveTtl(
  value: number | string | bigint,
  field: string,
): number {
  const ttlSeconds = Number(value);
  if (!Number.isSafeInteger(ttlSeconds) || ttlSeconds <= 0) {
    throw new Error(`响应字段 ${field} 不是有效的正整数`);
  }
  return ttlSeconds;
}

configureAuthSessionHandlers({
  refreshAccessToken: refreshAccessTokenFromSession,
  clearRefreshSession,
});

export function verifyCode(params: VerifyCodeRequest): Promise<VerifyCodeResp> {
  return requestJson<VerifyCodeRequest, VerifyCodeResp>("/user/verify_code", {
    method: "POST",
    body: params,
  });
}

export function register(params: RegisterRequest): Promise<void> {
  return requestJson<RegisterRequest, void>("/user/register", {
    method: "POST",
    body: params,
  });
}

export function getCurrentUser(): Promise<CurrentUserResp> {
  console.info("[auth-api] loading current user profile");
  return requestJson<undefined, CurrentUserResp>("/me/profile", { auth: true });
}

export function updateCurrentUserAvatar(
  params: UpdateUserAvatarRequest,
): Promise<CurrentUserResp> {
  console.info("[auth-api] updating current user avatar", {
    avatarAssetId: params.avatar_asset_id,
  });
  return requestJson<UpdateUserAvatarRequest, CurrentUserResp>(
    "/me/profile/avatar",
    { method: "PATCH", auth: true, body: params },
  );
}

export type {
  CurrentUserResp,
  LoginRequest,
  LoginResp,
  RefreshTokenRequest,
  RegisterRequest,
  UpdateUserAvatarRequest,
  VerifyCodeRequest,
  VerifyCodeResp,
};
