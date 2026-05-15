import { router } from "expo-router";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
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
import type { EventResp } from "@/lib/dto";
import { listEvents } from "@/lib/event-api";

type CarouselItem = {
  id: string;
  title: string;
  subtitle: string;
  color: string;
};

type ListItem = {
  id: string;
  title: string;
  description: string;
};

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
  const [activeIndex, setActiveIndex] = useState(0);
  const [events, setEvents] = useState<EventResp[]>([]);
  const [loadingEvents, setLoadingEvents] = useState(false);
  const [eventsError, setEventsError] = useState("");
  const slideWidth = Math.max(width - 32, 1);

  const loadEvents = useCallback(async () => {
    setLoadingEvents(true);
    setEventsError("");
    try {
      const resp = await listEvents();
      setEvents(resp.items);
    } catch (e) {
      setEventsError(e instanceof Error ? e.message : "活动列表加载失败");
    } finally {
      setLoadingEvents(false);
    }
  }, []);

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

  useEffect(() => {
    void loadEvents();
  }, [loadEvents]);

  const listItems = useMemo(() => {
    if (!events.length) {
      return FALLBACK_ITEMS;
    }
    return events.map((event) => ({
      id: event.event_id,
      title: event.title,
      description: formatEventDescription(event),
    }));
  }, [events]);

  const handleCarouselScrollEnd = (
    event: NativeSyntheticEvent<NativeScrollEvent>,
  ) => {
    const index = Math.round(event.nativeEvent.contentOffset.x / slideWidth);
    const boundedIndex = Math.max(0, Math.min(index, CAROUSEL_ITEMS.length - 1));
    setActiveIndex(boundedIndex);
  };

  return (
    <SafeAreaView style={styles.safeArea} edges={["top", "left", "right"]}>
      <ThemedView style={styles.container}>
        <ThemedView style={styles.topSection}>
          <ThemedText type="subtitle">活动推荐</ThemedText>
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
              style={styles.publishButton}
              onPress={() => router.push("/event/create")}
            >
              <ThemedText style={styles.publishButtonText}>发布</ThemedText>
            </Pressable>
          </View>
          <FlatList
            data={listItems}
            keyExtractor={(item) => item.id}
            style={styles.list}
            showsVerticalScrollIndicator={false}
            contentContainerStyle={styles.listContent}
            refreshing={loadingEvents}
            onRefresh={loadEvents}
            renderItem={({ item }) => (
              <ThemedView
                style={styles.listCard}
                lightColor="#F3F6FA"
                darkColor="#1E252C"
              >
                <ThemedText type="defaultSemiBold">{item.title}</ThemedText>
                <ThemedText>{item.description}</ThemedText>
              </ThemedView>
            )}
          />
        </ThemedView>
      </ThemedView>
    </SafeAreaView>
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
  carousel: {
    flex: 1,
    marginTop: 12,
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
  publishButton: {
    minWidth: 64,
    height: 36,
    borderRadius: 10,
    backgroundColor: "#0A7EA4",
    alignItems: "center",
    justifyContent: "center",
    paddingHorizontal: 14,
  },
  publishButtonText: {
    color: "#FFFFFF",
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
  listCard: {
    borderRadius: 12,
    paddingHorizontal: 14,
    paddingVertical: 12,
    gap: 4,
  },
});
