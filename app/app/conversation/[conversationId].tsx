import { Redirect, router, useLocalSearchParams } from "expo-router";
import { useEffect, useState } from "react";
import { ActivityIndicator, Pressable, StyleSheet } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";

import { ConversationScreen } from "@/components/conversation/conversation-screen";
import { ThemedText } from "@/components/themed-text";
import { ThemedView } from "@/components/themed-view";
import { IconSymbol } from "@/components/ui/icon-symbol";
import { useThemeColor } from "@/hooks/use-theme-color";
import { getCurrentUser } from "@/lib/auth-api";
import { subscribeRealtimeEvents } from "@/lib/realtime-client";
import { getConversation } from "@/lib/conversation-api";
import type { ConversationResp } from "@/lib/dto";

export default function ConversationRoute() {
  const textColor = useThemeColor({}, "text");
  const params = useLocalSearchParams<{ conversationId?: string | string[] }>();
  const conversationId = Array.isArray(params.conversationId) ? params.conversationId[0] : params.conversationId;
  const [conversation, setConversation] = useState<ConversationResp | null>(null);
  const [currentUserId, setCurrentUserId] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    if (!conversationId) {
      setError("会话地址无效");
      return;
    }
    let active = true;
    setConversation(null); setError("");
    void Promise.all([getConversation(conversationId), getCurrentUser()])
      .then(([nextConversation, user]) => {
        if (!active) return;
        setConversation(nextConversation);
        setCurrentUserId(user.user_id);
        console.info("[conversation-route] conversation context loaded", {
          conversationId,
          kind: nextConversation.kind,
          subjectId: nextConversation.subject_id,
        });
      })
      .catch((loadError) => {
        if (!active) return;
        const message = loadError instanceof Error ? loadError.message : "会话加载失败";
        console.warn("[conversation-route] conversation context load failed", { conversationId, reason: message });
        setError(message);
      });
    return () => {
      active = false;
    };
  }, [conversationId]);

  useEffect(() => {
    if (!conversationId) return;
    let active = true;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const unsubscribe = subscribeRealtimeEvents((event) => {
      if (event.conversation_id && event.conversation_id !== conversationId) return;
      if (!timer) timer = setTimeout(() => {
        timer = null;
        void getConversation(conversationId).then((next) => { if (active) setConversation(next); }).catch(() => console.warn("[conversation-route] conversation refresh failed"));
      }, 150);
    });
    return () => { active = false; unsubscribe(); if (timer) clearTimeout(timer); };
  }, [conversationId]);

  if (!conversation || !currentUserId) {
    return (
      <SafeAreaView style={styles.safeArea}>
        <ThemedView style={styles.center}>
          {error ? (
            <>
              <ThemedText style={styles.error}>{error}</ThemedText>
              <Pressable onPress={() => router.back()} style={styles.backAction}><ThemedText style={styles.backText}>返回</ThemedText></Pressable>
            </>
          ) : (
            <><ActivityIndicator color="#0A7EA4" /><ThemedText style={styles.muted}>正在进入讨论...</ThemedText></>
          )}
        </ThemedView>
      </SafeAreaView>
    );
  }

  if (conversation.kind === "post_thread") {
    return <Redirect href={{ pathname: "/community/post/[postId]", params: { postId: conversation.subject_id } }} />;
  }

  return (
    <ConversationScreen
      key={conversation.conversation_id}
      conversation={conversation}
      currentUserId={currentUserId}
      onBack={() => router.back()}
      onConversationChange={setConversation}
      headerAction={conversation.kind === "event_group" ? (
        <Pressable
          accessibilityRole="button"
          accessibilityLabel="群聊管理"
          style={styles.manageButton}
          onPress={() => {
            console.info("[conversation-route] opening group management", { conversationId });
            router.push(`/conversation/${encodeURIComponent(conversation.conversation_id)}/manage` as never);
          }}
        >
          <IconSymbol name="ellipsis" size={26} color={textColor} />
        </Pressable>
      ) : undefined}
    />
  );
}

const styles = StyleSheet.create({
  manageButton: { width: 42, height: 42, alignItems: "center", justifyContent: "center" },
  safeArea: { flex: 1 }, center: { flex: 1, alignItems: "center", justifyContent: "center", gap: 12, padding: 30 },
  muted: { opacity: 0.6 }, error: { color: "#C23C3C", textAlign: "center" }, backAction: { paddingHorizontal: 20, paddingVertical: 9, borderRadius: 20, backgroundColor: "#0A7EA4" }, backText: { color: "#FFFFFF", fontWeight: "700" },
});
