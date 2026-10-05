import { useEffect, useState } from "react";
import { Pressable, StyleSheet, Text } from "react-native";
import { router } from "expo-router";
import { subscribePushNotices, type PushNotice } from "@/lib/notification-state";
import { subscribeSessionState } from "@/lib/api-client";

export function NotificationBanner() {
  const [notice, setNotice] = useState<PushNotice | null>(null);
  useEffect(() => subscribePushNotices(setNotice), []);
  useEffect(() => subscribeSessionState((authenticated) => { if (!authenticated) setNotice(null); }), []);
  useEffect(() => {
    if (!notice) return;
    const timer = setTimeout(() => setNotice(null), 5000);
    return () => clearTimeout(timer);
  }, [notice]);
  if (!notice) return null;
  return <Pressable accessibilityRole="button" onPress={() => { router.push(`/conversation/${encodeURIComponent(notice.conversationId)}` as never); setNotice(null); }} style={styles.banner}>
    <Text style={styles.title} numberOfLines={1}>{notice.title}</Text>
    <Text style={styles.body} numberOfLines={2}>{notice.body}</Text>
  </Pressable>;
}
const styles = StyleSheet.create({ banner: { position: "absolute", top: 60, left: 16, right: 16, borderRadius: 12, padding: 14, backgroundColor: "#154B5F", zIndex: 1000 }, title: { color: "white", fontWeight: "700", marginBottom: 4 }, body: { color: "white" } });
