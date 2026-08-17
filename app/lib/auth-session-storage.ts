import * as SecureStore from "expo-secure-store";

export type StoredRefreshSession = {
  refreshToken: string;
  refreshExpiresAtMs: number;
};

type StoredRefreshSessionPayload = StoredRefreshSession & {
  version: 1;
};

const REFRESH_SESSION_KEY = "loop.auth.refresh_session";
const SECURE_STORE_OPTIONS: SecureStore.SecureStoreOptions = {
  keychainService: "loop.auth.session",
  keychainAccessible: SecureStore.WHEN_UNLOCKED_THIS_DEVICE_ONLY,
};

export async function loadStoredRefreshSession(): Promise<StoredRefreshSession | null> {
  if (!(await SecureStore.isAvailableAsync())) {
    // Web 不把 refresh token 写入 localStorage，避免可被页面脚本直接读取；该平台
    // 仅在当前页面生命周期内维持会话。
    console.info("[auth-session-storage] secure storage unavailable");
    return null;
  }

  const raw = await SecureStore.getItemAsync(
    REFRESH_SESSION_KEY,
    SECURE_STORE_OPTIONS,
  );
  if (!raw) {
    console.info("[auth-session-storage] no persisted refresh session");
    return null;
  }

  try {
    const parsed = JSON.parse(raw) as Partial<StoredRefreshSessionPayload>;
    if (
      parsed.version !== 1 ||
      typeof parsed.refreshToken !== "string" ||
      !parsed.refreshToken ||
      typeof parsed.refreshExpiresAtMs !== "number" ||
      !Number.isSafeInteger(parsed.refreshExpiresAtMs) ||
      parsed.refreshExpiresAtMs <= 0
    ) {
      throw new Error("persisted refresh session payload is invalid");
    }
    console.info("[auth-session-storage] persisted refresh session loaded", {
      expiresAt: new Date(parsed.refreshExpiresAtMs).toISOString(),
    });
    return {
      refreshToken: parsed.refreshToken,
      refreshExpiresAtMs: parsed.refreshExpiresAtMs,
    };
  } catch (error) {
    console.warn("[auth-session-storage] invalid refresh session removed", {
      reason: error instanceof Error ? error.message : String(error),
    });
    await SecureStore.deleteItemAsync(
      REFRESH_SESSION_KEY,
      SECURE_STORE_OPTIONS,
    );
    return null;
  }
}

export async function saveStoredRefreshSession(
  session: StoredRefreshSession,
): Promise<void> {
  if (!(await SecureStore.isAvailableAsync())) {
    console.info("[auth-session-storage] refresh session kept in memory only");
    return;
  }

  const payload: StoredRefreshSessionPayload = {
    version: 1,
    ...session,
  };
  await SecureStore.setItemAsync(
    REFRESH_SESSION_KEY,
    JSON.stringify(payload),
    SECURE_STORE_OPTIONS,
  );
  console.info("[auth-session-storage] rotated refresh session persisted", {
    expiresAt: new Date(session.refreshExpiresAtMs).toISOString(),
  });
}

export async function clearStoredRefreshSession(): Promise<void> {
  if (!(await SecureStore.isAvailableAsync())) {
    return;
  }
  await SecureStore.deleteItemAsync(
    REFRESH_SESSION_KEY,
    SECURE_STORE_OPTIONS,
  );
  console.info("[auth-session-storage] persisted refresh session cleared");
}
