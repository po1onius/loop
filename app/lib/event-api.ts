import type { CreateEventRequest, EventResp, ListEventsResp } from "@/lib/dto";
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

export type { CreateEventRequest, EventResp, ListEventsResp };
