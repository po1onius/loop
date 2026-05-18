import type {
  CompleteMediaUploadResp,
  CreateMediaUploadRequest,
  CreateMediaUploadResp,
  MediaAssetResp,
  PresignedHeader,
} from "@/lib/dto";
import { requestJson } from "@/lib/api-client";

export type UploadLocalImageInput = {
  uri: string;
  mimeType?: string | null;
  fileName?: string | null;
  width?: number | null;
  height?: number | null;
  file?: Blob | null;
};

const MAX_BACKEND_UPLOAD_BYTES = 2_147_483_647;

export async function uploadLocalImageAsset(
  input: UploadLocalImageInput,
): Promise<MediaAssetResp> {
  const blob = await readLocalBlob(input);
  const mimeType = normalizeImageMimeType(
    input.mimeType ?? blob.type,
    input.fileName ?? input.uri,
  );
  const byteSize = normalizeByteSize(blob.size);
  const width = normalizeDimension(input.width);
  const height = normalizeDimension(input.height);

  console.info("[media-api] requesting media upload url", {
    mimeType,
    byteSize,
    width,
    height,
  });

  const upload = await requestJson<
    CreateMediaUploadRequest,
    CreateMediaUploadResp
  >("/media/upload_url", {
    method: "POST",
    auth: true,
    body: {
      mime_type: mimeType,
      byte_size: byteSize,
      width,
      height,
    },
  });

  await putPresignedObject(upload, blob, mimeType);

  console.info("[media-api] completing media upload", {
    assetId: upload.asset_id,
  });
  const completed = await requestJson<undefined, CompleteMediaUploadResp>(
    `/media/${encodeURIComponent(upload.asset_id)}/complete`,
    {
      method: "POST",
      auth: true,
    },
  );

  return completed.asset;
}

async function readLocalBlob(input: UploadLocalImageInput): Promise<Blob> {
  if (input.file instanceof Blob) {
    return input.file;
  }

  const resp = await fetch(input.uri);
  if (!resp.ok) {
    const text = await safeReadResponseText(resp);
    console.warn("[media-api] failed to read local image", {
      status: resp.status,
      reason: text,
    });
    throw new Error("读取本地图片失败");
  }

  return resp.blob();
}

async function putPresignedObject(
  upload: CreateMediaUploadResp,
  blob: Blob,
  mimeType: string,
) {
  const headers = toUploadHeaders(upload.upload_headers, mimeType);
  const uploadMethod = upload.upload_method || "PUT";
  const uploadOrigin = safeUrlOrigin(upload.upload_url);

  console.info("[media-api] uploading media object", {
    assetId: upload.asset_id,
    method: uploadMethod,
    uploadOrigin,
    byteSize: blob.size,
  });

  let resp: Response;
  try {
    resp = await fetch(upload.upload_url, {
      method: uploadMethod,
      headers,
      body: blob,
    });
  } catch (error) {
    const reason = error instanceof Error ? error.message : String(error);
    console.warn("[media-api] media upload network failed", {
      assetId: upload.asset_id,
      uploadOrigin,
      reason,
    });
    throw new Error(`图片上传失败：无法连接到对象存储 ${uploadOrigin}`);
  }

  if (!resp.ok) {
    const text = await safeReadResponseText(resp);
    console.warn("[media-api] media upload put failed", {
      assetId: upload.asset_id,
      uploadOrigin,
      status: resp.status,
      reason: text,
    });
    throw new Error(text || `图片上传失败 (${resp.status})`);
  }
}

function safeUrlOrigin(value: string): string {
  try {
    return new URL(value).origin;
  } catch {
    return "<invalid-upload-url>";
  }
}

function toUploadHeaders(
  headers: PresignedHeader[],
  mimeType: string,
): Record<string, string> {
  const result: Record<string, string> = {};
  let hasContentType = false;
  for (const header of headers) {
    const lowerName = header.name.toLowerCase();
    if (lowerName === "host" || lowerName === "content-length") {
      continue;
    }
    result[header.name] = header.value;
    if (lowerName === "content-type") {
      hasContentType = true;
    }
  }

  // 后端签名 PUT 时会绑定 content-type；如果 S3 SDK 没有返回该 header，
  // 仍显式带上本次登记的 MIME，避免对象元数据和后端校验不一致。
  if (!hasContentType) {
    result["Content-Type"] = mimeType;
  }
  return result;
}

function normalizeImageMimeType(
  value: string | null | undefined,
  fallbackName: string,
): string {
  const normalized = value?.trim().toLowerCase();
  if (normalized?.startsWith("image/")) {
    return normalized;
  }

  const lowerName = fallbackName.toLowerCase();
  if (lowerName.endsWith(".png")) {
    return "image/png";
  }
  if (lowerName.endsWith(".webp")) {
    return "image/webp";
  }
  if (lowerName.endsWith(".gif")) {
    return "image/gif";
  }
  return "image/jpeg";
}

function normalizeByteSize(value: number | null | undefined): number {
  const byteSize = Math.trunc(value ?? 0);
  if (byteSize <= 0 || byteSize > MAX_BACKEND_UPLOAD_BYTES) {
    throw new Error("图片大小不符合上传限制");
  }
  return byteSize;
}

function normalizeDimension(value: number | null | undefined): number | null {
  const dimension = Math.trunc(value ?? 0);
  return dimension > 0 ? dimension : null;
}

async function safeReadResponseText(resp: Response): Promise<string> {
  try {
    return (await resp.text()).trim();
  } catch {
    return "";
  }
}
