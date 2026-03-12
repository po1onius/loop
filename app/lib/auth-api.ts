import type {
  LoginRequest,
  LoginResp,
  RefreshTokenRequest,
  RegisterRequest,
  VerifyCodeRequest,
  VerifyCodeResp,
} from "@/lib/dto";
import { Platform } from "react-native";

const API_BASE_URL =
  process.env.EXPO_PUBLIC_API_BASE_URL?.trim() ||
  (Platform.OS === "android"
    ? "http://10.0.2.2:3000/loop"
    : "http://127.0.0.1:3000/loop");

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
    body: JSON.stringify(body, (_key, value) =>
      typeof value === "bigint" ? value.toString() : value,
    ),
  });

  if (!resp.ok) {
    throw new Error(await readError(resp));
  }

  if (resp.status === 204) {
    return undefined as TResp;
  }

  return (await resp.json()) as TResp;
}

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
  const resp = await postJson<LoginRequest, LoginResp>("/user/login", params);
  return normalizeLoginResp(resp);
}

export async function refreshToken(
  params: RefreshTokenRequest,
): Promise<LoginResp> {
  const resp = await postJson<RefreshTokenRequest, LoginResp>(
    "/user/refresh_token",
    params,
  );
  return normalizeLoginResp(resp);
}

export function verifyCode(params: VerifyCodeRequest): Promise<VerifyCodeResp> {
  return postJson<VerifyCodeRequest, VerifyCodeResp>(
    "/user/verify_code",
    params,
  );
}

export function register(params: RegisterRequest): Promise<void> {
  return postJson<RegisterRequest, void>("/user/register", params);
}

export type {
  LoginRequest,
  LoginResp,
  RefreshTokenRequest,
  RegisterRequest,
  VerifyCodeRequest,
  VerifyCodeResp,
};
