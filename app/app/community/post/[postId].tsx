import { router, useLocalSearchParams } from "expo-router";
import { useEffect, useState } from "react";
import { ActivityIndicator, Pressable, StyleSheet, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";

import { CommunityPostCard } from "@/components/community-post-card";
import { ConversationScreen } from "@/components/conversation/conversation-screen";
import { ThemedText } from "@/components/themed-text";
import { ThemedView } from "@/components/themed-view";
import { IconSymbol } from "@/components/ui/icon-symbol";
import { useColorScheme } from "@/hooks/use-color-scheme";
import { getCurrentUser } from "@/lib/auth-api";
import { getCommunityPost, setPostInterested } from "@/lib/community-api";
import { getConversation } from "@/lib/conversation-api";
import type { CommunityPostResp, ConversationResp } from "@/lib/dto";
import { subscribeRealtimeEvents } from "@/lib/realtime-client";

export default function CommunityPostDetailScreen() {
  const isDark = useColorScheme() === "dark";
  const params = useLocalSearchParams<{ postId?: string | string[] }>();
  const postId = Array.isArray(params.postId) ? params.postId[0] : params.postId;
  const [post, setPost] = useState<CommunityPostResp | null>(null);
  const [conversation, setConversation] = useState<ConversationResp | null>(null);
  const [currentUserId, setCurrentUserId] = useState("");
  const [loading, setLoading] = useState(true);
  const [reactionBusy, setReactionBusy] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (!postId) {
      setError("帖子地址无效");
      setLoading(false);
      return;
    }
    let active = true;
    setLoading(true);
    setPost(null);
    setConversation(null);
    setError("");
    void Promise.all([getCommunityPost(postId), getCurrentUser()])
      .then(async ([nextPost, user]) => {
        const nextConversation = await getConversation(nextPost.discussion_conversation_id);
        if (!active) return;
        setPost(nextPost);
        setConversation(nextConversation);
        setCurrentUserId(user.user_id);
        console.info("[community-post-detail] post and replies context loaded", {
          postId,
          conversationId: nextConversation.conversation_id,
        });
      })
      .catch((loadError) => {
        if (!active) return;
        const message = loadError instanceof Error ? loadError.message : "帖子加载失败";
        console.warn("[community-post-detail] post load failed", { postId, reason: message });
        setError(message);
      })
      .finally(() => active && setLoading(false));
    return () => {
      active = false;
    };
  }, [postId]);

  const conversationId = conversation?.conversation_id;
  useEffect(() => {
    if (!conversationId) return;
    let active = true;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const unsubscribe = subscribeRealtimeEvents((event) => {
      if (event.conversation_id && event.conversation_id !== conversationId) return;
      if (timer) return;
      timer = setTimeout(() => {
        timer = null;
        void getConversation(conversationId).then((next) => {
          if (!active) return;
          setConversation(next);
          setPost((current) => current ? { ...current, discussion_count: next.message_count } : current);
        }).catch((refreshError) => {
          console.warn("[community-post-detail] replies context refresh failed", {
            conversationId,
            reason: refreshError instanceof Error ? refreshError.message : String(refreshError),
          });
        });
      }, 150);
    });
    return () => {
      active = false;
      unsubscribe();
      if (timer) clearTimeout(timer);
    };
  }, [conversationId]);

  const toggleInterested = async () => {
    if (!post || reactionBusy) return;
    setReactionBusy(true);
    setError("");
    try {
      const reaction = await setPostInterested(post.post_id, !post.viewer_interested);
      setPost((current) => current?.post_id === post.post_id
        ? { ...current, viewer_interested: reaction.interested, interest_count: reaction.interest_count }
        : current);
    } catch (reactionError) {
      const message = reactionError instanceof Error ? reactionError.message : "操作失败";
      console.warn("[community-post-detail] interested reaction failed", { postId: post.post_id, reason: message });
      setError(message);
    } finally {
      setReactionBusy(false);
    }
  };

  if (!loading && post && conversation && currentUserId) {
    return (
      <ConversationScreen
        key={conversation.conversation_id}
        conversation={conversation}
        currentUserId={currentUserId}
        onBack={() => router.back()}
        onConversationChange={setConversation}
        initialScrollToEnd={false}
        listHeaderContext={
          <View style={styles.postContent}>
            <CommunityPostCard
              post={post}
              expanded
              interestedBusy={reactionBusy}
              onInterestedPress={() => void toggleInterested()}
            />
            {error ? <ThemedText style={styles.error}>{error}</ThemedText> : null}
            <ThemedText type="defaultSemiBold" style={styles.repliesTitle}>帖子回复</ThemedText>
          </View>
        }
      />
    );
  }

  const textColor = isDark ? "#ECEDEE" : "#11181C";
  return (
    <SafeAreaView style={styles.safeArea} edges={["top", "bottom", "left", "right"]}>
      <ThemedView style={styles.container}>
        <View style={[styles.header, { borderBottomColor: isDark ? "#303941" : "#E8EBEE" }]}>
          <Pressable accessibilityRole="button" onPress={() => router.back()} style={styles.backButton}>
            <IconSymbol name="chevron.left" size={26} color={textColor} />
          </Pressable>
          <ThemedText type="subtitle">帖子</ThemedText>
          <View style={styles.headerSpacer} />
        </View>
        {loading ? <View style={styles.center}><ActivityIndicator color="#0A7EA4" /><ThemedText style={styles.muted}>正在加载帖子...</ThemedText></View>
          : <View style={styles.center}><ThemedText style={styles.error}>{error || "帖子不存在"}</ThemedText><Pressable onPress={() => router.back()}><ThemedText style={styles.retry}>返回社区</ThemedText></Pressable></View>}
      </ThemedView>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  safeArea: { flex: 1 }, container: { flex: 1 }, header: { height: 58, flexDirection: "row", alignItems: "center", justifyContent: "space-between", borderBottomWidth: StyleSheet.hairlineWidth, paddingHorizontal: 10 }, backButton: { width: 42, height: 42, justifyContent: "center" }, headerSpacer: { width: 42 },
  center: { flex: 1, alignItems: "center", justifyContent: "center", gap: 10, padding: 30 }, muted: { opacity: 0.6 },
  postContent: { gap: 12, paddingBottom: 16 }, repliesTitle: { paddingHorizontal: 3 },
  error: { color: "#C23C3C", textAlign: "center" }, retry: { color: "#0A7EA4", fontWeight: "700" },
});
