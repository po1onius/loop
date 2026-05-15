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

  if (resp.status === 204 || resp.status === 205) {
    return undefined as TResp;
  }

  const text = await readResponseText(resp);
  return parseJsonResponse<TResp>(text);
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
