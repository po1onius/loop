import { getApp } from "@react-native-firebase/app";
import { AuthorizationStatus, deleteToken, getInitialNotification, getMessaging, getToken, onMessage, onNotificationOpenedApp, onTokenRefresh, requestPermission, type RemoteMessage } from "@react-native-firebase/messaging";
import * as Crypto from "expo-crypto";
import * as SecureStore from "expo-secure-store";
import * as Notifications from "expo-notifications";
import { router } from "expo-router";
import { AppState, PermissionsAndroid, Platform } from "react-native";
import { requestJson } from "@/lib/api-client";
import { getConversation } from "@/lib/conversation-api";
import { emitConversationEvent } from "@/lib/realtime-client";
import { getActiveConversation, showPushNotice } from "@/lib/notification-state";

let generation = 0;
let registrationWork: Promise<void> = Promise.resolve();
let installationPromise: Promise<string> | null = null;
function installationId() {
  installationPromise ??= (async () => {
    const stored = await SecureStore.getItemAsync("loop.push.installation");
    if (stored) return stored;
    const id = Crypto.randomUUID();
    await SecureStore.setItemAsync("loop.push.installation", id);
    return id;
  })();
  return installationPromise;
}

export function startPushNotifications(): () => void {
  const current = ++generation;
  const messaging = getMessaging(getApp());
  let userId: string | null = null;
  const seen = new Set<string>();
  const register = (token: string) => {
    registrationWork = registrationWork.catch(() => undefined).then(async () => {
      if (generation !== current) return;
      const id = await installationId();
      if (generation !== current) return;
      await requestJson(`/me/push-devices/${id}`, { method: "PUT", auth: true, body: { platform: Platform.OS, token } });
      console.info("[push] device registered", { installationId: id, platform: Platform.OS });
    });
    void registrationWork.catch(() => console.warn("[push] device registration failed; verify API connectivity"));
    return registrationWork;
  };
  const open = async (message: RemoteMessage) => {
    const id = message.data?.['conversation_id'];
    if (typeof id !== "string" || message.data?.['user_id'] !== userId || generation !== current) return;
    try {
      await getConversation(id);
      if (generation === current) router.push(`/conversation/${encodeURIComponent(id)}` as never);
    } catch { console.warn("[push] notification conversation is no longer accessible"); }
  };
  const unsubscribeToken = onTokenRefresh(messaging, (token) => { void register(token); });
  const unsubscribeOpen = onNotificationOpenedApp(messaging, (message) => { void open(message); });
  const unsubscribeMessage = onMessage(messaging, async (message) => {
    if (generation !== current || message.data?.['user_id'] !== userId) return;
    const conversationId = message.data?.['conversation_id'];
    const messageId = message.data?.['message_id'];
    if (typeof conversationId !== "string" || typeof messageId !== "string" || seen.has(messageId)) return;
    seen.add(messageId);
    if (seen.size > 500) seen.delete(seen.values().next().value!);
    emitConversationEvent({ type: "conversation.message_created", conversation_id: conversationId, message_id: messageId });
    if (AppState.currentState === "active" && getActiveConversation() !== conversationId) {
      showPushNotice({ conversationId, messageId, title: message.notification?.title ?? "新消息", body: message.notification?.body ?? "收到一条群消息" });
    }
  });
  const initialize = async () => {
    userId = (await requestJson<undefined, { user_id: string }>("/me/profile", { auth: true })).user_id;
    if (generation !== current) return;
    const initial = await getInitialNotification(messaging);
    if (initial) await open(initial);
    if (Platform.OS === "android") {
      await Notifications.setNotificationChannelAsync("messages", { name: "聊天消息", importance: Notifications.AndroidImportance.HIGH });
      if (Number(Platform.Version) >= 33 && await PermissionsAndroid.request(PermissionsAndroid.PERMISSIONS.POST_NOTIFICATIONS) !== PermissionsAndroid.RESULTS.GRANTED) {
        console.info("[push] notification permission declined"); return;
      }
    } else {
      const authorization = await requestPermission(messaging);
      if (authorization !== AuthorizationStatus.AUTHORIZED && authorization !== AuthorizationStatus.PROVISIONAL) {
        console.info("[push] notification permission declined"); return;
      }
    }
    if (generation === current) await register(await getToken(messaging));
  };
  void initialize().catch(() => console.warn("[push] initialization failed; verify Firebase configuration and device Google/APNs services"));
  const foreground = AppState.addEventListener("change", (state) => {
    if (state === "active" && generation === current) {
      void getToken(messaging).then(register).catch(() => console.warn("[push] device token refresh failed"));
    }
  });
  return () => { generation++; unsubscribeToken(); unsubscribeOpen(); unsubscribeMessage(); foreground.remove(); };
}

export async function unregisterPushDevice(): Promise<void> {
  generation++;
  await registrationWork.catch(() => undefined);
  // Revoke locally as well, so logout while API is unreachable doesn't retain a usable token.
  await deleteToken(getMessaging(getApp())).catch(() => console.warn("[push] local token revocation failed"));
  try {
    const id = await installationId();
    await requestJson(`/me/push-devices/${id}`, { method: "DELETE", auth: true });
  } catch { console.warn("[push] server device unregistration failed"); }
}
