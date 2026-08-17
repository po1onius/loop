import { useFocusEffect } from "@react-navigation/native";
import { router } from "expo-router";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ActivityIndicator,
  FlatList,
  Pressable,
  StyleSheet,
  type NativeScrollEvent,
  type NativeSyntheticEvent,
  useWindowDimensions,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";

import { ThemedText } from "@/components/themed-text";
import { ThemedView } from "@/components/themed-view";
import { IconSymbol } from "@/components/ui/icon-symbol";
import type { EventResp } from "@/lib/dto";
import { listEvents } from "@/lib/event-api";

type CarouselItem = {
  id: string;
  title: string;
  subtitle: string;
  color: string;
};

type ListItem = {
  description: string;
  eventId?: string;
  id: string;
  title: string;
};

type EventLoadMode = "background" | "refresh";

const CAROUSEL_ITEMS: CarouselItem[] = [
  {
    id: "1",
    title: "本周活动",
    subtitle: "发现附近正在招募的兴趣活动",
    color: "#3F8EFC",
  },
  {
    id: "2",
    title: "今日精选",
    subtitle: "技术交流、摄影外拍和城市徒步",
    color: "#12B886",
  },
  {
    id: "3",
    title: "热门主题",
    subtitle: "按地点、时间和兴趣快速筛选",
    color: "#F08C00",
  },
];

const FALLBACK_ITEMS: ListItem[] = [
  {
    id: "1",
    title: "Web3 技术交流会",
    description: "周六 14:00 · 南山科技园 · 32 人已报名",
  },
  {
    id: "2",
    title: "城市摄影外拍",
    description: "周日 16:30 · 滨海步道 · 18 人已报名",
  },
  {
    id: "3",
    title: "手作烘焙体验",
    description: "下周三 19:00 · 福田中心区 · 12 人已报名",
  },
  {
    id: "4",
    title: "独立游戏试玩夜",
    description: "下周五 20:00 · 创意园 · 24 人已报名",
  },
  {
    id: "5",
    title: "户外飞盘新手局",
    description: "周六 09:30 · 深圳湾公园 · 40 人已报名",
  },
];

export default function HomeScreen() {
  const { width } = useWindowDimensions();
  const carouselRef = useRef<FlatList<CarouselItem>>(null);
  const latestEventsRequestIdRef = useRef(0);
  const activeRefreshRequestIdRef = useRef<number | null>(null);
  const [activeIndex, setActiveIndex] = useState(0);
  const [events, setEvents] = useState<EventResp[]>([]);
  const [loadingEvents, setLoadingEvents] = useState(false);
  const [refreshingEvents, setRefreshingEvents] = useState(false);
  const [eventsError, setEventsError] = useState("");
  const slideWidth = Math.max(width - 32, 1);

  const loadEvents = useCallback(async (mode: EventLoadMode = "background") => {
    const requestId = latestEventsRequestIdRef.current + 1;
    latestEventsRequestIdRef.current = requestId;
    if (mode === "refresh") {
      activeRefreshRequestIdRef.current = requestId;
      setRefreshingEvents(true);
    } else {
      setLoadingEvents(true);
    }
    setEventsError("");
    try {
      console.info("[home] loading events", {
        mode,
        requestId,
      });
      const resp = await listEvents();
      if (latestEventsRequestIdRef.current !== requestId) {
        console.info("[home] ignored stale events response", {
          mode,
          requestId,
        });
        return;
      }
      setEvents(resp.items);
      console.info("[home] events loaded", {
        requestId,
        count: resp.items.length,
      });
    } catch (e) {
      if (latestEventsRequestIdRef.current !== requestId) {
        return;
      }
      setEventsError(e instanceof Error ? e.message : "活动列表加载失败");
    } finally {
      if (
        mode === "refresh" &&
        activeRefreshRequestIdRef.current === requestId
      ) {
        activeRefreshRequestIdRef.current = null;
        setRefreshingEvents(false);
      }
      if (latestEventsRequestIdRef.current === requestId) {
        setLoadingEvents(false);
      }
    }
  }, []);

  useFocusEffect(
    useCallback(() => {
      // 页面重新获得焦点时静默刷新数据，避免触发 FlatList 的下拉刷新动画。
      void loadEvents("background");
      return () => {
        latestEventsRequestIdRef.current += 1;
        activeRefreshRequestIdRef.current = null;
        setRefreshingEvents(false);
        setLoadingEvents(false);
      };
    }, [loadEvents]),
  );

  const handleRefreshEvents = useCallback(() => {
    void loadEvents("refresh");
  }, [loadEvents]);

  useEffect(() => {
    const timer = setInterval(() => {
      setActiveIndex((prevIndex) => {
        const nextIndex = (prevIndex + 1) % CAROUSEL_ITEMS.length;
        carouselRef.current?.scrollToIndex({ index: nextIndex, animated: true });
        return nextIndex;
      });
    }, 3500);

    return () => {
      clearInterval(timer);
    };
  }, []);

  const listItems = useMemo(() => {
    if (!events.length) {
      return FALLBACK_ITEMS;
    }
    return events.map((event) => ({
      description: formatEventDescription(event),
      eventId: event.event_id,
      id: event.event_id,
      title: event.title,
    }));
  }, [events]);

  const handleCarouselScrollEnd = (
    event: NativeSyntheticEvent<NativeScrollEvent>,
  ) => {
    const index = Math.round(event.nativeEvent.contentOffset.x / slideWidth);
    const boundedIndex = Math.max(0, Math.min(index, CAROUSEL_ITEMS.length - 1));
    setActiveIndex(boundedIndex);
  };

  const handleEventPress = useCallback((item: ListItem) => {
    if (!item.eventId) {
      console.info("[home] fallback event pressed without detail", {
        itemId: item.id,
        title: item.title,
      });
      setEventsError("演示活动暂无详情，请启动后端并刷新真实活动列表");
      return;
    }

    console.info("[home] event item pressed", {
      eventId: item.eventId,
      title: item.title,
    });
    router.push(`/event/${encodeURIComponent(item.eventId)}` as never);
  }, []);

  return (
    <SafeAreaView style={styles.safeArea} edges={["top", "left", "right"]}>
      <ThemedView style={styles.container}>
        <ThemedView style={styles.topSection}>
          <View style={styles.homeHeader}>
            <ThemedText type="subtitle" style={styles.homeHeaderTitle}>
              活动推荐
            </ThemedText>
          </View>
          <FlatList
            ref={carouselRef}
            data={CAROUSEL_ITEMS}
            horizontal
            pagingEnabled
            showsHorizontalScrollIndicator={false}
            keyExtractor={(item) => item.id}
            style={styles.carousel}
            onMomentumScrollEnd={handleCarouselScrollEnd}
            getItemLayout={(_, index) => ({
              length: slideWidth,
              offset: slideWidth * index,
              index,
            })}
            renderItem={({ item }) => (
              <ThemedView style={[styles.slide, { width: slideWidth, backgroundColor: item.color }]}>
                <ThemedText style={styles.slideTitle}>{item.title}</ThemedText>
                <ThemedText style={styles.slideSubtitle}>{item.subtitle}</ThemedText>
              </ThemedView>
            )}
          />

          <View style={styles.dotsContainer}>
            {CAROUSEL_ITEMS.map((item, index) => (
              <View
                key={item.id}
                style={[styles.dot, index === activeIndex ? styles.dotActive : null]}
              />
            ))}
          </View>
        </ThemedView>

        <ThemedView style={styles.bottomSection}>
          <View style={styles.sectionHeader}>
            <ThemedView style={styles.sectionTitleWrap}>
              <ThemedText type="subtitle">近期活动</ThemedText>
              {eventsError ? (
                <ThemedText style={styles.errorText}>{eventsError}</ThemedText>
              ) : null}
            </ThemedView>
            <Pressable
              accessibilityRole="button"
              style={styles.createButton}
              onPress={() => router.push("/create-event" as never)}
            >
              <IconSymbol size={18} name="square.and.pencil" color="#FFFFFF" />
              <ThemedText style={styles.createButtonText}>发布</ThemedText>
            </Pressable>
          </View>
          <FlatList
            data={listItems}
            keyExtractor={(item) => item.id}
            style={styles.list}
            showsVerticalScrollIndicator={false}
            contentContainerStyle={styles.listContent}
            refreshing={refreshingEvents}
            onRefresh={handleRefreshEvents}
            ListEmptyComponent={
              <EventListEmptyState loading={loadingEvents} />
            }
            renderItem={({ item }) => (
              <Pressable
                accessibilityRole="button"
                onPress={() => handleEventPress(item)}
                style={({ pressed }) => [
                  styles.listPressable,
                  pressed ? styles.listPressablePressed : undefined,
                ]}
              >
                <ThemedView
                  style={styles.listCard}
                  lightColor="#F3F6FA"
                  darkColor="#1E252C"
                >
                  <View style={styles.listCardHeader}>
                    <ThemedText
                      numberOfLines={1}
                      type="defaultSemiBold"
                      style={styles.listCardTitle}
                    >
                      {item.title}
                    </ThemedText>
                    <IconSymbol size={18} name="chevron.right" color="#8A94A6" />
                  </View>
                  <ThemedText numberOfLines={2}>{item.description}</ThemedText>
                </ThemedView>
              </Pressable>
            )}
          />
        </ThemedView>
      </ThemedView>
    </SafeAreaView>
  );
}

function EventListEmptyState({
  loading,
}: {
  loading: boolean;
}) {
  if (loading) {
    return (
      <View style={styles.eventEmptyState}>
        <ActivityIndicator color="#0A7EA4" />
        <ThemedText style={styles.eventEmptyText}>正在加载活动...</ThemedText>
      </View>
    );
  }
  return (
    <View style={styles.eventEmptyState}>
      <ThemedText type="defaultSemiBold">暂时没有近期活动</ThemedText>
      <ThemedText style={styles.eventEmptyText}>下拉刷新后再看看</ThemedText>
    </View>
  );
}

function formatEventDescription(event: EventResp): string {
  const parts = [
    event.start_at ? formatDateTime(event.start_at) : undefined,
    event.location_name,
    event.summary,
  ].filter(Boolean);
  return parts.join(" · ") || "暂无活动简介";
}

function formatDateTime(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

const styles = StyleSheet.create({
  safeArea: {
    flex: 1,
  },
  container: {
    flex: 1,
  },
  topSection: {
    height: "40%",
    paddingHorizontal: 16,
    paddingTop: 8,
    paddingBottom: 8,
  },
  homeHeader: {
    minHeight: 44,
    flexDirection: "row",
    alignItems: "center",
    gap: 10,
  },
  homeHeaderTitle: {
    flex: 1,
  },
  carousel: {
    flex: 1,
    marginTop: 10,
  },
  slide: {
    flex: 1,
    borderRadius: 16,
    paddingHorizontal: 20,
    paddingVertical: 18,
    justifyContent: "center",
  },
  slideTitle: {
    color: "#FFFFFF",
    fontSize: 24,
    lineHeight: 30,
    fontWeight: "700",
  },
  slideSubtitle: {
    color: "#FFFFFF",
    marginTop: 8,
    fontSize: 15,
    lineHeight: 22,
  },
  dotsContainer: {
    marginTop: 10,
    flexDirection: "row",
    justifyContent: "center",
    alignItems: "center",
    gap: 6,
  },
  dot: {
    width: 8,
    height: 8,
    borderRadius: 99,
    backgroundColor: "#B9C0CC",
  },
  dotActive: {
    width: 18,
    backgroundColor: "#0A7EA4",
  },
  bottomSection: {
    flex: 1,
    paddingHorizontal: 16,
    paddingTop: 10,
  },
  sectionHeader: {
    flexDirection: "row",
    alignItems: "center",
    gap: 12,
  },
  sectionTitleWrap: {
    flex: 1,
  },
  createButton: {
    height: 38,
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
    gap: 6,
    borderRadius: 10,
    paddingHorizontal: 12,
    backgroundColor: "#0A7EA4",
  },
  createButtonText: {
    color: "#FFFFFF",
    fontSize: 14,
    lineHeight: 18,
    fontWeight: "700",
  },
  errorText: {
    color: "#D64545",
    fontSize: 12,
    lineHeight: 16,
    marginTop: 2,
  },
  list: {
    marginTop: 10,
  },
  listContent: {
    paddingBottom: 24,
    gap: 10,
  },
  eventEmptyState: {
    minHeight: 150,
    alignItems: "center",
    justifyContent: "center",
    gap: 8,
    paddingHorizontal: 24,
  },
  eventEmptyText: {
    color: "#687076",
    textAlign: "center",
  },
  listPressable: {
    borderRadius: 12,
  },
  listPressablePressed: {
    opacity: 0.72,
  },
  listCard: {
    borderRadius: 12,
    paddingHorizontal: 14,
    paddingVertical: 12,
    gap: 4,
  },
  listCardHeader: {
    minHeight: 24,
    flexDirection: "row",
    alignItems: "center",
    gap: 8,
  },
  listCardTitle: {
    flex: 1,
  },
});
