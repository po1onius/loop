import { Platform } from "react-native";

const API_BASE_URL =
  process.env.EXPO_PUBLIC_API_BASE_URL?.trim() ||
  (Platform.OS === "android"
    ? "http://10.0.2.2:3000/loop"
    : "http://127.0.0.1:3000/loop");

let accessToken: string | null = null;

export function setAccessToken(token: string | null) {
  accessToken = token;
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
  method?: "GET" | "POST";
  body?: TReq;
  auth?: boolean;
};

export async function requestJson<TReq, TResp>(
  path: string,
  options: RequestJsonOptions<TReq> = {},
): Promise<TResp> {
  const { method = "GET", body, auth = false } = options;
  const headers: Record<string, string> = {};

  if (body !== undefined) {
    headers["Content-Type"] = "application/json";
  }
  if (auth) {
    if (!accessToken) {
      throw new Error("请先登录");
    }
    headers.Authorization = `Bearer ${accessToken}`;
  }

  const resp = await fetch(`${API_BASE_URL}${path}`, {
    method,
    headers,
    body:
      body === undefined
        ? undefined
        : JSON.stringify(body, (_key, value) =>
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
