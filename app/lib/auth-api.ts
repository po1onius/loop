import { Platform } from "react-native";

type LoginResponse = {
  access_token: string;
  expires_in: number;
  refresh_token: string;
  refresh_exp: number;
};

type VerifyCodeResponse = {
  code: string;
};

const API_BASE_URL =
  process.env.EXPO_PUBLIC_API_BASE_URL?.trim() ||
  (Platform.OS === "android" ? "http://10.0.2.2:3000" : "http://127.0.0.1:3000");

async function readError(resp: Response): Promise<string> {
  try {
    const data = await resp.json();
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
    // ignore
  }

  try {
    const text = await resp.text();
    if (text.trim()) {
      return text.trim();
    }
  } catch {
    // ignore
  }

  return `请求失败 (${resp.status})`;
}

async function postJson<TReq, TResp>(path: string, body: TReq): Promise<TResp> {
  const resp = await fetch(`${API_BASE_URL}${path}`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify(body),
  });

  if (!resp.ok) {
    throw new Error(await readError(resp));
  }

  if (resp.status === 204) {
    return undefined as TResp;
  }

  return (await resp.json()) as TResp;
}

export function login(account: string, password: string) {
  return postJson<{ account: string; password: string }, LoginResponse>("/user/login", {
    account,
    password,
  });
}

export function verifyCode(account: string) {
  return postJson<{ account: string }, VerifyCodeResponse>("/user/verify_code", {
    account,
  });
}

export function register(params: {
  username: string;
  account: string;
  pwd: string;
  verify_code: string;
}) {
  return postJson<typeof params, void>("/user/register", params);
}
