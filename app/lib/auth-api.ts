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
  AuthenticationSessionChangedError,
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
let authSessionGeneration = 0;

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
  // 新登录会话取代之前可能仍在返回途中的刷新操作。
  authSessionGeneration += 1;
  await applyTokenPair(normalized, authSessionGeneration);
  return normalized;
}

export async function refreshToken(
  params: RefreshTokenRequest,
): Promise<LoginResp> {
  const expectedGeneration = authSessionGeneration;
  const resp = await requestJson<RefreshTokenRequest, LoginResp>(
    "/user/refresh_token",
    {
      method: "POST",
      body: params,
    },
  );
  const normalized = normalizeLoginResp(resp);
  await applyTokenPair(normalized, expectedGeneration);
  return normalized;
}

export async function logout(): Promise<void> {
  const session = await ensureRefreshSessionLoaded();

  // 先使当前代次失效并清除内存 token，保证点击注销后不会再发出新的鉴权请求；
  // 已经在途的刷新响应也会因代次不匹配而被丢弃。
  authSessionGeneration += 1;
  currentRefreshSession = null;
  refreshSessionLoaded = true;
  setAccessToken(null);
  console.info("[auth-api] local authentication session invalidated for logout", {
    hasRefreshToken: Boolean(session),
  });

  try {
    // 先清安全存储再访问网络，避免请求期间进程被系统终止后，下次启动又恢复
    // 已经选择注销的会话。服务端撤销仍使用上方保留在内存中的 token 副本。
    await clearStoredRefreshSession();
  } catch (error) {
    // 清理失败时恢复内存中的 refresh session，使用户可以再次点击注销；不能在
    // 磁盘凭证仍存在的情况下对 UI 谎报注销成功。
    currentRefreshSession = session;
    console.warn("[auth-api] local refresh session cleanup failed", {
      reason: error instanceof Error ? error.message : String(error),
    });
    throw error;
  }

  let remoteLogoutError: unknown = null;
  try {
    if (session) {
      await requestJson<RefreshTokenRequest, void>("/user/logout", {
        method: "POST",
        body: { refresh_token: session.refreshToken },
      });
      console.info("[auth-api] server authentication session revoked");
    }
  } catch (error) {
    // 本机注销不能被临时网络问题阻断；服务端 token 最迟会按 refresh TTL
    // 自动过期，且本机已经不再持有其明文。完整原因保留在日志中便于排查。
    remoteLogoutError = error;
    console.warn("[auth-api] server session revocation failed", {
      reason: error instanceof Error ? error.message : String(error),
    });
  }

  console.info("[auth-api] logout completed", {
    serverSessionRevoked: remoteLogoutError === null,
  });
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

async function applyTokenPair(
  resp: LoginResp,
  expectedGeneration: number,
): Promise<void> {
  if (expectedGeneration !== authSessionGeneration) {
    throw new AuthenticationSessionChangedError();
  }
  const refreshTtlSeconds = normalizePositiveTtl(
    resp.refresh_exp,
    "refresh_exp",
  );
  const nextRefreshSession: StoredRefreshSession = {
    refreshToken: resp.refresh_token,
    refreshExpiresAtMs: Date.now() + refreshTtlSeconds * 1000,
  };

  try {
    // refresh token 每次使用都会轮换，必须用新值覆盖安全存储中的旧值。
    await saveStoredRefreshSession(nextRefreshSession);
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

  if (expectedGeneration !== authSessionGeneration) {
    // 持久化期间若发生注销，删除刚写入的过期代次；若已经建立了更新会话，
    // 则重新写入当前会话，避免较晚完成的旧写操作覆盖新 refresh token。
    if (currentRefreshSession) {
      await saveStoredRefreshSession(currentRefreshSession);
    } else {
      await clearStoredRefreshSession();
    }
    throw new AuthenticationSessionChangedError();
  }

  currentRefreshSession = nextRefreshSession;
  refreshSessionLoaded = true;
  setAccessToken(resp.access_token, resp.expires_in);
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
  authSessionGeneration += 1;
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
