import type { CreateEventRequest, EventResp, ListEventsResp } from "@/lib/dto";
import { requestJson } from "@/lib/api-client";

export function listEvents(limit = 20, offset = 0): Promise<ListEventsResp> {
  const params = new URLSearchParams({
    limit: String(limit),
    offset: String(offset),
  });
  return requestJson<undefined, ListEventsResp>(`/event?${params.toString()}`);
}

export function createEvent(params: CreateEventRequest): Promise<EventResp> {
  return requestJson<CreateEventRequest, EventResp>("/event", {
    method: "POST",
    body: params,
    auth: true,
  });
}

export type { CreateEventRequest, EventResp, ListEventsResp };
