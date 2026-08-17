import type {
  CommunityPostReactionResp,
  CommunityPostResp,
  CommunitySectionResp,
  CreateCommunityPostRequest,
  ListCommunityPostsResp,
  UpdateCommunityPostRequest,
} from "@/lib/dto";
import { requestJson } from "@/lib/api-client";

export type CommunityPostSort = "latest" | "active";

export function listCommunitySections(): Promise<CommunitySectionResp[]> {
  console.info("[community-api] loading community sections");
  return requestJson<undefined, CommunitySectionResp[]>("/community/sections", {
    auth: true,
  });
}

export async function listCommunityPosts(params: {
  sectionId?: string | null;
  sort?: CommunityPostSort;
  cursor?: string | null;
  limit?: number;
} = {}): Promise<ListCommunityPostsResp> {
  const query = new URLSearchParams({
    sort: params.sort ?? "latest",
    limit: String(params.limit ?? 20),
  });
  if (params.sectionId) {
    query.set("section_id", params.sectionId);
  }
  if (params.cursor) {
    query.set("cursor", params.cursor);
  }
  console.info("[community-api] loading community posts", {
    sectionId: params.sectionId ?? null,
    sort: params.sort ?? "latest",
    hasCursor: Boolean(params.cursor),
  });
  const resp = await requestJson<undefined, ListCommunityPostsResp>(
    `/community/posts?${query.toString()}`,
    { auth: true },
  );
  return normalizePostsResp(resp);
}

export async function listMyCommunityPosts(
  cursor?: string | null,
  limit = 20,
): Promise<ListCommunityPostsResp> {
  const query = new URLSearchParams({ limit: String(limit) });
  if (cursor) {
    query.set("cursor", cursor);
  }
  const resp = await requestJson<undefined, ListCommunityPostsResp>(
    `/me/community/posts?${query.toString()}`,
    { auth: true },
  );
  return normalizePostsResp(resp);
}

export async function getCommunityPost(
  postId: string,
): Promise<CommunityPostResp> {
  const resp = await requestJson<undefined, CommunityPostResp>(
    `/community/posts/${encodeURIComponent(postId)}`,
    { auth: true },
  );
  return normalizePost(resp);
}

export async function createCommunityPost(
  params: CreateCommunityPostRequest,
): Promise<CommunityPostResp> {
  console.info("[community-api] creating community post", {
    sectionId: params.section_id,
    postType: params.post_type,
    titleLength: params.title.trim().length,
    bodyLength: params.body.trim().length,
    imageCount: params.image_asset_ids.length,
  });
  const resp = await requestJson<
    CreateCommunityPostRequest,
    CommunityPostResp
  >("/community/posts", {
    method: "POST",
    auth: true,
    body: params,
  });
  return normalizePost(resp);
}

export async function updateCommunityPost(
  postId: string,
  params: UpdateCommunityPostRequest,
): Promise<CommunityPostResp> {
  const resp = await requestJson<
    UpdateCommunityPostRequest,
    CommunityPostResp
  >(`/community/posts/${encodeURIComponent(postId)}`, {
    method: "PATCH",
    auth: true,
    body: params,
  });
  return normalizePost(resp);
}

export function deleteCommunityPost(postId: string): Promise<void> {
  return requestJson<undefined, void>(
    `/community/posts/${encodeURIComponent(postId)}`,
    { method: "DELETE", auth: true },
  );
}

export async function setPostInterested(
  postId: string,
  interested: boolean,
): Promise<CommunityPostReactionResp> {
  const resp = await requestJson<undefined, CommunityPostReactionResp>(
    `/community/posts/${encodeURIComponent(postId)}/reactions/interested`,
    { method: interested ? "PUT" : "DELETE", auth: true },
  );
  return {
    ...resp,
    interest_count: BigInt(resp.interest_count),
  };
}

function normalizePostsResp(resp: ListCommunityPostsResp): ListCommunityPostsResp {
  return {
    ...resp,
    items: resp.items.map(normalizePost),
  };
}

function normalizePost(post: CommunityPostResp): CommunityPostResp {
  return {
    ...post,
    discussion_count: BigInt(post.discussion_count),
    interest_count: BigInt(post.interest_count),
  };
}

export type {
  CommunityPostResp,
  CommunitySectionResp,
  CreateCommunityPostRequest,
};
