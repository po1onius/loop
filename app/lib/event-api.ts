import type { EventResp, ListEventsResp } from "@/lib/dto";
import { requestJson } from "@/lib/api-client";

export function listEvents(limit = 20, offset = 0): Promise<ListEventsResp> {
  const params = new URLSearchParams({
    limit: String(limit),
    offset: String(offset),
  });
  return requestJson<undefined, ListEventsResp>(`/event?${params.toString()}`);
}

export type { EventResp, ListEventsResp };
