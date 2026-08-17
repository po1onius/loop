import { router, useLocalSearchParams } from "expo-router";
import { useEffect, useState } from "react";
import { ActivityIndicator, Pressable, StyleSheet, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";

import { ConversationScreen } from "@/components/conversation/conversation-screen";
import { ThemedText } from "@/components/themed-text";
import { ThemedView } from "@/components/themed-view";
import { getCurrentUser } from "@/lib/auth-api";
import { getCommunityPost } from "@/lib/community-api";
import { getConversation } from "@/lib/conversation-api";
import type { CommunityPostResp, ConversationResp } from "@/lib/dto";

export default function ConversationRoute() {
  const params = useLocalSearchParams<{ conversationId?: string | string[] }>();
  const conversationId = Array.isArray(params.conversationId) ? params.conversationId[0] : params.conversationId;
  const [conversation, setConversation] = useState<ConversationResp | null>(null);
  const [post, setPost] = useState<CommunityPostResp | null>(null);
  const [currentUserId, setCurrentUserId] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    if (!conversationId) {
      setError("会话地址无效");
      return;
    }
    let active = true;
    void Promise.all([getConversation(conversationId), getCurrentUser()])
      .then(async ([nextConversation, user]) => {
        const nextPost = nextConversation.kind === "post_thread"
          ? await getCommunityPost(nextConversation.subject_id)
          : null;
        if (!active) return;
        setConversation(nextConversation);
        setCurrentUserId(user.user_id);
        setPost(nextPost);
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

  return (
    <ConversationScreen
      conversation={conversation}
      currentUserId={currentUserId}
      onBack={() => router.back()}
      onConversationChange={setConversation}
      headerContext={post ? <PostThreadContext post={post} /> : undefined}
    />
  );
}

function PostThreadContext({ post }: { post: CommunityPostResp }) {
  return (
    <Pressable
      accessibilityRole="button"
      onPress={() => router.push(`/community/post/${encodeURIComponent(post.post_id)}` as never)}
      style={({ pressed }) => [styles.context, pressed ? styles.pressed : undefined]}
    >
      <View style={styles.contextAccent} />
      <View style={styles.contextCopy}>
        <ThemedText style={styles.contextLabel}>正在讨论帖子</ThemedText>
        <ThemedText lightColor="#11181C" darkColor="#11181C" type="defaultSemiBold" numberOfLines={1}>{post.title}</ThemedText>
        <ThemedText lightColor="#50616A" darkColor="#50616A" style={styles.contextBody} numberOfLines={1}>{post.body}</ThemedText>
      </View>
      <ThemedText style={styles.contextArrow}>›</ThemedText>
    </Pressable>
  );
}

const styles = StyleSheet.create({
  safeArea: { flex: 1 }, center: { flex: 1, alignItems: "center", justifyContent: "center", gap: 12, padding: 30 },
  muted: { opacity: 0.6 }, error: { color: "#C23C3C", textAlign: "center" }, backAction: { paddingHorizontal: 20, paddingVertical: 9, borderRadius: 20, backgroundColor: "#0A7EA4" }, backText: { color: "#FFFFFF", fontWeight: "700" },
  context: { flexDirection: "row", alignItems: "center", marginHorizontal: 10, marginTop: 7, marginBottom: 2, borderRadius: 10, padding: 9, backgroundColor: "#EAF6FA" },
  contextAccent: { width: 3, alignSelf: "stretch", borderRadius: 2, backgroundColor: "#0A7EA4", marginRight: 9 }, contextCopy: { flex: 1 },
  contextLabel: { color: "#0A7EA4", fontSize: 11, lineHeight: 15, fontWeight: "700" }, contextBody: { color: "#50616A", fontSize: 12, lineHeight: 16 }, contextArrow: { color: "#0A7EA4", fontSize: 25 }, pressed: { opacity: 0.7 },
});
