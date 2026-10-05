import { useState } from "react";
import { Pressable, ScrollView, View, StyleSheet } from "react-native";
import { router } from "expo-router";
import { ThemedText } from "@/components/themed-text";
import { listConversationMembers, setConversationSubscription } from "@/lib/conversation-api";
import type { ConversationMemberResp, ConversationResp } from "@/lib/dto";

export function GroupContext({ conversation, onChange }: { conversation: ConversationResp; onChange: (conversation: ConversationResp) => void }) {
  const [members, setMembers] = useState<ConversationMemberResp[] | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const toggleMembers = async () => {
    setExpanded(!expanded);
    if (expanded) return;
    try { setMembers(await listConversationMembers(conversation.conversation_id)); }
    catch { setError("群成员加载失败，请重试"); }
  };
  const toggleMute = async () => {
    if (busy) return;
    setBusy(true); setError("");
    try {
      await setConversationSubscription({ conversationId: conversation.conversation_id, subscribed: true, muted: !conversation.muted });
      onChange({ ...conversation, muted: !conversation.muted });
    } catch { setError("免打扰设置失败，请重试"); }
    finally { setBusy(false); }
  };
  return <View style={styles.container}>
    <View style={styles.actions}>
      <Pressable onPress={() => router.push(`/event/${encodeURIComponent(conversation.subject_id)}` as never)}><ThemedText style={styles.link}>查看活动</ThemedText></Pressable>
      <Pressable onPress={() => void toggleMembers()}><ThemedText style={styles.link}>群成员{members ? ` (${members.length})` : ""}</ThemedText></Pressable>
      <Pressable disabled={busy} onPress={() => void toggleMute()}><ThemedText style={styles.link}>{conversation.muted ? "已免打扰" : "开启免打扰"}</ThemedText></Pressable>
    </View>
    {error ? <ThemedText>{error}</ThemedText> : null}
    {expanded ? <ScrollView style={styles.members}>{members?.map((member) => <ThemedText key={member.user_id}>{member.username}{member.is_owner ? " · 群主" : ""}</ThemedText>) ?? <ThemedText>正在加载...</ThemedText>}</ScrollView> : null}
  </View>;
}
const styles = StyleSheet.create({ container: { padding: 12, gap: 8 }, actions: { flexDirection: "row", justifyContent: "space-between" }, link: { color: "#0A7EA4", fontSize: 13 }, members: { maxHeight: 160 } });
