import { useEffect, useRef, useState } from "react";
import {
  FlatList,
  StyleSheet,
  type NativeScrollEvent,
  type NativeSyntheticEvent,
  useWindowDimensions,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";

import { ThemedText } from "@/components/themed-text";
import { ThemedView } from "@/components/themed-view";

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
    title: "晨间推荐",
    subtitle: "左右滑动查看更多内容",
    color: "#3F8EFC",
  },
  {
    id: "2",
    title: "今日精选",
    subtitle: "自动轮播每 3.5 秒切换",
    color: "#12B886",
  },
  {
    id: "3",
    title: "热点速览",
    subtitle: "支持手势切换卡片",
    color: "#F08C00",
  },
];

const LIST_ITEMS: ListItem[] = Array.from({ length: 24 }, (_, index) => ({
  id: String(index + 1),
  title: `列表项 ${index + 1}`,
  description: "这里是滚动列表的内容描述，可按业务替换。",
}));

export default function HomeScreen() {
  const { width } = useWindowDimensions();
  const carouselRef = useRef<FlatList<CarouselItem>>(null);
  const [activeIndex, setActiveIndex] = useState(0);
  const slideWidth = Math.max(width - 32, 1);

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
          <ThemedText type="subtitle">轮播图</ThemedText>
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
          <ThemedText type="subtitle">滚动列表</ThemedText>
          <FlatList
            data={LIST_ITEMS}
            keyExtractor={(item) => item.id}
            style={styles.list}
            showsVerticalScrollIndicator={false}
            contentContainerStyle={styles.listContent}
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
