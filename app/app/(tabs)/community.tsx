import { FlashList } from "@shopify/flash-list";
import { useFocusEffect } from "@react-navigation/native";
import { router } from "expo-router";
import { useCallback, useRef, useState } from "react";
import {
  ActivityIndicator,
  Pressable,
  RefreshControl,
  ScrollView,
  StyleSheet,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";

import { CommunityPostCard } from "@/components/community-post-card";
import { ThemedText } from "@/components/themed-text";
import { ThemedView } from "@/components/themed-view";
import { IconSymbol } from "@/components/ui/icon-symbol";
import { useColorScheme } from "@/hooks/use-color-scheme";
import { hasAccessToken } from "@/lib/api-client";
import {
  listCommunityPosts,
  listCommunitySections,
  setPostInterested,
  type CommunityPostSort,
} from "@/lib/community-api";
import type { CommunityPostResp, CommunitySectionResp } from "@/lib/dto";

type LoadMode = "initial" | "refresh" | "more";

export default function CommunityScreen() {
  const isDark = useColorScheme() === "dark";
  const requestVersionRef = useRef(0);
  const sectionsRef = useRef<CommunitySectionResp[]>([]);
  const selectedSectionIdRef = useRef<string | null>(null);
  const sortRef = useRef<CommunityPostSort>("latest");
  const nextCursorRef = useRef<string | null>(null);
  const loadingMoreRef = useRef(false);
  const [sections, setSections] = useState<CommunitySectionResp[]>([]);
  const [posts, setPosts] = useState<CommunityPostResp[]>([]);
  const [selectedSectionId, setSelectedSectionId] = useState<string | null>(null);
  const [sort, setSort] = useState<CommunityPostSort>("latest");
  const [loading, setLoading] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [interestBusyIds, setInterestBusyIds] = useState<Set<string>>(new Set());
  const [error, setError] = useState("");

  const loadFeed = useCallback(
    async (
      mode: LoadMode,
      sectionId = selectedSectionIdRef.current,
      nextSort = sortRef.current,
    ) => {
      if (!hasAccessToken()) {
        router.replace({ pathname: "/login", params: { redirect: "/(tabs)/community" } } as never);
        return;
      }
      if (mode === "more" && (!nextCursorRef.current || loadingMoreRef.current)) {
        return;
      }
      const version = mode === "more" ? requestVersionRef.current : requestVersionRef.current + 1;
      if (mode !== "more") requestVersionRef.current = version;
      if (mode === "initial") setLoading(true);
      if (mode === "refresh") setRefreshing(true);
      if (mode === "more") {
        loadingMoreRef.current = true;
        setLoadingMore(true);
      }
      setError("");

      try {
        console.info("[community] loading feed", { mode, sectionId, sort: nextSort });
        const [sectionResp, postResp] = await Promise.all([
          sectionsRef.current.length || mode === "more"
            ? Promise.resolve(sectionsRef.current)
            : listCommunitySections(),
          listCommunityPosts({
            sectionId,
            sort: nextSort,
            cursor: mode === "more" ? nextCursorRef.current : null,
          }),
        ]);
        if (version !== requestVersionRef.current) return;
        sectionsRef.current = sectionResp;
        setSections(sectionResp);
        setPosts((current) => mode === "more" ? mergePosts(current, postResp.items) : postResp.items);
        nextCursorRef.current = postResp.next_cursor;
        console.info("[community] feed loaded", {
          mode,
          itemCount: postResp.items.length,
          hasMore: Boolean(postResp.next_cursor),
        });
      } catch (loadError) {
        if (version !== requestVersionRef.current) return;
        const message = loadError instanceof Error ? loadError.message : "社区内容加载失败";
        console.warn("[community] feed load failed", { mode, reason: message });
        setError(message);
      } finally {
        if (version === requestVersionRef.current) {
          setLoading(false);
          setRefreshing(false);
          loadingMoreRef.current = false;
          setLoadingMore(false);
        }
      }
    },
    [],
  );

  useFocusEffect(
    useCallback(() => {
      void loadFeed("initial");
      return () => {
        requestVersionRef.current += 1;
      };
    }, [loadFeed]),
  );

  const selectSection = useCallback((sectionId: string | null) => {
    selectedSectionIdRef.current = sectionId;
    setSelectedSectionId(sectionId);
    setPosts([]);
    nextCursorRef.current = null;
    void loadFeed("initial", sectionId, sortRef.current);
  }, [loadFeed]);

  const selectSort = useCallback((nextSort: CommunityPostSort) => {
    if (nextSort === sort) return;
    sortRef.current = nextSort;
    setSort(nextSort);
    setPosts([]);
    nextCursorRef.current = null;
    void loadFeed("initial", selectedSectionIdRef.current, nextSort);
  }, [loadFeed, sort]);

  const toggleInterested = useCallback(async (post: CommunityPostResp) => {
    if (interestBusyIds.has(post.post_id)) return;
    const desired = !post.viewer_interested;
    setInterestBusyIds((current) => new Set(current).add(post.post_id));
    // 立即反馈点击结果，服务端响应会用事务后的权威计数覆盖；失败则回滚。
    setPosts((current) => current.map((item) => item.post_id === post.post_id ? {
      ...item,
      viewer_interested: desired,
      interest_count: item.interest_count + (desired ? 1n : -1n),
    } : item));
    try {
      const result = await setPostInterested(post.post_id, desired);
      setPosts((current) => current.map((item) => item.post_id === post.post_id ? {
        ...item,
        viewer_interested: result.interested,
        interest_count: result.interest_count,
      } : item));
    } catch (reactionError) {
      console.warn("[community] interested reaction failed", {
        postId: post.post_id,
        desired,
        reason: reactionError instanceof Error ? reactionError.message : String(reactionError),
      });
      setPosts((current) => current.map((item) => item.post_id === post.post_id ? post : item));
      setError(reactionError instanceof Error ? reactionError.message : "操作失败");
    } finally {
      setInterestBusyIds((current) => {
        const next = new Set(current);
        next.delete(post.post_id);
        return next;
      });
    }
  }, [interestBusyIds]);

  return (
    <SafeAreaView style={styles.safeArea} edges={["top", "left", "right"]}>
      <ThemedView style={styles.container}>
        <View style={styles.header}>
          <View style={styles.headerCopy}>
            <ThemedText type="title">社区</ThemedText>
            <ThemedText style={styles.description}>
              从一个想法开始讨论，找到同好，再把讨论变成活动。
            </ThemedText>
          </View>
          <Pressable
            accessibilityRole="button"
            accessibilityLabel="发布帖子"
            onPress={() => router.push("/create-post" as never)}
            style={({ pressed }) => [styles.createButton, pressed ? styles.pressed : undefined]}
          >
            <IconSymbol name="square.and.pencil" size={20} color="#FFFFFF" />
          </Pressable>
        </View>

        <View style={styles.filterArea}>
          <ScrollView horizontal showsHorizontalScrollIndicator={false} contentContainerStyle={styles.chips}>
            <FilterChip label="全部" selected={!selectedSectionId} onPress={() => selectSection(null)} isDark={isDark} />
            {sections.map((section) => (
              <FilterChip key={section.section_id} label={section.name} selected={selectedSectionId === section.section_id} onPress={() => selectSection(section.section_id)} isDark={isDark} />
            ))}
          </ScrollView>
          <View style={styles.sortRow}>
            <SortButton label="最新发布" selected={sort === "latest"} onPress={() => selectSort("latest")} />
            <SortButton label="最近讨论" selected={sort === "active"} onPress={() => selectSort("active")} />
          </View>
        </View>

        {error ? (
          <Pressable onPress={() => void loadFeed("refresh")} style={styles.errorCard}>
            <ThemedText style={styles.errorText}>{error}，点击重试</ThemedText>
          </Pressable>
        ) : null}

        {loading && !posts.length ? (
          <View style={styles.centerState}>
            <ActivityIndicator color="#0A7EA4" />
            <ThemedText style={styles.muted}>正在加载社区内容...</ThemedText>
          </View>
        ) : (
          <FlashList
            data={posts}
            keyExtractor={(item) => item.post_id}
            renderItem={({ item }) => (
              <CommunityPostCard
                post={item}
                interestedBusy={interestBusyIds.has(item.post_id)}
                onInterestedPress={() => void toggleInterested(item)}
                onPress={() => router.push(`/community/post/${encodeURIComponent(item.post_id)}` as never)}
                onDiscussionPress={() => router.push(`/conversation/${encodeURIComponent(item.discussion_conversation_id)}` as never)}
              />
            )}
            ItemSeparatorComponent={() => <View style={styles.separator} />}
            contentContainerStyle={styles.listContent}
            refreshControl={<RefreshControl refreshing={refreshing} tintColor="#0A7EA4" onRefresh={() => void loadFeed("refresh")} />}
            onEndReached={() => void loadFeed("more")}
            onEndReachedThreshold={0.35}
            ListEmptyComponent={<View style={styles.centerState}><ThemedText type="defaultSemiBold">这里还没有帖子</ThemedText><ThemedText style={styles.muted}>发布第一个话题，邀请大家一起讨论吧</ThemedText></View>}
            ListFooterComponent={loadingMore ? <ActivityIndicator style={styles.footerLoader} color="#0A7EA4" /> : null}
          />
        )}
      </ThemedView>
    </SafeAreaView>
  );
}

function FilterChip({ label, selected, onPress, isDark }: { label: string; selected: boolean; onPress: () => void; isDark: boolean }) {
  return <Pressable onPress={onPress} style={[styles.chip, { backgroundColor: selected ? "#0A7EA4" : isDark ? "#20282F" : "#EDF2F5" }]}><ThemedText style={[styles.chipText, selected ? styles.chipTextSelected : undefined]}>{label}</ThemedText></Pressable>;
}

function SortButton({ label, selected, onPress }: { label: string; selected: boolean; onPress: () => void }) {
  return <Pressable onPress={onPress} hitSlop={8}><ThemedText style={[styles.sortText, selected ? styles.sortTextSelected : undefined]}>{label}</ThemedText></Pressable>;
}

function mergePosts(current: CommunityPostResp[], incoming: CommunityPostResp[]): CommunityPostResp[] {
  const seen = new Set(current.map((item) => item.post_id));
  return [...current, ...incoming.filter((item) => !seen.has(item.post_id))];
}

const styles = StyleSheet.create({
  safeArea: { flex: 1 },
  container: { flex: 1 },
  header: { paddingHorizontal: 16, paddingTop: 14, paddingBottom: 10, flexDirection: "row", alignItems: "center", gap: 12 },
  headerCopy: { flex: 1, gap: 6 },
  description: { fontSize: 14, lineHeight: 20, opacity: 0.7 },
  createButton: { width: 44, height: 44, borderRadius: 22, alignItems: "center", justifyContent: "center", backgroundColor: "#0A7EA4" },
  filterArea: { gap: 10, paddingBottom: 12 },
  chips: { paddingHorizontal: 16, gap: 8 },
  chip: { paddingHorizontal: 15, paddingVertical: 8, borderRadius: 999 },
  chipText: { fontSize: 14, lineHeight: 19 },
  chipTextSelected: { color: "#FFFFFF", fontWeight: "700" },
  sortRow: { flexDirection: "row", paddingHorizontal: 18, gap: 20 },
  sortText: { fontSize: 13, lineHeight: 18, opacity: 0.52 },
  sortTextSelected: { color: "#0A7EA4", opacity: 1, fontWeight: "700" },
  listContent: { paddingHorizontal: 12, paddingBottom: 28 },
  separator: { height: 10 },
  centerState: { padding: 36, alignItems: "center", gap: 10 },
  muted: { textAlign: "center", opacity: 0.62 },
  errorCard: { marginHorizontal: 16, marginBottom: 10, padding: 10, borderRadius: 9, backgroundColor: "#FFF0F0" },
  errorText: { color: "#C23C3C", fontSize: 13, lineHeight: 18, textAlign: "center" },
  footerLoader: { paddingVertical: 18 },
  pressed: { opacity: 0.7 },
});
