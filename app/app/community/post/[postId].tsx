import { router, useLocalSearchParams } from "expo-router";
import { useEffect, useState } from "react";
import { ActivityIndicator, Pressable, ScrollView, StyleSheet, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";

import { CommunityPostCard } from "@/components/community-post-card";
import { ThemedText } from "@/components/themed-text";
import { ThemedView } from "@/components/themed-view";
import { IconSymbol } from "@/components/ui/icon-symbol";
import { useColorScheme } from "@/hooks/use-color-scheme";
import { getCommunityPost, setPostInterested } from "@/lib/community-api";
import type { CommunityPostResp } from "@/lib/dto";

export default function CommunityPostDetailScreen() {
  const isDark = useColorScheme() === "dark";
  const params = useLocalSearchParams<{ postId?: string | string[] }>();
  const postId = Array.isArray(params.postId) ? params.postId[0] : params.postId;
  const [post, setPost] = useState<CommunityPostResp | null>(null);
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
    void getCommunityPost(postId)
      .then((nextPost) => active && setPost(nextPost))
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

  const toggleInterested = async () => {
    if (!post || reactionBusy) return;
    setReactionBusy(true);
    setError("");
    try {
      const reaction = await setPostInterested(post.post_id, !post.viewer_interested);
      setPost({ ...post, viewer_interested: reaction.interested, interest_count: reaction.interest_count });
    } catch (reactionError) {
      const message = reactionError instanceof Error ? reactionError.message : "操作失败";
      console.warn("[community-post-detail] interested reaction failed", { postId: post.post_id, reason: message });
      setError(message);
    } finally {
      setReactionBusy(false);
    }
  };

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
        {loading ? <View style={styles.center}><ActivityIndicator color="#0A7EA4" /><ThemedText style={styles.muted}>正在加载帖子...</ThemedText></View> : post ? (
          <ScrollView contentContainerStyle={styles.content}>
            <CommunityPostCard
              post={post}
              interestedBusy={reactionBusy}
              onInterestedPress={() => void toggleInterested()}
              onDiscussionPress={() => router.push(`/conversation/${encodeURIComponent(post.discussion_conversation_id)}` as never)}
            />
            <Pressable accessibilityRole="button" onPress={() => router.push(`/conversation/${encodeURIComponent(post.discussion_conversation_id)}` as never)} style={({ pressed }) => [styles.enterDiscussion, pressed ? styles.pressed : undefined]}>
              <View style={styles.discussionCopy}>
                <ThemedText style={styles.enterTitle}>进入讨论线程</ThemedText>
                <ThemedText style={styles.enterDescription}>像群聊一样实时交流，不需要先加入群组</ThemedText>
              </View>
              <ThemedText style={styles.enterArrow}>›</ThemedText>
            </Pressable>
            {error ? <ThemedText style={styles.error}>{error}</ThemedText> : null}
          </ScrollView>
        ) : <View style={styles.center}><ThemedText style={styles.error}>{error || "帖子不存在"}</ThemedText><Pressable onPress={() => router.back()}><ThemedText style={styles.retry}>返回社区</ThemedText></Pressable></View>}
      </ThemedView>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  safeArea: { flex: 1 }, container: { flex: 1 }, header: { height: 58, flexDirection: "row", alignItems: "center", justifyContent: "space-between", borderBottomWidth: StyleSheet.hairlineWidth, paddingHorizontal: 10 }, backButton: { width: 42, height: 42, justifyContent: "center" }, headerSpacer: { width: 42 },
  center: { flex: 1, alignItems: "center", justifyContent: "center", gap: 10, padding: 30 }, muted: { opacity: 0.6 }, content: { padding: 12, gap: 12, paddingBottom: 30 },
  enterDiscussion: { flexDirection: "row", alignItems: "center", borderRadius: 14, padding: 16, backgroundColor: "#0A7EA4" }, discussionCopy: { flex: 1 }, enterTitle: { color: "#FFFFFF", fontWeight: "800" }, enterDescription: { color: "#DDF5FC", fontSize: 12, lineHeight: 17 }, enterArrow: { color: "#FFFFFF", fontSize: 30 },
  error: { color: "#C23C3C", textAlign: "center" }, retry: { color: "#0A7EA4", fontWeight: "700" }, pressed: { opacity: 0.72 },
});
