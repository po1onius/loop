import { Image } from "expo-image";
import { useEffect, useMemo, useState } from "react";
import { Pressable, StyleSheet, View } from "react-native";

import { ThemedText } from "@/components/themed-text";
import { ThemedView } from "@/components/themed-view";
import { UserAvatar } from "@/components/user-avatar";
import { useColorScheme } from "@/hooks/use-color-scheme";
import type { CommunityPostResp } from "@/lib/dto";
import { getMediaDownloadUrl } from "@/lib/media-api";

export function CommunityPostCard({
  post,
  onPress,
  onDiscussionPress,
  onInterestedPress,
  interestedBusy = false,
  compact = false,
}: {
  post: CommunityPostResp;
  onPress?: () => void;
  onDiscussionPress: () => void;
  onInterestedPress?: () => void;
  interestedBusy?: boolean;
  compact?: boolean;
}) {
  const isDark = useColorScheme() === "dark";
  const body = (
    <ThemedView
      style={styles.card}
      lightColor="#FFFFFF"
      darkColor="#1E252C"
    >
      <View style={styles.authorRow}>
        <UserAvatar
          username={post.author_username}
          avatarAssetId={post.author_avatar_asset_id}
        />
        <View style={styles.authorText}>
          <ThemedText type="defaultSemiBold" numberOfLines={1}>
            {post.author_username}
          </ThemedText>
          <ThemedText style={styles.metaText} numberOfLines={1}>
            {post.section_name} · {formatRelativeTime(post.created_at)}
            {post.edited_at ? " · 已编辑" : ""}
          </ThemedText>
        </View>
        <View
          style={[
            styles.typeBadge,
            { backgroundColor: isDark ? "#263B44" : "#EAF6FA" },
          ]}
        >
          <ThemedText style={styles.typeText}>
            {postTypeLabel(post.post_type)}
          </ThemedText>
        </View>
      </View>

      <ThemedText type="subtitle" style={styles.title} numberOfLines={2}>
        {post.title}
      </ThemedText>
      <ThemedText
        style={styles.body}
        numberOfLines={compact ? 3 : 6}
      >
        {post.body}
      </ThemedText>

      {!compact && post.image_asset_ids.length ? (
        <PostImageGrid assetIds={post.image_asset_ids} />
      ) : null}

      <View
        style={[
          styles.actions,
          { borderTopColor: isDark ? "#303941" : "#E9EDF1" },
        ]}
      >
        {onInterestedPress ? (
          <Pressable
            accessibilityRole="button"
            accessibilityState={{
              selected: post.viewer_interested,
              disabled: interestedBusy,
            }}
            disabled={interestedBusy}
            onPress={onInterestedPress}
            hitSlop={8}
            style={({ pressed }) => [
              styles.actionButton,
              pressed ? styles.pressed : undefined,
            ]}
          >
            <ThemedText
              style={[
                styles.actionText,
                post.viewer_interested ? styles.interestedText : undefined,
              ]}
            >
              {post.viewer_interested ? "♥" : "♡"} 感兴趣 {formatCount(post.interest_count)}
            </ThemedText>
          </Pressable>
        ) : (
          <ThemedText style={styles.actionText}>
            ♡ {formatCount(post.interest_count)}
          </ThemedText>
        )}

        <Pressable
          accessibilityRole="button"
          onPress={onDiscussionPress}
          hitSlop={8}
          style={({ pressed }) => [
            styles.discussionButton,
            pressed ? styles.pressed : undefined,
          ]}
        >
          <ThemedText style={styles.discussionText}>
            {formatCount(post.discussion_count)} 条讨论  ›
          </ThemedText>
        </Pressable>
      </View>
    </ThemedView>
  );

  return onPress ? (
    <Pressable
      accessibilityRole="button"
      onPress={onPress}
      style={({ pressed }) => pressed ? styles.cardPressed : undefined}
    >
      {body}
    </Pressable>
  ) : body;
}

function PostImageGrid({ assetIds }: { assetIds: string[] }) {
  const [uris, setUris] = useState<(string | null)[]>([]);
  const visibleAssetIds = useMemo(() => assetIds.slice(0, 4), [assetIds]);

  useEffect(() => {
    let active = true;
    setUris([]);
    void Promise.all(
      visibleAssetIds.map(async (assetId) => {
        try {
          return (await getMediaDownloadUrl(assetId)).download_url;
        } catch (error) {
          console.warn("[community-post-card] post image URL load failed", {
            assetId,
            reason: error instanceof Error ? error.message : String(error),
          });
          return null;
        }
      }),
    ).then((nextUris) => {
      if (active) {
        setUris(nextUris);
      }
    });
    return () => {
      active = false;
    };
  }, [visibleAssetIds]);

  if (!uris.some(Boolean)) {
    return null;
  }
  const columns = uris.length === 1 ? 1 : 2;
  return (
    <View style={styles.imageGrid}>
      {uris.map((uri, index) =>
        uri ? (
          <View
            key={visibleAssetIds[index]}
            style={[styles.imageCell, { width: columns === 1 ? "100%" : "49%" }]}
          >
            <Image source={uri} contentFit="cover" style={styles.postImage} />
            {index === 3 && assetIds.length > 4 ? (
              <View style={styles.imageMoreMask}>
                <ThemedText style={styles.imageMoreText}>
                  +{assetIds.length - 4}
                </ThemedText>
              </View>
            ) : null}
          </View>
        ) : null,
      )}
    </View>
  );
}

export function postTypeLabel(type: CommunityPostResp["post_type"]): string {
  switch (type) {
    case "event_idea":
      return "活动提议";
    case "event_discussion":
      return "活动见闻";
    case "general":
      return "日常讨论";
  }
}

export function formatRelativeTime(value: string): string {
  const time = new Date(value).getTime();
  const delta = Date.now() - time;
  if (!Number.isFinite(time) || delta < 0) {
    return "刚刚";
  }
  const minutes = Math.floor(delta / 60_000);
  if (minutes < 1) return "刚刚";
  if (minutes < 60) return `${minutes} 分钟前`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} 小时前`;
  const days = Math.floor(hours / 24);
  if (days < 7) return `${days} 天前`;
  return new Intl.DateTimeFormat("zh-CN", {
    month: "numeric",
    day: "numeric",
  }).format(new Date(time));
}

function formatCount(value: bigint): string {
  if (value < 1_000n) return value.toString();
  if (value < 10_000n) return `${(Number(value) / 1_000).toFixed(1)}k`;
  return `${Math.floor(Number(value) / 1_000)}k`;
}

const styles = StyleSheet.create({
  card: { borderRadius: 16, padding: 15, gap: 10 },
  cardPressed: { opacity: 0.82 },
  authorRow: { flexDirection: "row", alignItems: "center", gap: 9 },
  authorText: { flex: 1 },
  metaText: { fontSize: 12, lineHeight: 17, opacity: 0.58 },
  typeBadge: { borderRadius: 999, paddingHorizontal: 9, paddingVertical: 5 },
  typeText: { color: "#08789C", fontSize: 12, lineHeight: 15, fontWeight: "700" },
  title: { fontSize: 19, lineHeight: 25 },
  body: { fontSize: 15, lineHeight: 23, opacity: 0.84 },
  imageGrid: { flexDirection: "row", flexWrap: "wrap", gap: 6 },
  imageCell: { height: 150, borderRadius: 10, overflow: "hidden", backgroundColor: "#E9EDF1" },
  postImage: { width: "100%", height: "100%" },
  imageMoreMask: { ...StyleSheet.absoluteFillObject, alignItems: "center", justifyContent: "center", backgroundColor: "rgba(0,0,0,0.5)" },
  imageMoreText: { color: "#FFFFFF", fontSize: 24, fontWeight: "700" },
  actions: { flexDirection: "row", alignItems: "center", borderTopWidth: StyleSheet.hairlineWidth, paddingTop: 11, marginTop: 2 },
  actionButton: { paddingVertical: 2 },
  actionText: { fontSize: 14, lineHeight: 20, opacity: 0.68 },
  interestedText: { color: "#E34E65", opacity: 1, fontWeight: "600" },
  discussionButton: { marginLeft: "auto", paddingVertical: 2 },
  discussionText: { color: "#0A7EA4", fontSize: 14, lineHeight: 20, fontWeight: "700" },
  pressed: { opacity: 0.58 },
});
