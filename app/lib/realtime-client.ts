import { getRealtimeAccessToken } from "@/lib/api-client";

const REALTIME_URL = normalizeRealtimeUrl(
  process.env["EXPO_PUBLIC_REALTIME_URL"],
);
const INITIAL_RECONNECT_DELAY_MS = 1_000;
const MAX_RECONNECT_DELAY_MS = 15_000;

export type ConversationRealtimeEvent = {
  type: "conversation.message_created";
  conversation_id: string;
  seq: string | number;
  message_id: string;
};

/**
 * 每个打开的会话维持一条 WebSocket。服务端事件只作为“有新序号”的通知，
 * 消息正文仍从 HTTP API 回补，因此 Redis Pub/Sub 的 at-most-once 语义不会
 * 造成永久丢消息，重连后定时对账也能补齐断线窗口。
 */
export function subscribeConversationRealtime(
  conversationId: string,
  onEvent: (event: ConversationRealtimeEvent) => void,
): { configured: boolean; close: () => void } {
  if (!REALTIME_URL) {
    console.info("[realtime-client] realtime URL is not configured; using HTTP polling", {
      conversationId,
    });
    return { configured: false, close: () => undefined };
  }

  let disposed = false;
  let socket: WebSocket | null = null;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  let reconnectDelay = INITIAL_RECONNECT_DELAY_MS;

  const scheduleReconnect = () => {
    if (disposed || reconnectTimer) return;
    reconnectTimer = setTimeout(() => {
      reconnectTimer = null;
      void connect();
    }, reconnectDelay);
    reconnectDelay = Math.min(reconnectDelay * 2, MAX_RECONNECT_DELAY_MS);
  };

  const connect = async () => {
    if (disposed) return;
    try {
      const token = await getRealtimeAccessToken();
      if (disposed) return;
      const nextSocket = new WebSocket(REALTIME_URL);
      socket = nextSocket;
      nextSocket.onopen = () => {
        reconnectDelay = INITIAL_RECONNECT_DELAY_MS;
        nextSocket.send(JSON.stringify({
          type: "authenticate",
          access_token: token,
          conversation_id: conversationId,
        }));
        console.info("[realtime-client] websocket opened; authentication sent", { conversationId });
      };
      nextSocket.onmessage = (message) => {
        if (typeof message.data !== "string") return;
        try {
          const frame = JSON.parse(message.data) as Record<string, unknown>;
          if (frame["type"] === "subscribed") {
            console.info("[realtime-client] conversation subscribed", { conversationId });
            return;
          }
          if (
            frame["type"] === "conversation.message_created" &&
            frame["conversation_id"] === conversationId &&
            typeof frame["message_id"] === "string"
          ) {
            onEvent(frame as ConversationRealtimeEvent);
          }
        } catch (error) {
          console.warn("[realtime-client] invalid server frame ignored", {
            conversationId,
            reason: error instanceof Error ? error.message : String(error),
          });
        }
      };
      nextSocket.onerror = () => {
        console.warn("[realtime-client] websocket transport error", { conversationId });
      };
      nextSocket.onclose = (event) => {
        if (socket === nextSocket) socket = null;
        console.info("[realtime-client] websocket closed", {
          conversationId,
          code: event.code,
          reason: event.reason,
          disposed,
        });
        scheduleReconnect();
      };
    } catch (error) {
      console.warn("[realtime-client] websocket connection preparation failed", {
        conversationId,
        reason: error instanceof Error ? error.message : String(error),
      });
      scheduleReconnect();
    }
  };

  void connect();
  return {
    configured: true,
    close: () => {
      disposed = true;
      if (reconnectTimer) clearTimeout(reconnectTimer);
      reconnectTimer = null;
      socket?.close(1000, "conversation screen closed");
      socket = null;
    },
  };
}

function normalizeRealtimeUrl(value: string | undefined): string | null {
  const normalized = value?.trim();
  if (!normalized) return null;
  try {
    const url = new URL(normalized);
    if (url.protocol !== "ws:" && url.protocol !== "wss:") {
      console.warn("[realtime-client] realtime URL must use ws:// or wss://");
      return null;
    }
    return url.toString();
  } catch {
    console.warn("[realtime-client] realtime URL is invalid");
    return null;
  }
}
