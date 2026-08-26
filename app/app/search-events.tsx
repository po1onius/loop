import { router } from "expo-router";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ActivityIndicator,
  FlatList,
  Pressable,
  ScrollView,
  StyleSheet,
  TextInput,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";

import { ThemedText } from "@/components/themed-text";
import { ThemedView } from "@/components/themed-view";
import { IconSymbol } from "@/components/ui/icon-symbol";
import { Colors } from "@/constants/theme";
import { useColorScheme } from "@/hooks/use-color-scheme";
import type { EventResp, EventSearchFacetResp } from "@/lib/dto";
import { searchEvents } from "@/lib/event-api";

type TimePreset = "all" | "today" | "next_7_days" | "next_30_days";

const PAGE_SIZE = 20;
const SEARCH_DEBOUNCE_MS = 350;
const TIME_PRESETS: { label: string; value: TimePreset }[] = [
  { label: "全部时间", value: "all" },
  { label: "今天", value: "today" },
  { label: "未来 7 天", value: "next_7_days" },
  { label: "未来 30 天", value: "next_30_days" },
];

export default function SearchEventsScreen() {
  const colorScheme = useColorScheme() === "dark" ? "dark" : "light";
  const latestRequestIdRef = useRef(0);
  const [query, setQuery] = useState("");
  const [selectedTags, setSelectedTags] = useState<string[]>([]);
  const [selectedLocation, setSelectedLocation] = useState<string | null>(null);
  const [timePreset, setTimePreset] = useState<TimePreset>("all");
  const [events, setEvents] = useState<EventResp[]>([]);
  const [tagFacets, setTagFacets] = useState<EventSearchFacetResp[]>([]);
  const [locationFacets, setLocationFacets] = useState<EventSearchFacetResp[]>([]);
  const [estimatedTotal, setEstimatedTotal] = useState(0);
  const [nextOffset, setNextOffset] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState("");

  const executeSearch = useCallback(
    async (offset: number, append: boolean) => {
      const requestId = latestRequestIdRef.current + 1;
      latestRequestIdRef.current = requestId;
      if (append) {
        setLoadingMore(true);
      } else {
        setLoading(true);
      }
      setError("");
      const timeRange = buildTimeRange(timePreset);
      try {
        console.info("[search-events] search started", {
          requestId,
          queryLength: query.trim().length,
          selectedTags,
          selectedLocation,
          timePreset,
          offset,
          append,
        });
        const response = await searchEvents({
          query,
          tags: selectedTags,
          location: selectedLocation ?? undefined,
          startFrom: timeRange.startFrom,
          startTo: timeRange.startTo,
          limit: PAGE_SIZE,
          offset,
        });
        if (latestRequestIdRef.current !== requestId) {
          console.info("[search-events] stale search response ignored", {
            requestId,
          });
          return;
        }
        setEvents((current) =>
          append ? mergeEvents(current, response.items) : response.items,
        );
        setTagFacets(response.tags);
        setLocationFacets(response.locations);
        setEstimatedTotal(response.estimated_total);
        setNextOffset(response.next_offset);
        console.info("[search-events] search completed", {
          requestId,
          resultCount: response.items.length,
          estimatedTotal: response.estimated_total,
          nextOffset: response.next_offset,
        });
      } catch (searchError) {
        if (latestRequestIdRef.current !== requestId) return;
        const message =
          searchError instanceof Error ? searchError.message : "搜索活动失败";
        setError(message);
        if (!append) {
          setEvents([]);
          setEstimatedTotal(0);
          setNextOffset(null);
        }
        console.warn("[search-events] search failed", {
          requestId,
          message,
        });
      } finally {
        if (latestRequestIdRef.current === requestId) {
          setLoading(false);
          setLoadingMore(false);
        }
      }
    }, [query, selectedLocation, selectedTags, timePreset],
  );

  useEffect(() => {
    // 关键词输入使用短防抖，标签、地点或时间改变也走同一搜索快照，避免多个
    // 条件变化时旧响应覆盖新结果。
    const timer = setTimeout(() => {
      void executeSearch(0, false);
    }, SEARCH_DEBOUNCE_MS);
    return () => {
      clearTimeout(timer);
      latestRequestIdRef.current += 1;
    };
  }, [executeSearch]);

  const visibleTagFacets = useMemo(
    () => includeSelectedFacets(tagFacets, selectedTags),
    [selectedTags, tagFacets],
  );
  const visibleLocationFacets = useMemo(
    () =>
      includeSelectedFacets(
        locationFacets,
        selectedLocation ? [selectedLocation] : [],
      ),
    [locationFacets, selectedLocation],
  );
  const hasFilters =
    selectedTags.length > 0 || selectedLocation !== null || timePreset !== "all";

  const toggleTag = useCallback((tag: string) => {
    setSelectedTags((current) =>
      current.includes(tag)
        ? current.filter((value) => value !== tag)
        : [...current, tag],
    );
  }, []);

  const clearFilters = useCallback(() => {
    console.info("[search-events] clearing filters");
    setSelectedTags([]);
    setSelectedLocation(null);
    setTimePreset("all");
  }, []);

  const loadMore = useCallback(() => {
    if (loading || loadingMore || nextOffset === null) return;
    void executeSearch(nextOffset, true);
  }, [executeSearch, loading, loadingMore, nextOffset]);

  return (
    <SafeAreaView style={styles.safeArea} edges={["top", "left", "right"]}>
      <ThemedView style={styles.container}>
        <View style={styles.header}>
          <Pressable
            accessibilityLabel="返回"
            accessibilityRole="button"
            onPress={() => router.back()}
            style={({ pressed }) => [
              styles.iconButton,
              pressed ? styles.pressed : undefined,
            ]}
          >
            <IconSymbol
              name="chevron.left"
              size={26}
              color={Colors[colorScheme].text}
            />
          </Pressable>
          <ThemedText type="subtitle" style={styles.headerTitle}>
            搜索活动
          </ThemedText>
          <View style={styles.headerSpacer} />
        </View>

        <View
          style={[
            styles.searchBox,
            colorScheme === "dark" ? styles.searchBoxDark : undefined,
          ]}
        >
          <IconSymbol name="magnifyingglass" size={21} color="#7B8794" />
          <TextInput
            accessibilityLabel="活动关键词"
            autoCapitalize="none"
            autoCorrect={false}
            onChangeText={setQuery}
            onSubmitEditing={() => void executeSearch(0, false)}
            placeholder="搜索标题、介绍、地点或标签"
            placeholderTextColor="#8A94A6"
            returnKeyType="search"
            style={[styles.searchInput, { color: Colors[colorScheme].text }]}
            value={query}
          />
          {query ? (
            <Pressable
              accessibilityRole="button"
              onPress={() => setQuery("")}
              style={styles.clearQueryButton}
            >
              <ThemedText style={styles.clearQueryText}>清除</ThemedText>
            </Pressable>
          ) : null}
        </View>

        <FlatList
          data={events}
          keyExtractor={(item) => item.event_id}
          keyboardShouldPersistTaps="handled"
          onEndReached={loadMore}
          onEndReachedThreshold={0.35}
          contentContainerStyle={styles.listContent}
          ListHeaderComponent={
            <View style={styles.filters}>
              <FilterHeader
                estimatedTotal={estimatedTotal}
                hasFilters={hasFilters}
                onClear={clearFilters}
              />
              <ThemedText type="defaultSemiBold" style={styles.filterLabel}>
                时间
              </ThemedText>
              <ScrollView
                horizontal
                showsHorizontalScrollIndicator={false}
                contentContainerStyle={styles.chipRow}
              >
                {TIME_PRESETS.map((preset) => (
                  <FilterChip
                    key={preset.value}
                    label={preset.label}
                    selected={timePreset === preset.value}
                    onPress={() => setTimePreset(preset.value)}
                  />
                ))}
              </ScrollView>

              {visibleLocationFacets.length ? (
                <>
                  <ThemedText type="defaultSemiBold" style={styles.filterLabel}>
                    地点
                  </ThemedText>
                  <ScrollView
                    horizontal
                    showsHorizontalScrollIndicator={false}
                    contentContainerStyle={styles.chipRow}
                  >
                    {visibleLocationFacets.map((facet) => (
                      <FilterChip
                        key={facet.value}
                        label={`${facet.value} ${facet.count}`}
                        selected={selectedLocation === facet.value}
                        onPress={() =>
                          setSelectedLocation((current) =>
                            current === facet.value ? null : facet.value,
                          )
                        }
                      />
                    ))}
                  </ScrollView>
                </>
              ) : null}

              {visibleTagFacets.length ? (
                <>
                  <ThemedText type="defaultSemiBold" style={styles.filterLabel}>
                    标签
                  </ThemedText>
                  <ScrollView
                    horizontal
                    showsHorizontalScrollIndicator={false}
                    contentContainerStyle={styles.chipRow}
                  >
                    {visibleTagFacets.map((facet) => (
                      <FilterChip
                        key={facet.value}
                        label={`#${facet.value} ${facet.count}`}
                        selected={selectedTags.includes(facet.value)}
                        onPress={() => toggleTag(facet.value)}
                      />
                    ))}
                  </ScrollView>
                </>
              ) : null}
            </View>
          }
          ListEmptyComponent={
            <SearchEmptyState loading={loading} error={error} />
          }
          ListFooterComponent={
            loadingMore ? (
              <View style={styles.loadingMore}>
                <ActivityIndicator color="#0A7EA4" />
                <ThemedText style={styles.mutedText}>正在加载更多...</ThemedText>
              </View>
            ) : null
          }
          renderItem={({ item }) => (
            <Pressable
              accessibilityRole="button"
              onPress={() => {
                console.info("[search-events] result opened", {
                  eventId: item.event_id,
                  title: item.title,
                });
                router.push(`/event/${encodeURIComponent(item.event_id)}` as never);
              }}
              style={({ pressed }) => [
                styles.resultPressable,
                pressed ? styles.pressed : undefined,
              ]}
            >
              <ThemedView
                lightColor="#F3F6FA"
                darkColor="#1E252C"
                style={styles.resultCard}
              >
                <View style={styles.resultTitleRow}>
                  <ThemedText
                    type="defaultSemiBold"
                    numberOfLines={1}
                    style={styles.resultTitle}
                  >
                    {item.title}
                  </ThemedText>
                  <IconSymbol name="chevron.right" size={18} color="#8A94A6" />
                </View>
                <ThemedText numberOfLines={2} style={styles.resultSummary}>
                  {formatEventDescription(item)}
                </ThemedText>
                {item.tags.length ? (
                  <ThemedText numberOfLines={1} style={styles.resultTags}>
                    {item.tags.map((tag) => `#${tag}`).join("  ")}
                  </ThemedText>
                ) : null}
              </ThemedView>
            </Pressable>
          )}
        />
      </ThemedView>
    </SafeAreaView>
  );
}

function FilterHeader({
  estimatedTotal,
  hasFilters,
  onClear,
}: {
  estimatedTotal: number;
  hasFilters: boolean;
  onClear: () => void;
}) {
  return (
    <View style={styles.filterHeader}>
      <ThemedText style={styles.resultCount}>
        找到 {estimatedTotal} 个活动
      </ThemedText>
      {hasFilters ? (
        <Pressable accessibilityRole="button" onPress={onClear}>
          <ThemedText style={styles.clearFiltersText}>清除筛选</ThemedText>
        </Pressable>
      ) : null}
    </View>
  );
}

function FilterChip({
  label,
  selected,
  onPress,
}: {
  label: string;
  selected: boolean;
  onPress: () => void;
}) {
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityState={{ selected }}
      onPress={onPress}
      style={({ pressed }) => [
        styles.chip,
        selected ? styles.chipSelected : undefined,
        pressed ? styles.pressed : undefined,
      ]}
    >
      <ThemedText style={selected ? styles.chipTextSelected : styles.chipText}>
        {label}
      </ThemedText>
    </Pressable>
  );
}

function SearchEmptyState({ loading, error }: { loading: boolean; error: string }) {
  if (loading) {
    return (
      <View style={styles.emptyState}>
        <ActivityIndicator color="#0A7EA4" />
        <ThemedText style={styles.mutedText}>正在搜索活动...</ThemedText>
      </View>
    );
  }
  return (
    <View style={styles.emptyState}>
      <ThemedText type="defaultSemiBold">
        {error ? "搜索暂时不可用" : "没有找到匹配活动"}
      </ThemedText>
      <ThemedText style={[styles.mutedText, error ? styles.errorText : undefined]}>
        {error || "换个关键词或减少筛选条件再试试"}
      </ThemedText>
    </View>
  );
}

function includeSelectedFacets(
  facets: EventSearchFacetResp[],
  selected: string[],
): EventSearchFacetResp[] {
  const values = new Set(facets.map((facet) => facet.value));
  return [
    ...selected
      .filter((value) => !values.has(value))
      .map((value) => ({ value, count: 0 })),
    ...facets,
  ];
}

function mergeEvents(current: EventResp[], incoming: EventResp[]): EventResp[] {
  const merged = [...current];
  const seen = new Set(current.map((event) => event.event_id));
  for (const event of incoming) {
    if (!seen.has(event.event_id)) {
      seen.add(event.event_id);
      merged.push(event);
    }
  }
  return merged;
}

function buildTimeRange(preset: TimePreset): {
  startFrom?: string;
  startTo?: string;
} {
  if (preset === "all") return {};
  const now = new Date();
  if (preset === "today") {
    const start = new Date(now);
    start.setHours(0, 0, 0, 0);
    const end = new Date(now);
    end.setHours(23, 59, 59, 999);
    return { startFrom: start.toISOString(), startTo: end.toISOString() };
  }
  const end = new Date(now);
  end.setDate(end.getDate() + (preset === "next_7_days" ? 7 : 30));
  return { startFrom: now.toISOString(), startTo: end.toISOString() };
}

function formatEventDescription(event: EventResp): string {
  const parts = [
    event.start_at ? formatDateTime(event.start_at) : undefined,
    event.location_name ?? undefined,
    event.summary || undefined,
  ].filter((value): value is string => Boolean(value));
  return parts.join(" · ") || "暂无活动简介";
}

function formatDateTime(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

const styles = StyleSheet.create({
  safeArea: { flex: 1 },
  container: { flex: 1 },
  header: {
    height: 52,
    paddingHorizontal: 12,
    flexDirection: "row",
    alignItems: "center",
  },
  headerTitle: { flex: 1, textAlign: "center" },
  headerSpacer: { width: 40 },
  iconButton: {
    width: 40,
    height: 40,
    alignItems: "center",
    justifyContent: "center",
    borderRadius: 20,
  },
  searchBox: {
    minHeight: 48,
    marginHorizontal: 16,
    marginBottom: 8,
    paddingHorizontal: 14,
    borderRadius: 14,
    backgroundColor: "#F0F3F6",
    flexDirection: "row",
    alignItems: "center",
    gap: 9,
  },
  searchBoxDark: { backgroundColor: "#23292F" },
  searchInput: { flex: 1, minHeight: 46, fontSize: 16, paddingVertical: 10 },
  clearQueryButton: { paddingVertical: 8, paddingLeft: 6 },
  clearQueryText: { color: "#0A7EA4", fontSize: 13 },
  listContent: { paddingHorizontal: 16, paddingBottom: 32, gap: 10 },
  filters: { paddingBottom: 4 },
  filterHeader: {
    height: 38,
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
  },
  resultCount: { color: "#687076", fontSize: 13 },
  clearFiltersText: { color: "#0A7EA4", fontSize: 13, fontWeight: "600" },
  filterLabel: { marginTop: 8, marginBottom: 7, fontSize: 14 },
  chipRow: { gap: 8, paddingRight: 12 },
  chip: {
    minHeight: 34,
    justifyContent: "center",
    paddingHorizontal: 12,
    borderRadius: 17,
    borderWidth: 1,
    borderColor: "#CDD5DF",
  },
  chipSelected: { borderColor: "#0A7EA4", backgroundColor: "#0A7EA4" },
  chipText: { fontSize: 13 },
  chipTextSelected: { color: "#FFFFFF", fontSize: 13, fontWeight: "600" },
  resultPressable: { borderRadius: 12 },
  resultCard: { borderRadius: 12, padding: 14, gap: 6 },
  resultTitleRow: { flexDirection: "row", alignItems: "center", gap: 8 },
  resultTitle: { flex: 1 },
  resultSummary: { lineHeight: 20 },
  resultTags: { color: "#0A7EA4", fontSize: 12, lineHeight: 18 },
  pressed: { opacity: 0.7 },
  emptyState: {
    minHeight: 220,
    paddingHorizontal: 24,
    alignItems: "center",
    justifyContent: "center",
    gap: 9,
  },
  loadingMore: {
    paddingVertical: 20,
    alignItems: "center",
    justifyContent: "center",
    gap: 8,
  },
  mutedText: { color: "#687076", textAlign: "center" },
  errorText: { color: "#D64545" },
});
