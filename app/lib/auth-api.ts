import type {
  LoginRequest,
  LoginResp,
  RefreshTokenRequest,
  RegisterRequest,
  VerifyCodeRequest,
  VerifyCodeResp,
} from "@/lib/dto";
import { requestJson, setAccessToken } from "@/lib/api-client";

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
  setAccessToken(normalized.access_token);
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
  setAccessToken(normalized.access_token);
  return normalized;
}

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

export type {
  LoginRequest,
  LoginResp,
  RefreshTokenRequest,
  RegisterRequest,
  VerifyCodeRequest,
  VerifyCodeResp,
};
