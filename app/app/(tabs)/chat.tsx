import { FlatList, StyleSheet, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";

import { ThemedText } from "@/components/themed-text";
import { ThemedView } from "@/components/themed-view";

type ChatThread = {
  id: string;
  name: string;
  message: string;
  time: string;
  unreadCount: number;
};

const CHAT_THREADS: ChatThread[] = [
  {
    id: "web3",
    name: "Web3 技术交流会",
    message: "组织者：签到二维码将在活动前 2 小时开放。",
    time: "12:42",
    unreadCount: 3,
  },
  {
    id: "photo",
    name: "城市摄影外拍",
    message: "阿林：这次建议带 35mm 或 50mm 定焦。",
    time: "昨天",
    unreadCount: 0,
  },
  {
    id: "outdoor",
    name: "户外飞盘新手局",
    message: "系统：你已成功报名本周六活动。",
    time: "周三",
    unreadCount: 1,
  },
];

export default function ChatScreen() {
  return (
    <SafeAreaView style={styles.safeArea} edges={["top", "left", "right"]}>
      <ThemedView style={styles.container}>
        <ThemedView style={styles.header}>
          <ThemedText type="title">聊天</ThemedText>
          <ThemedText style={styles.description}>
            管理已报名活动的群聊消息，及时接收组织者通知。
          </ThemedText>
        </ThemedView>

        <FlatList
          data={CHAT_THREADS}
          keyExtractor={(item) => item.id}
          showsVerticalScrollIndicator={false}
          contentContainerStyle={styles.listContent}
          renderItem={({ item }) => (
            <ThemedView
              style={styles.threadCard}
              lightColor="#F3F6FA"
              darkColor="#1E252C"
            >
              <View style={styles.threadHeader}>
                <ThemedText type="defaultSemiBold" style={styles.threadName}>
                  {item.name}
                </ThemedText>
                <ThemedText style={styles.time}>{item.time}</ThemedText>
              </View>
              <View style={styles.threadBody}>
                <ThemedText style={styles.message} numberOfLines={1}>
                  {item.message}
                </ThemedText>
                {item.unreadCount > 0 ? (
                  <View style={styles.badge}>
                    <ThemedText style={styles.badgeText}>
                      {item.unreadCount}
                    </ThemedText>
                  </View>
                ) : null}
              </View>
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
  threadCard: {
    borderRadius: 12,
    paddingHorizontal: 14,
    paddingVertical: 12,
    gap: 8,
  },
  threadHeader: {
    flexDirection: "row",
    alignItems: "center",
    gap: 10,
  },
  threadName: {
    flex: 1,
  },
  time: {
    fontSize: 12,
    lineHeight: 16,
    opacity: 0.62,
  },
  threadBody: {
    flexDirection: "row",
    alignItems: "center",
    gap: 10,
  },
  message: {
    flex: 1,
    lineHeight: 20,
    opacity: 0.72,
  },
  badge: {
    minWidth: 22,
    height: 22,
    borderRadius: 11,
    alignItems: "center",
    justifyContent: "center",
    backgroundColor: "#0A7EA4",
    paddingHorizontal: 6,
  },
  badgeText: {
    color: "#FFFFFF",
    fontSize: 12,
    lineHeight: 16,
    fontWeight: "700",
  },
});
