import { useFocusEffect } from "@react-navigation/native";
import { router } from "expo-router";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Animated,
  ActivityIndicator,
  Easing,
  FlatList,
  Modal,
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
import { hasAccessToken } from "@/lib/api-client";
import type { EventResp } from "@/lib/dto";
import { listEvents, listMyEvents, listMyJoinedEvents } from "@/lib/event-api";

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

type DrawerItem = {
  id: string;
  title: string;
  icon: "person.text.rectangle" | "list.bullet.rectangle" | "doc.text";
};

type EventLoadMode = "background" | "refresh";
type EventListFilter = "recent" | "published" | "joined";

const EVENT_LIST_FILTERS: { id: EventListFilter; label: string }[] = [
  { id: "recent", label: "近期活动" },
  { id: "published", label: "我发布的" },
  { id: "joined", label: "我参加的" },
];

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

const DRAWER_ITEMS: DrawerItem[] = [
  {
    id: "profile",
    title: "用户信息",
    icon: "person.text.rectangle",
  },
  {
    id: "events",
    title: "我的活动",
    icon: "list.bullet.rectangle",
  },
  {
    id: "posts",
    title: "我的帖子",
    icon: "doc.text",
  },
];

export default function HomeScreen() {
  const { width } = useWindowDimensions();
  const carouselRef = useRef<FlatList<CarouselItem>>(null);
  const drawerTranslateX = useRef(new Animated.Value(-360)).current;
  const latestEventsRequestIdRef = useRef(0);
  const activeRefreshRequestIdRef = useRef<number | null>(null);
  const [activeIndex, setActiveIndex] = useState(0);
  const [drawerVisible, setDrawerVisible] = useState(false);
  const [events, setEvents] = useState<EventResp[]>([]);
  const [eventListFilter, setEventListFilter] =
    useState<EventListFilter>("recent");
  const [loadingEvents, setLoadingEvents] = useState(false);
  const [refreshingEvents, setRefreshingEvents] = useState(false);
  const [eventsError, setEventsError] = useState("");
  const slideWidth = Math.max(width - 32, 1);
  const drawerWidth = Math.min(Math.max(width * 0.78, 260), 320);

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
        filter: eventListFilter,
      });
      const resp = await loadEventList(eventListFilter);
      if (latestEventsRequestIdRef.current !== requestId) {
        console.info("[home] ignored stale events response", {
          mode,
          requestId,
          filter: eventListFilter,
        });
        return;
      }
      setEvents(resp.items);
      console.info("[home] events loaded", {
        requestId,
        filter: eventListFilter,
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
  }, [eventListFilter]);

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

  const handleEventFilterPress = useCallback(
    (nextFilter: EventListFilter) => {
      if (nextFilter === eventListFilter) {
        return;
      }
      if (nextFilter !== "recent" && !hasAccessToken()) {
        console.info("[home] personal event filter requires login", {
          filter: nextFilter,
        });
        router.push("/login");
        return;
      }
      console.info("[home] event filter changed", {
        from: eventListFilter,
        to: nextFilter,
      });
      // 先清空上一筛选的结果，避免请求期间把“近期活动”误显示成个人活动。
      setEvents([]);
      setEventsError("");
      setEventListFilter(nextFilter);
    },
    [eventListFilter],
  );

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
    if (!drawerVisible) {
      return;
    }

    drawerTranslateX.setValue(-drawerWidth);
    Animated.timing(drawerTranslateX, {
      toValue: 0,
      duration: 220,
      easing: Easing.out(Easing.cubic),
      useNativeDriver: true,
    }).start();
  }, [drawerTranslateX, drawerVisible, drawerWidth]);

  const listItems = useMemo(() => {
    if (!events.length && eventListFilter === "recent") {
      return FALLBACK_ITEMS;
    }
    return events.map((event) => ({
      description: formatEventDescription(event),
      eventId: event.event_id,
      id: event.event_id,
      title: event.title,
    }));
  }, [eventListFilter, events]);

  const handleCarouselScrollEnd = (
    event: NativeSyntheticEvent<NativeScrollEvent>,
  ) => {
    const index = Math.round(event.nativeEvent.contentOffset.x / slideWidth);
    const boundedIndex = Math.max(0, Math.min(index, CAROUSEL_ITEMS.length - 1));
    setActiveIndex(boundedIndex);
  };

  const openUserDrawer = useCallback(() => {
    setDrawerVisible(true);
  }, []);

  const closeUserDrawer = useCallback(() => {
    Animated.timing(drawerTranslateX, {
      toValue: -drawerWidth,
      duration: 180,
      easing: Easing.in(Easing.cubic),
      useNativeDriver: true,
    }).start(({ finished }) => {
      if (finished) {
        setDrawerVisible(false);
      }
    });
  }, [drawerTranslateX, drawerWidth]);

  const handleDrawerItemPress = useCallback(
    (item: DrawerItem) => {
      console.info("[home] user drawer item pressed", {
        itemId: item.id,
        title: item.title,
      });
      closeUserDrawer();
    },
    [closeUserDrawer],
  );

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
            <Pressable
              accessibilityRole="button"
              onPress={openUserDrawer}
              style={styles.avatarButton}
            >
              <IconSymbol size={34} name="person.crop.circle" color="#0A7EA4" />
            </Pressable>
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
              <View accessibilityRole="tablist" style={styles.eventFilterBar}>
                {EVENT_LIST_FILTERS.map((filter) => {
                  const selected = filter.id === eventListFilter;
                  return (
                    <Pressable
                      key={filter.id}
                      accessibilityRole="tab"
                      accessibilityState={{ selected }}
                      onPress={() => handleEventFilterPress(filter.id)}
                      style={[
                        styles.eventFilterButton,
                        selected ? styles.eventFilterButtonActive : undefined,
                      ]}
                    >
                      <ThemedText
                        numberOfLines={1}
                        style={[
                          styles.eventFilterText,
                          selected ? styles.eventFilterTextActive : undefined,
                        ]}
                      >
                        {filter.label}
                      </ThemedText>
                    </Pressable>
                  );
                })}
              </View>
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
              <EventListEmptyState
                filter={eventListFilter}
                loading={loadingEvents}
              />
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
        <UserDrawer
          drawerWidth={drawerWidth}
          items={DRAWER_ITEMS}
          translateX={drawerTranslateX}
          visible={drawerVisible}
          onClose={closeUserDrawer}
          onItemPress={handleDrawerItemPress}
        />
      </ThemedView>
    </SafeAreaView>
  );
}

async function loadEventList(filter: EventListFilter) {
  switch (filter) {
    case "published":
      return listMyEvents("published");
    case "joined":
      return listMyJoinedEvents();
    case "recent":
      return listEvents();
  }
}

function EventListEmptyState({
  filter,
  loading,
}: {
  filter: EventListFilter;
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
      <ThemedText type="defaultSemiBold">
        {filter === "published" ? "还没有发布活动" : "还没有参加活动"}
      </ThemedText>
      <ThemedText style={styles.eventEmptyText}>
        {filter === "published"
          ? "点击右侧“发布”创建第一个活动"
          : "正式加入活动后会显示在这里"}
      </ThemedText>
    </View>
  );
}

function UserDrawer({
  drawerWidth,
  items,
  translateX,
  visible,
  onClose,
  onItemPress,
}: {
  drawerWidth: number;
  items: DrawerItem[];
  translateX: Animated.Value;
  visible: boolean;
  onClose: () => void;
  onItemPress: (item: DrawerItem) => void;
}) {
  return (
    <Modal
      animationType="none"
      transparent
      visible={visible}
      onRequestClose={onClose}
    >
      <View style={styles.drawerRoot}>
        <Pressable
          accessibilityRole="button"
          onPress={onClose}
          style={styles.drawerScrim}
        />
        <Animated.View
          style={[
            styles.drawerPanel,
            {
              width: drawerWidth,
              transform: [{ translateX }],
            },
          ]}
        >
          <View style={styles.drawerProfile}>
            <IconSymbol size={48} name="person.crop.circle" color="#0A7EA4" />
            <View style={styles.drawerProfileText}>
              <ThemedText type="defaultSemiBold" style={styles.drawerName}>
                Loop 用户
              </ThemedText>
              <ThemedText style={styles.drawerHint}>个人中心</ThemedText>
            </View>
          </View>

          <View style={styles.drawerList}>
            {items.map((item) => (
              <Pressable
                key={item.id}
                accessibilityRole="button"
                onPress={() => onItemPress(item)}
                style={styles.drawerItem}
              >
                <IconSymbol size={22} name={item.icon} color="#0A7EA4" />
                <ThemedText style={styles.drawerItemText}>{item.title}</ThemedText>
                <IconSymbol size={18} name="chevron.right" color="#8A94A6" />
              </Pressable>
            ))}
          </View>
        </Animated.View>
      </View>
    </Modal>
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
  avatarButton: {
    width: 44,
    height: 44,
    borderRadius: 22,
    alignItems: "center",
    justifyContent: "center",
    backgroundColor: "#EAF6FA",
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
  eventFilterBar: {
    flexDirection: "row",
    alignItems: "center",
    gap: 4,
  },
  eventFilterButton: {
    minHeight: 34,
    justifyContent: "center",
    borderRadius: 9,
    paddingHorizontal: 8,
  },
  eventFilterButtonActive: {
    backgroundColor: "#EAF6FA",
  },
  eventFilterText: {
    color: "#687076",
    fontSize: 13,
    lineHeight: 18,
    fontWeight: "600",
  },
  eventFilterTextActive: {
    color: "#0A7EA4",
    fontWeight: "800",
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
  drawerRoot: {
    flex: 1,
    flexDirection: "row",
  },
  drawerScrim: {
    ...StyleSheet.absoluteFillObject,
    backgroundColor: "rgba(17, 24, 28, 0.42)",
  },
  drawerPanel: {
    height: "100%",
    paddingHorizontal: 16,
    paddingTop: 54,
    backgroundColor: "#FFFFFF",
    shadowColor: "#000000",
    shadowOpacity: 0.18,
    shadowRadius: 16,
    shadowOffset: { width: 4, height: 0 },
    elevation: 12,
  },
  drawerProfile: {
    minHeight: 72,
    flexDirection: "row",
    alignItems: "center",
    gap: 12,
    borderBottomWidth: 1,
    borderBottomColor: "#E2E8F0",
    paddingBottom: 16,
  },
  drawerProfileText: {
    flex: 1,
  },
  drawerName: {
    color: "#11181C",
  },
  drawerHint: {
    marginTop: 2,
    fontSize: 13,
    lineHeight: 18,
    color: "#687076",
  },
  drawerList: {
    paddingTop: 12,
    gap: 4,
  },
  drawerItem: {
    minHeight: 50,
    flexDirection: "row",
    alignItems: "center",
    gap: 12,
    borderRadius: 10,
    paddingHorizontal: 10,
  },
  drawerItemText: {
    flex: 1,
    color: "#11181C",
    fontWeight: "600",
  },
});
