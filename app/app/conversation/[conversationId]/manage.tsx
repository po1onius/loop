import { router, useLocalSearchParams } from "expo-router";
import { useEffect, useRef, useState } from "react";
import { ActivityIndicator, FlatList, Pressable, StyleSheet, Switch, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";

import { ThemedText } from "@/components/themed-text";
import { ThemedView } from "@/components/themed-view";
import { IconSymbol } from "@/components/ui/icon-symbol";
import { UserAvatar } from "@/components/user-avatar";
import { useThemeColor } from "@/hooks/use-theme-color";
import { getConversation, listConversationMembers, setConversationSubscription } from "@/lib/conversation-api";
import type { ConversationMemberResp, ConversationResp } from "@/lib/dto";

export default function GroupManagementScreen() {
  const params = useLocalSearchParams<{ conversationId?: string | string[] }>();
  const conversationId = Array.isArray(params.conversationId) ? params.conversationId[0] : params.conversationId;
  const textColor = useThemeColor({}, "text");
  const borderColor = useThemeColor({ light: "#E8EBEE", dark: "#2D353C" }, "background");
  const [conversation, setConversation] = useState<ConversationResp | null>(null);
  const [members, setMembers] = useState<ConversationMemberResp[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState("");
  const [muteError, setMuteError] = useState("");
  const [savingMute, setSavingMute] = useState(false);
  const muteRequestRef = useRef(false);
  const [loadAttempt, setLoadAttempt] = useState(0);

  useEffect(() => {
    if (!conversationId) {
      setLoadError("会话地址无效");
      setLoading(false);
      return;
    }
    let active = true;
    setLoading(true);
    setLoadError("");
    console.info("[group-management] loading group", { conversationId });
    void Promise.all([getConversation(conversationId), listConversationMembers(conversationId)])
      .then(([nextConversation, nextMembers]) => {
        if (!active) return;
        if (nextConversation.kind !== "event_group") {
          throw new Error("该会话不是活动群聊");
        }
        setConversation(nextConversation);
        setMembers(nextMembers);
        console.info("[group-management] group loaded", {
          conversationId,
          memberCount: nextMembers.length,
          muted: nextConversation.muted,
        });
      })
      .catch((error) => {
        if (!active) return;
        const reason = error instanceof Error ? error.message : "群聊信息加载失败，请重试";
        console.warn("[group-management] group load failed", { conversationId, reason });
        setLoadError(reason);
      })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [conversationId, loadAttempt]);

  async function changeMute(muted: boolean) {
    if (!conversation || muteRequestRef.current) return;
    muteRequestRef.current = true;
    setSavingMute(true);
    setMuteError("");
    const id = conversation.conversation_id;
    console.info("[group-management] updating mute setting", { conversationId: id, muted });
    try {
      await setConversationSubscription({ conversationId: id, subscribed: conversation.subscribed, muted });
      setConversation((current) => current?.conversation_id === id ? { ...current, muted } : current);
      console.info("[group-management] mute setting updated", { conversationId: id, muted });
    } catch (error) {
      const reason = error instanceof Error ? error.message : "免打扰设置失败，请重试";
      console.warn("[group-management] mute setting update failed", { conversationId: id, muted, reason });
      setMuteError(reason);
    } finally {
      muteRequestRef.current = false;
      setSavingMute(false);
    }
  }

  return (
    <SafeAreaView style={styles.flex} edges={["top", "bottom", "left", "right"]}>
      <ThemedView style={styles.flex}>
        <View style={[styles.header, { borderBottomColor: borderColor }]}>
          <Pressable accessibilityRole="button" accessibilityLabel="返回群聊" onPress={() => router.back()} style={styles.iconButton}>
            <IconSymbol name="chevron.left" size={26} color={textColor} />
          </Pressable>
          <ThemedText type="defaultSemiBold" style={styles.title}>群聊管理</ThemedText>
          <View style={styles.iconButton} />
        </View>

        {loading ? (
          <View style={styles.centerState}>
            <ActivityIndicator color="#0A7EA4" />
            <ThemedText style={styles.secondary}>正在加载群聊信息...</ThemedText>
          </View>
        ) : loadError ? (
          <View style={styles.centerState}>
            <ThemedText style={styles.error} accessibilityLiveRegion="polite">{loadError}</ThemedText>
            <Pressable accessibilityRole="button" onPress={() => setLoadAttempt((attempt) => attempt + 1)} style={styles.retryButton}>
              <ThemedText type="link">重试</ThemedText>
            </Pressable>
          </View>
        ) : conversation ? (
          <FlatList
            data={members}
            keyExtractor={(member) => member.user_id}
            contentContainerStyle={styles.listContent}
            ListHeaderComponent={
              <View>
                <View style={[styles.settingRow, { borderBottomColor: borderColor }]}>
                  <ThemedText type="defaultSemiBold" style={styles.settingLabel}>免打扰</ThemedText>
                  {savingMute ? <ActivityIndicator size="small" color="#0A7EA4" /> : null}
                  <Switch
                    accessibilityLabel="免打扰"
                    accessibilityState={{ busy: savingMute }}
                    value={conversation.muted}
                    disabled={savingMute}
                    onValueChange={(muted) => void changeMute(muted)}
                    trackColor={{ true: "#0A7EA4" }}
                  />
                </View>
                {muteError ? <ThemedText style={styles.error} accessibilityLiveRegion="polite">{muteError}</ThemedText> : null}
                <ThemedText type="defaultSemiBold" style={styles.membersTitle}>群成员（{members.length}）</ThemedText>
              </View>
            }
            ListEmptyComponent={<ThemedText style={styles.secondary}>暂无群成员</ThemedText>}
            renderItem={({ item }) => (
              <View style={[styles.memberRow, { borderBottomColor: borderColor }]}>
                <UserAvatar username={item.username} avatarAssetId={item.avatar_asset_id} size={42} />
                <ThemedText style={styles.memberName} numberOfLines={1}>{item.username}</ThemedText>
                {item.is_owner ? <ThemedText style={styles.ownerBadge}>群主</ThemedText> : null}
              </View>
            )}
          />
        ) : null}
      </ThemedView>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  flex: { flex: 1 },
  header: { height: 58, paddingHorizontal: 10, flexDirection: "row", alignItems: "center", borderBottomWidth: StyleSheet.hairlineWidth },
  iconButton: { width: 42, height: 42, justifyContent: "center" },
  title: { flex: 1, textAlign: "center" },
  centerState: { flex: 1, alignItems: "center", justifyContent: "center", gap: 12, padding: 24 },
  secondary: { opacity: 0.6, textAlign: "center" },
  error: { color: "#C23C3C", paddingVertical: 8, textAlign: "center" },
  retryButton: { paddingHorizontal: 20, paddingVertical: 8 },
  listContent: { paddingHorizontal: 20, paddingBottom: 24 },
  settingRow: { minHeight: 76, flexDirection: "row", alignItems: "center", gap: 12, borderBottomWidth: StyleSheet.hairlineWidth },
  settingLabel: { flex: 1 },
  membersTitle: { marginTop: 24, marginBottom: 8 },
  memberRow: { minHeight: 70, flexDirection: "row", alignItems: "center", gap: 12, paddingVertical: 12, borderBottomWidth: StyleSheet.hairlineWidth },
  memberName: { flex: 1 },
  ownerBadge: { color: "#0A7EA4", fontSize: 12, lineHeight: 20, paddingHorizontal: 8, borderRadius: 6, backgroundColor: "#EAF6FA" },
});
