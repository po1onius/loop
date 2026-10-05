export type PushNotice = { conversationId: string; messageId: string; title: string; body: string };
let activeConversation: string | null = null;
const listeners = new Set<(notice: PushNotice) => void>();
export function setActiveConversation(id: string | null) { activeConversation = id; }
export function getActiveConversation() { return activeConversation; }
export function showPushNotice(notice: PushNotice) { for (const listener of listeners) listener(notice); }
export function subscribePushNotices(listener: (notice: PushNotice) => void) {
  listeners.add(listener);
  return () => { listeners.delete(listener); };
}
