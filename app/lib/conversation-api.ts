import * as Crypto from "expo-crypto";

import type {
  ConversationMessageResp,
  ConversationResp,
  ListConversationMessagesResp,
  ListConversationsResp,
  SendConversationMessageRequest,
} from "@/lib/dto";
import { requestJson } from "@/lib/api-client";

export async function listConversations(): Promise<ListConversationsResp> {
  const resp = await requestJson<undefined, ListConversationsResp>(
    "/conversations",
    { auth: true },
  );
  return {
    items: resp.items.map(normalizeConversation),
  };
}

export async function getConversation(
  conversationId: string,
): Promise<ConversationResp> {
  const resp = await requestJson<undefined, ConversationResp>(
    `/conversations/${encodeURIComponent(conversationId)}`,
    { auth: true },
  );
  return normalizeConversation(resp);
}

export async function listConversationMessages(params: {
  conversationId: string;
  beforeSeq?: bigint | null;
  afterSeq?: bigint | null;
  limit?: number;
}): Promise<ListConversationMessagesResp> {
  const query = new URLSearchParams({ limit: String(params.limit ?? 50) });
  if (params.beforeSeq !== null && params.beforeSeq !== undefined) {
    query.set("before_seq", params.beforeSeq.toString());
  }
  if (params.afterSeq !== null && params.afterSeq !== undefined) {
    query.set("after_seq", params.afterSeq.toString());
  }
  const resp = await requestJson<undefined, ListConversationMessagesResp>(
    `/conversations/${encodeURIComponent(params.conversationId)}/messages?${query.toString()}`,
    { auth: true },
  );
  return {
    items: resp.items.map(normalizeMessage),
    next_before_seq:
      resp.next_before_seq === null ? null : BigInt(resp.next_before_seq),
    next_after_seq:
      resp.next_after_seq === null ? null : BigInt(resp.next_after_seq),
  };
}

export async function sendConversationMessage(params: {
  conversationId: string;
  body: string;
  imageAssetIds?: string[];
  quoteMessageId?: string | null;
  clientMessageId?: string;
}): Promise<ConversationMessageResp> {
  const body: SendConversationMessageRequest = {
    client_message_id: params.clientMessageId ?? Crypto.randomUUID(),
    body: params.body,
    image_asset_ids: params.imageAssetIds ?? [],
    quote_message_id: params.quoteMessageId ?? null,
  };
  const resp = await requestJson<
    SendConversationMessageRequest,
    ConversationMessageResp
  >(
    `/conversations/${encodeURIComponent(params.conversationId)}/messages`,
    { method: "POST", auth: true, body },
  );
  return normalizeMessage(resp);
}

export function markConversationRead(
  conversationId: string,
  lastReadSeq: bigint,
): Promise<void> {
  return requestJson<{ last_read_seq: bigint }, void>(
    `/conversations/${encodeURIComponent(conversationId)}/read`,
    {
      method: "PUT",
      auth: true,
      body: { last_read_seq: lastReadSeq },
    },
  );
}

export function setConversationSubscription(params: {
  conversationId: string;
  subscribed: boolean;
  muted?: boolean;
}): Promise<void> {
  return requestJson<{ subscribed: boolean; muted: boolean }, void>(
    `/conversations/${encodeURIComponent(params.conversationId)}/subscription`,
    {
      method: "PUT",
      auth: true,
      body: {
        subscribed: params.subscribed,
        muted: params.muted ?? false,
      },
    },
  );
}

function normalizeConversation(
  conversation: ConversationResp,
): ConversationResp {
  return {
    ...conversation,
    last_seq: BigInt(conversation.last_seq),
    message_count: BigInt(conversation.message_count),
    last_read_seq: BigInt(conversation.last_read_seq),
    unread_count: BigInt(conversation.unread_count),
  };
}

function normalizeMessage(
  message: ConversationMessageResp,
): ConversationMessageResp {
  return {
    ...message,
    seq: BigInt(message.seq),
  };
}

export type { ConversationMessageResp, ConversationResp };
