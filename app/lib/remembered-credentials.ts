import * as SecureStore from "expo-secure-store";

export type RememberedCredentials = {
  account: string;
  password: string;
};

type StoredCredentials = RememberedCredentials & {
  version: 1;
};

const CREDENTIALS_KEY = "loop.login.remembered_credentials";
const SECURE_STORE_OPTIONS: SecureStore.SecureStoreOptions = {
  // 使用独立的 Keychain service，避免登录凭据与后续其他安全数据共用命名空间。
  keychainService: "loop.login.credentials",
  keychainAccessible: SecureStore.WHEN_UNLOCKED_THIS_DEVICE_ONLY,
};

export async function isRememberPasswordAvailable(): Promise<boolean> {
  const available = await SecureStore.isAvailableAsync();
  console.info("[remembered-credentials] secure storage availability checked", {
    available,
  });
  return available;
}

export async function loadRememberedCredentials(): Promise<RememberedCredentials | null> {
  await requireSecureStorage();
  const stored = await SecureStore.getItemAsync(
    CREDENTIALS_KEY,
    SECURE_STORE_OPTIONS,
  );
  if (!stored) {
    console.info("[remembered-credentials] no saved credentials found");
    return null;
  }

  try {
    const parsed = JSON.parse(stored) as Partial<StoredCredentials>;
    if (
      parsed.version !== 1 ||
      typeof parsed.account !== "string" ||
      !parsed.account.trim() ||
      typeof parsed.password !== "string" ||
      !parsed.password
    ) {
      throw new Error("saved credential payload is invalid");
    }
    console.info("[remembered-credentials] saved credentials loaded");
    return {
      account: parsed.account,
      password: parsed.password,
    };
  } catch (error) {
    // 无效内容不能继续用于自动填充；立即清理，避免每次进入登录页重复解析失败。
    console.warn("[remembered-credentials] invalid saved credentials removed", {
      reason: error instanceof Error ? error.message : String(error),
    });
    await SecureStore.deleteItemAsync(CREDENTIALS_KEY, SECURE_STORE_OPTIONS);
    return null;
  }
}

export async function saveRememberedCredentials(
  credentials: RememberedCredentials,
): Promise<void> {
  await requireSecureStorage();
  const payload: StoredCredentials = {
    version: 1,
    account: credentials.account.trim(),
    password: credentials.password,
  };
  await SecureStore.setItemAsync(
    CREDENTIALS_KEY,
    JSON.stringify(payload),
    SECURE_STORE_OPTIONS,
  );
  // 日志只说明操作结果，严禁记录账号或密码内容。
  console.info("[remembered-credentials] credentials saved securely");
}

export async function clearRememberedCredentials(): Promise<void> {
  await requireSecureStorage();
  await SecureStore.deleteItemAsync(CREDENTIALS_KEY, SECURE_STORE_OPTIONS);
  console.info("[remembered-credentials] saved credentials cleared");
}

async function requireSecureStorage(): Promise<void> {
  if (!(await SecureStore.isAvailableAsync())) {
    throw new Error("当前平台不支持安全保存密码");
  }
}
