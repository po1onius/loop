import type {
  CreateEventDraftRequest,
  CreateEventRequest,
  EventResp,
  ListEventsResp,
  OpenEventDraftResp,
  UpdateEventDraftRequest,
} from "@/lib/dto";
import { requestJson } from "@/lib/api-client";

const DRAFT_SESSION_HEADER = "x-draft-session-id";

export function listEvents(limit = 20, offset = 0): Promise<ListEventsResp> {
  const params = new URLSearchParams({
    limit: String(limit),
    offset: String(offset),
  });
  return requestJson<undefined, ListEventsResp>(`/event?${params.toString()}`);
}

export function getEvent(eventId: string): Promise<EventResp> {
  const normalizedEventId = eventId.trim();
  console.info("[event-api] loading event detail", {
    eventId: normalizedEventId,
  });
  return requestJson<undefined, EventResp>(
    `/event/${encodeURIComponent(normalizedEventId)}`,
  );
}

export function createEvent(params: CreateEventRequest): Promise<EventResp> {
  console.info("[event-api] creating event", {
    titleLength: params.title.trim().length,
    blockCount: params.content.blocks.length,
    tagCount: params.tags.length,
  });
  return requestJson<CreateEventRequest, EventResp>("/event", {
    method: "POST",
    auth: true,
    body: params,
  });
}

export function createEventDraft(
  params: CreateEventDraftRequest,
  draftSessionId?: string,
): Promise<EventResp> {
  console.info("[event-api] creating event draft", {
    hasDraftSession: Boolean(draftSessionId),
    titleLength: params.title?.trim().length ?? 0,
    blockCount: params.content?.blocks.length ?? 0,
    tagCount: params.tags?.length ?? 0,
  });
  return requestJson<CreateEventDraftRequest, EventResp>("/event/drafts", {
    method: "POST",
    auth: true,
    headers: draftSessionHeaders(draftSessionId),
    body: params,
  });
}

export function updateEventDraft(
  eventId: string,
  params: UpdateEventDraftRequest,
  draftSessionId?: string,
): Promise<EventResp> {
  const normalizedEventId = eventId.trim();
  console.info("[event-api] updating event draft", {
    eventId: normalizedEventId,
    hasDraftSession: Boolean(draftSessionId),
    titleLength: params.title.trim().length,
    blockCount: params.content.blocks.length,
    tagCount: params.tags.length,
  });
  return requestJson<UpdateEventDraftRequest, EventResp>(
    `/event/${encodeURIComponent(normalizedEventId)}`,
    {
      method: "PATCH",
      auth: true,
      headers: draftSessionHeaders(draftSessionId),
      body: params,
    },
  );
}

export function publishEventDraft(
  eventId: string,
  draftSessionId?: string,
): Promise<EventResp> {
  const normalizedEventId = eventId.trim();
  console.info("[event-api] publishing event draft", {
    eventId: normalizedEventId,
    hasDraftSession: Boolean(draftSessionId),
  });
  return requestJson<undefined, EventResp>(
    `/event/${encodeURIComponent(normalizedEventId)}/publish`,
    {
      method: "POST",
      auth: true,
      headers: draftSessionHeaders(draftSessionId),
    },
  );
}

export function deleteEventDraft(
  eventId: string,
  draftSessionId?: string,
): Promise<void> {
  const normalizedEventId = eventId.trim();
  console.info("[event-api] deleting event draft", {
    eventId: normalizedEventId,
    hasDraftSession: Boolean(draftSessionId),
  });
  return requestJson<undefined, void>(
    `/event/${encodeURIComponent(normalizedEventId)}`,
    {
      method: "DELETE",
      auth: true,
      headers: draftSessionHeaders(draftSessionId),
    },
  );
}

export function openCurrentEventDraft(
  draftSessionId: string,
): Promise<OpenEventDraftResp> {
  console.info("[event-api] opening current event draft", {
    draftSessionId,
  });
  return requestJson<{ draft_session_id: string }, OpenEventDraftResp>(
    "/me/event-draft/open",
    {
      method: "POST",
      auth: true,
      body: { draft_session_id: draftSessionId },
    },
  );
}

export function updateCurrentEventDraft(
  draftSessionId: string,
  params: UpdateEventDraftRequest,
): Promise<EventResp> {
  console.info("[event-api] updating current event draft", {
    draftSessionId,
    titleLength: params.title.trim().length,
    blockCount: params.content.blocks.length,
    tagCount: params.tags.length,
  });
  return requestJson<UpdateEventDraftRequest, EventResp>("/me/event-draft", {
    method: "PATCH",
    auth: true,
    headers: draftSessionHeaders(draftSessionId),
    body: params,
  });
}

export function publishCurrentEventDraft(
  draftSessionId: string,
): Promise<EventResp> {
  console.info("[event-api] publishing current event draft", {
    draftSessionId,
  });
  return requestJson<undefined, EventResp>("/me/event-draft/publish", {
    method: "POST",
    auth: true,
    headers: draftSessionHeaders(draftSessionId),
  });
}

export function deleteCurrentEventDraft(draftSessionId: string): Promise<void> {
  console.info("[event-api] deleting current event draft", {
    draftSessionId,
  });
  return requestJson<undefined, void>("/me/event-draft", {
    method: "DELETE",
    auth: true,
    headers: draftSessionHeaders(draftSessionId),
  });
}

export function refreshCurrentEventDraftLease(
  draftSessionId: string,
): Promise<void> {
  return requestJson<{ draft_session_id: string }, void>(
    "/me/event-draft/lease/refresh",
    {
      method: "POST",
      auth: true,
      body: { draft_session_id: draftSessionId },
    },
  );
}

export function releaseCurrentEventDraftLease(
  draftSessionId: string,
): Promise<void> {
  return requestJson<{ draft_session_id: string }, void>(
    "/me/event-draft/lease",
    {
      method: "DELETE",
      auth: true,
      body: { draft_session_id: draftSessionId },
    },
  );
}

export function listMyEvents(
  status?: "draft" | "published" | "cancelled",
  limit = 20,
  offset = 0,
): Promise<ListEventsResp> {
  const params = new URLSearchParams({
    limit: String(limit),
    offset: String(offset),
  });
  if (status) {
    params.set("status", status);
  }
  return requestJson<undefined, ListEventsResp>(
    `/me/events?${params.toString()}`,
    { auth: true },
  );
}

function draftSessionHeaders(
  draftSessionId: string | null | undefined,
): Record<string, string> {
  const value = draftSessionId?.trim();
  return value ? { [DRAFT_SESSION_HEADER]: value } : {};
}

export type {
  CreateEventDraftRequest,
  CreateEventRequest,
  EventResp,
  ListEventsResp,
  OpenEventDraftResp,
  UpdateEventDraftRequest,
};
