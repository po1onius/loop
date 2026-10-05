import { AppState } from "react-native";
import { getRealtimeAccessToken } from "@/lib/api-client";

const REALTIME_URL = process.env.EXPO_PUBLIC_REALTIME_URL?.trim();
export type ConversationRealtimeEvent = {
  type: "conversation.message_created" | "conversation.changed" | "sync";
  conversation_id?: string;
  seq?: string | number;
  message_id?: string;
};
type Listener = (event: ConversationRealtimeEvent) => void;
const listeners = new Set<Listener>();
const watched = new Map<string, number>();
let socket: WebSocket | null = null;
let authenticated = false;

export function emitConversationEvent(event: ConversationRealtimeEvent) {
  for (const listener of listeners) listener(event);
}
export function subscribeRealtimeEvents(listener: Listener): () => void {
  listeners.add(listener);
  return () => { listeners.delete(listener); };
}
function send(type: string, conversationId: string) {
  if (authenticated && socket?.readyState === WebSocket.OPEN) {
    socket.send(JSON.stringify({ type, conversation_id: conversationId }));
  }
}

/** One connection per signed-in app; screens only manage their subscriptions. */
export function startRealtime(): () => void {
  if (!REALTIME_URL) {
    console.info("[realtime] EXPO_PUBLIC_REALTIME_URL missing; periodic HTTP sync remains active");
    return () => undefined;
  }
  let disposed = false;
  let connecting = false;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let delay = 1000;
  const schedule = () => {
    if (disposed || timer || AppState.currentState === "background") return;
    timer = setTimeout(() => { timer = null; void connect(); }, delay);
    delay = Math.min(delay * 2, 15000);
  };
  const connect = async () => {
    if (disposed || connecting || socket) return;
    connecting = true;
    try {
      const token = await getRealtimeAccessToken();
      if (disposed) return;
      const next = new WebSocket(REALTIME_URL);
      socket = next;
      next.onopen = () => next.send(JSON.stringify({ type: "authenticate", access_token: token }));
      next.onmessage = ({ data }) => {
        if (disposed || socket !== next || typeof data !== "string") return;
        try {
          const event = JSON.parse(data);
          if (event.type === "ready") {
            authenticated = true;
            delay = 1000;
            for (const id of watched.keys()) send("subscribe", id);
            emitConversationEvent({ type: "sync" });
            console.info("[realtime] user connection ready");
          } else if (event.type === "subscribed") {
            emitConversationEvent({ type: "sync", conversation_id: event.conversation_id });
          } else if (["conversation.message_created", "conversation.changed"].includes(event.type) && typeof event.conversation_id === "string") {
            emitConversationEvent(event);
          } else if (event.type === "error") {
            console.warn("[realtime] subscription rejected", { reason: event.reason, conversationId: event.conversation_id });
          }
        } catch { console.warn("[realtime] invalid server frame"); }
      };
      next.onerror = () => console.warn("[realtime] transport error");
      next.onclose = () => {
        if (socket === next) { socket = null; authenticated = false; }
        schedule();
      };
    } catch (error) {
      console.warn("[realtime] connection failed", { reason: error instanceof Error ? error.message : String(error) });
      schedule();
    } finally { connecting = false; }
  };
  const appState = AppState.addEventListener("change", (state) => {
    if (state === "active") {
      emitConversationEvent({ type: "sync" });
      // The OS may have suspended a socket without delivering its close callback.
      if (socket) { const old = socket; socket = null; authenticated = false; old.close(); }
      void connect();
    }
  });
  void connect();
  return () => {
    disposed = true;
    appState.remove();
    if (timer) clearTimeout(timer);
    const old = socket; socket = null; authenticated = false;
    old?.close(1000, "session ended");
  };
}

export function subscribeConversationRealtime(conversationId: string, onEvent: Listener) {
  watched.set(conversationId, (watched.get(conversationId) ?? 0) + 1);
  send("subscribe", conversationId);
  const unsubscribe = subscribeRealtimeEvents((event) => {
    if (!event.conversation_id || event.conversation_id === conversationId) onEvent(event);
  });
  return {
    configured: Boolean(REALTIME_URL),
    close: () => {
      unsubscribe();
      const count = (watched.get(conversationId) ?? 1) - 1;
      if (count > 0) watched.set(conversationId, count);
      else { watched.delete(conversationId); send("unsubscribe", conversationId); }
    },
  };
}
