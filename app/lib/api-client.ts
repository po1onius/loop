const API_BASE_URL = normalizeBaseUrl(process.env.EXPO_PUBLIC_API_BASE_URL);

let accessToken: string | null = null;

export function setAccessToken(token: string | null) {
  accessToken = token;
}

export function hasAccessToken(): boolean {
  return Boolean(accessToken);
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
  method?: "GET" | "POST" | "PATCH" | "DELETE";
  body?: TReq;
  headers?: Record<string, string>;
  auth?: boolean;
};

export async function requestJson<TReq, TResp>(
  path: string,
  options: RequestJsonOptions<TReq> = {},
): Promise<TResp> {
  const { method = "GET", body, headers: extraHeaders = {}, auth = false } = options;
  const apiBaseUrl = requireApiBaseUrl();
  const headers: Record<string, string> = { ...extraHeaders };

  if (body !== undefined) {
    headers["Content-Type"] = "application/json";
  }
  if (auth) {
    if (!accessToken) {
      throw new Error("请先登录");
    }
    headers["Authorization"] = `Bearer ${accessToken}`;
  }

  console.info("[api-client] request", {
    method,
    path,
    auth,
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
    throw new Error(reason);
  }

  if (resp.status === 204 || resp.status === 205) {
    return undefined as TResp;
  }

  const text = await readResponseText(resp);
  return parseJsonResponse<TResp>(text);
}
