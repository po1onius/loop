import type {
  CreateEventDraftRequest,
  CreateEventRequest,
  EventResp,
  ListEventsResp,
  UpdateEventDraftRequest,
} from "@/lib/dto";
import { requestJson } from "@/lib/api-client";

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
): Promise<EventResp> {
  console.info("[event-api] creating event draft", {
    titleLength: params.title?.trim().length ?? 0,
    blockCount: params.content?.blocks.length ?? 0,
    tagCount: params.tags?.length ?? 0,
  });
  return requestJson<CreateEventDraftRequest, EventResp>("/event/drafts", {
    method: "POST",
    auth: true,
    body: params,
  });
}

export function updateEventDraft(
  eventId: string,
  params: UpdateEventDraftRequest,
): Promise<EventResp> {
  const normalizedEventId = eventId.trim();
  console.info("[event-api] updating event draft", {
    eventId: normalizedEventId,
    titleLength: params.title.trim().length,
    blockCount: params.content.blocks.length,
    tagCount: params.tags.length,
  });
  return requestJson<UpdateEventDraftRequest, EventResp>(
    `/event/${encodeURIComponent(normalizedEventId)}`,
    {
      method: "PATCH",
      auth: true,
      body: params,
    },
  );
}

export function publishEventDraft(
  eventId: string,
): Promise<EventResp> {
  const normalizedEventId = eventId.trim();
  console.info("[event-api] publishing event draft", {
    eventId: normalizedEventId,
  });
  return requestJson<undefined, EventResp>(
    `/event/${encodeURIComponent(normalizedEventId)}/publish`,
    {
      method: "POST",
      auth: true,
    },
  );
}

export function deleteEventDraft(
  eventId: string,
): Promise<void> {
  const normalizedEventId = eventId.trim();
  console.info("[event-api] deleting event draft", {
    eventId: normalizedEventId,
  });
  return requestJson<undefined, void>(
    `/event/${encodeURIComponent(normalizedEventId)}`,
    {
      method: "DELETE",
      auth: true,
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

export async function listAllMyEventDrafts(): Promise<EventResp[]> {
  const pageSize = 50;
  const drafts: EventResp[] = [];
  const seenEventIds = new Set<string>();
  let offset = 0;

  // 后端单页最多返回 50 条。入口选择器需要展示全部草稿，因此按 next_offset
  // 继续拉取并按 event_id 去重，避免静默隐藏较早保存的草稿。
  for (;;) {
    const page = await listMyEvents("draft", pageSize, offset);
    for (const draft of page.items) {
      if (!seenEventIds.has(draft.event_id)) {
        seenEventIds.add(draft.event_id);
        drafts.push(draft);
      }
    }
    console.info("[event-api] loaded event draft page", {
      offset,
      pageCount: page.items.length,
      totalCount: drafts.length,
      nextOffset: page.next_offset,
    });

    if (page.next_offset === null || page.next_offset <= offset) {
      return drafts;
    }
    offset = page.next_offset;
  }
}

export type {
  CreateEventDraftRequest,
  CreateEventRequest,
  EventResp,
  ListEventsResp,
  UpdateEventDraftRequest,
};
