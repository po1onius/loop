import { FlatList, StyleSheet } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";

import { ThemedText } from "@/components/themed-text";
import { ThemedView } from "@/components/themed-view";

type CommunitySection = {
  id: string;
  name: string;
  description: string;
  stats: string;
};

const COMMUNITY_SECTIONS: CommunitySection[] = [
  {
    id: "tech",
    name: "科技",
    description: "Web3、AI、开发者交流与线下分享。",
    stats: "1,248 位成员 · 36 篇新帖",
  },
  {
    id: "food",
    name: "美食",
    description: "探店、烘焙、咖啡和城市餐桌活动。",
    stats: "892 位成员 · 18 篇新帖",
  },
  {
    id: "photo",
    name: "摄影",
    description: "约拍、器材经验和主题创作活动。",
    stats: "764 位成员 · 22 篇新帖",
  },
  {
    id: "outdoor",
    name: "户外",
    description: "徒步、骑行、飞盘和周末轻运动。",
    stats: "1,031 位成员 · 27 篇新帖",
  },
];

export default function CommunityScreen() {
  return (
    <SafeAreaView style={styles.safeArea} edges={["top", "left", "right"]}>
      <ThemedView style={styles.container}>
        <ThemedView style={styles.header}>
          <ThemedText type="title">社区</ThemedText>
          <ThemedText style={styles.description}>
            按兴趣主题沉淀讨论内容，并从社区帖子孵化新的线下活动。
          </ThemedText>
        </ThemedView>

        <FlatList
          data={COMMUNITY_SECTIONS}
          keyExtractor={(item) => item.id}
          showsVerticalScrollIndicator={false}
          contentContainerStyle={styles.listContent}
          renderItem={({ item }) => (
            <ThemedView
              style={styles.sectionCard}
              lightColor="#F3F6FA"
              darkColor="#1E252C"
            >
              <ThemedText type="defaultSemiBold">{item.name}</ThemedText>
              <ThemedText>{item.description}</ThemedText>
              <ThemedText style={styles.stats}>{item.stats}</ThemedText>
            </ThemedView>
          )}
        />
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
    paddingHorizontal: 16,
  },
  header: {
    paddingTop: 16,
    paddingBottom: 14,
    gap: 8,
  },
  description: {
    lineHeight: 22,
    opacity: 0.76,
  },
  listContent: {
    paddingBottom: 24,
    gap: 10,
  },
  sectionCard: {
    borderRadius: 12,
    paddingHorizontal: 14,
    paddingVertical: 12,
    gap: 6,
  },
  stats: {
    fontSize: 13,
    lineHeight: 18,
    opacity: 0.68,
  },
});
