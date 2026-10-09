import { Pressable, StyleSheet, View } from "react-native";

import { formatRelativeTime, postTypeLabel } from "@/components/community-post-card";
import { ThemedText } from "@/components/themed-text";
import { ThemedView } from "@/components/themed-view";
import { useColorScheme } from "@/hooks/use-color-scheme";
import type { CommunityPostResp } from "@/lib/dto";

export function CommunityPostListItem({
  post,
  onPress,
}: {
  post: CommunityPostResp;
  onPress: () => void;
}) {
  const isDark = useColorScheme() === "dark";

  return (
    <Pressable
      accessibilityRole="button"
      onPress={onPress}
      style={({ pressed }) => pressed ? styles.pressed : undefined}
    >
      <ThemedView
        style={[styles.item, { borderColor: isDark ? "#45535F" : "#CDD9E2" }]}
        lightColor="#F3F7FA"
        darkColor="#242E37"
      >
        <View style={styles.content}>
          <ThemedText type="defaultSemiBold" style={styles.title} numberOfLines={2}>
            {post.title}
          </ThemedText>
          <ThemedText style={styles.tags} numberOfLines={2}>
            #{post.section_name} · #{postTypeLabel(post.post_type)}
          </ThemedText>
        </View>
        <View style={styles.times}>
          <ThemedText style={styles.time}>
            发布 {formatRelativeTime(post.created_at)}
          </ThemedText>
          <ThemedText style={styles.time}>
            {post.discussion_count > 0n
              ? `最新回复 ${formatRelativeTime(post.last_activity_at)}`
              : "暂无回复"}
          </ThemedText>
        </View>
      </ThemedView>
    </Pressable>
  );
}

const styles = StyleSheet.create({
  item: { flexDirection: "row", alignItems: "center", gap: 12, borderRadius: 12, borderWidth: 1, paddingHorizontal: 14, paddingVertical: 18 },
  content: { flex: 1, minWidth: 0, gap: 7 },
  title: { fontSize: 17, lineHeight: 24, fontWeight: "700" },
  tags: { fontSize: 12, lineHeight: 17, opacity: 0.7 },
  times: { flexShrink: 0, alignItems: "flex-end", gap: 5 },
  time: { fontSize: 11, lineHeight: 17, opacity: 0.68, textAlign: "right" },
  pressed: { opacity: 0.72 },
});
