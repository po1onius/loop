import { FlashList } from "@shopify/flash-list";
import { useFocusEffect } from "@react-navigation/native";
import { router } from "expo-router";
import { useCallback, useRef, useState } from "react";
import { ActivityIndicator, Pressable, RefreshControl, StyleSheet, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";

import { formatRelativeTime } from "@/components/community-post-card";
import { ThemedText } from "@/components/themed-text";
import { ThemedView } from "@/components/themed-view";
import { useColorScheme } from "@/hooks/use-color-scheme";
import { listConversations } from "@/lib/conversation-api";
import type { ConversationResp } from "@/lib/dto";

export default function ChatScreen() {
  const isDark = useColorScheme() === "dark";
  const requestIdRef = useRef(0);
  const [conversations, setConversations] = useState<ConversationResp[]>([]);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState("");

  const load = useCallback(async (refresh = false) => {
    const requestId = ++requestIdRef.current;
    if (refresh) {
      setRefreshing(true);
    } else {
      setLoading(true);
    }
    setError("");
    try {
      const resp = await listConversations();
      if (requestId !== requestIdRef.current) return;
      setConversations(resp.items);
      console.info("[chat] subscribed conversations loaded", { count: resp.items.length });
    } catch (loadError) {
      if (requestId !== requestIdRef.current) return;
      const message = loadError instanceof Error ? loadError.message : "聊天列表加载失败";
      console.warn("[chat] conversations load failed", { reason: message });
      setError(message);
    } finally {
      if (requestId === requestIdRef.current) {
        setLoading(false);
        setRefreshing(false);
      }
    }
  }, []);

  useFocusEffect(useCallback(() => {
    void load(false);
    return () => {
      requestIdRef.current += 1;
    };
  }, [load]));

  return (
    <SafeAreaView style={styles.safeArea} edges={["top", "left", "right"]}>
      <ThemedView style={styles.container}>
        <View style={styles.header}>
          <ThemedText type="title">聊天</ThemedText>
          <ThemedText style={styles.description}>帖子讨论和后续活动群聊都会集中显示在这里。</ThemedText>
        </View>
        {error ? <Pressable onPress={() => void load(false)} style={styles.errorCard}><ThemedText style={styles.errorText}>{error}，点击重试</ThemedText></Pressable> : null}
        {loading && !conversations.length ? <View style={styles.center}><ActivityIndicator color="#0A7EA4" /><ThemedText style={styles.muted}>正在加载会话...</ThemedText></View> : (
          <FlashList
            data={conversations}
            keyExtractor={(item) => item.conversation_id}
            renderItem={({ item }) => (
              <Pressable accessibilityRole="button" onPress={() => router.push(`/conversation/${encodeURIComponent(item.conversation_id)}` as never)} style={({ pressed }) => [styles.thread, { borderBottomColor: isDark ? "#2D353C" : "#E8EBEE" }, pressed ? styles.pressed : undefined]}>
                <View style={[styles.avatar, { backgroundColor: item.kind === "post_thread" ? "#D9EEF5" : "#E9E1F8" }]}>
                  <ThemedText style={styles.avatarIcon}>{item.kind === "post_thread" ? "#" : "群"}</ThemedText>
                </View>
                <View style={styles.threadCopy}>
                  <View style={styles.titleRow}>
                    <ThemedText type="defaultSemiBold" numberOfLines={1} style={styles.threadTitle}>{item.title}</ThemedText>
                    <ThemedText style={styles.time}>{item.last_message_at ? formatRelativeTime(item.last_message_at) : "新会话"}</ThemedText>
                  </View>
                  <View style={styles.previewRow}>
                    <ThemedText numberOfLines={1} style={styles.preview}>{item.last_message_preview || (item.kind === "post_thread" ? "进入帖子讨论" : "进入群聊")}</ThemedText>
                    {item.unread_count > 0n ? <View style={styles.badge}><ThemedText style={styles.badgeText}>{item.unread_count > 99n ? "99+" : item.unread_count.toString()}</ThemedText></View> : null}
                  </View>
                </View>
              </Pressable>
            )}
            contentContainerStyle={styles.listContent}
            refreshControl={<RefreshControl refreshing={refreshing} tintColor="#0A7EA4" onRefresh={() => void load(true)} />}
            ListEmptyComponent={<View style={styles.center}><ThemedText type="defaultSemiBold">暂无会话</ThemedText><ThemedText style={styles.muted}>发布帖子或参与讨论后，会话会出现在这里</ThemedText><Pressable onPress={() => router.push("/(tabs)/community" as never)} style={styles.communityButton}><ThemedText style={styles.communityButtonText}>去社区看看</ThemedText></Pressable></View>}
          />
        )}
      </ThemedView>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  safeArea: { flex: 1 }, container: { flex: 1 }, header: { paddingHorizontal: 16, paddingTop: 16, paddingBottom: 12, gap: 7 }, description: { lineHeight: 21, opacity: 0.7 },
  listContent: { paddingHorizontal: 14, paddingBottom: 24 }, thread: { flexDirection: "row", alignItems: "center", paddingVertical: 13, gap: 11, borderBottomWidth: StyleSheet.hairlineWidth },
  avatar: { width: 50, height: 50, borderRadius: 25, alignItems: "center", justifyContent: "center" }, avatarIcon: { color: "#08789C", fontSize: 21, fontWeight: "800" }, threadCopy: { flex: 1, gap: 4 },
  titleRow: { flexDirection: "row", alignItems: "center", gap: 8 }, threadTitle: { flex: 1 }, time: { opacity: 0.5, fontSize: 11, lineHeight: 15 }, previewRow: { flexDirection: "row", alignItems: "center", gap: 8 }, preview: { flex: 1, opacity: 0.62, fontSize: 14, lineHeight: 19 },
  badge: { minWidth: 21, height: 21, borderRadius: 11, paddingHorizontal: 5, alignItems: "center", justifyContent: "center", backgroundColor: "#0A7EA4" }, badgeText: { color: "#FFFFFF", fontSize: 11, lineHeight: 14, fontWeight: "800" },
  center: { padding: 40, alignItems: "center", justifyContent: "center", gap: 10 }, muted: { opacity: 0.6, textAlign: "center" }, communityButton: { marginTop: 5, backgroundColor: "#0A7EA4", borderRadius: 20, paddingHorizontal: 18, paddingVertical: 9 }, communityButtonText: { color: "#FFFFFF", fontWeight: "700" },
  errorCard: { marginHorizontal: 16, backgroundColor: "#FFF0F0", borderRadius: 9, padding: 9 }, errorText: { color: "#C23C3C", textAlign: "center", fontSize: 13 }, pressed: { opacity: 0.65 },
});
