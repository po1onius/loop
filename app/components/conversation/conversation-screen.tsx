import { FlashList, type FlashListRef } from "@shopify/flash-list";
import { Image } from "expo-image";
import * as ImagePicker from "expo-image-picker";
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import {
  ActivityIndicator,
  KeyboardAvoidingView,
  Platform,
  Pressable,
  StyleSheet,
  TextInput,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";

import { ThemedText } from "@/components/themed-text";
import { ThemedView } from "@/components/themed-view";
import { IconSymbol } from "@/components/ui/icon-symbol";
import { UserAvatar } from "@/components/user-avatar";
import { useColorScheme } from "@/hooks/use-color-scheme";
import {
  listConversationMessages,
  markConversationRead,
  sendConversationMessage,
  setConversationSubscription,
} from "@/lib/conversation-api";
import type { ConversationMessageResp, ConversationResp } from "@/lib/dto";
import { getMediaDownloadUrl, uploadLocalImageAsset } from "@/lib/media-api";
import { subscribeConversationRealtime } from "@/lib/realtime-client";

const MESSAGE_POLL_INTERVAL_MS = 4_000;
const MESSAGE_RECONCILE_INTERVAL_MS = 30_000;

type PendingImage = { localUri: string; assetId: string };

/**
 * 通用会话视图只依赖 Conversation/Message 协议，不理解帖子或活动业务。
 * headerContext 由路由层注入，因此后续活动群聊、私聊都能复用消息区和输入区。
 */
export function ConversationScreen({
  conversation,
  currentUserId,
  headerContext,
  onBack,
  onConversationChange,
}: {
  conversation: ConversationResp;
  currentUserId: string;
  headerContext?: ReactNode;
  onBack: () => void;
  onConversationChange?: (conversation: ConversationResp) => void;
}) {
  const isDark = useColorScheme() === "dark";
  const listRef = useRef<FlashListRef<ConversationMessageResp>>(null);
  const mountedRef = useRef(true);
  const messagesRef = useRef<ConversationMessageResp[]>([]);
  const initialScrollDoneRef = useRef(false);
  const [messages, setMessages] = useState<ConversationMessageResp[]>([]);
  const [nextBeforeSeq, setNextBeforeSeq] = useState<bigint | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadingOlder, setLoadingOlder] = useState(false);
  const [sending, setSending] = useState(false);
  const [uploadingImages, setUploadingImages] = useState(false);
  const [subscriptionBusy, setSubscriptionBusy] = useState(false);
  const [subscribed, setSubscribed] = useState(conversation.subscribed);
  const [draft, setDraft] = useState("");
  const [pendingImages, setPendingImages] = useState<PendingImage[]>([]);
  const [quotedMessage, setQuotedMessage] = useState<ConversationMessageResp | null>(null);
  const [error, setError] = useState("");

  const markLatestRead = useCallback((items: ConversationMessageResp[]) => {
    const last = items[items.length - 1];
    if (!last) return;
    void markConversationRead(conversation.conversation_id, last.seq).catch((markError) => {
      console.warn("[conversation-screen] read cursor update failed", {
        conversationId: conversation.conversation_id,
        seq: last.seq.toString(),
        reason: markError instanceof Error ? markError.message : String(markError),
      });
    });
  }, [conversation.conversation_id]);

  const loadRecent = useCallback(async (mode: "initial" | "poll") => {
    try {
      if (mode === "poll") {
        let afterSeq = messagesRef.current[messagesRef.current.length - 1]?.seq ?? 0n;
        const incoming: ConversationMessageResp[] = [];
        // 每页最多 100 条，沿 next_after_seq 连续追赶到最新位置，避免长时间
        // 离线后只拿到最后一页而在本地时间线中留下不可见缺口。
        for (;;) {
          const page = await listConversationMessages({
            conversationId: conversation.conversation_id,
            afterSeq,
            limit: 100,
          });
          incoming.push(...page.items);
          if (page.next_after_seq === null) break;
          afterSeq = page.next_after_seq;
        }
        if (!mountedRef.current || !incoming.length) return;
        const merged = mergeMessages(messagesRef.current, incoming);
        messagesRef.current = merged;
        setMessages(merged);
        markLatestRead(incoming);
        return;
      }

      const resp = await listConversationMessages({
        conversationId: conversation.conversation_id,
        limit: 50,
      });
      if (!mountedRef.current) return;
      messagesRef.current = resp.items;
      setMessages(resp.items);
      setNextBeforeSeq(resp.next_before_seq);
      markLatestRead(resp.items);
      console.info("[conversation-screen] initial messages loaded", {
        conversationId: conversation.conversation_id,
        count: resp.items.length,
        hasOlder: resp.next_before_seq !== null,
      });
    } catch (loadError) {
      if (!mountedRef.current) return;
      const message = loadError instanceof Error ? loadError.message : "讨论消息加载失败";
      if (mode === "initial") setError(message);
      console.warn("[conversation-screen] messages load failed", {
        conversationId: conversation.conversation_id,
        mode,
        reason: message,
      });
    } finally {
      if (mode === "initial" && mountedRef.current) setLoading(false);
    }
  }, [conversation.conversation_id, markLatestRead]);

  useEffect(() => {
    mountedRef.current = true;
    let active = true;
    let reconcileTimer: ReturnType<typeof setInterval> | null = null;
    let closeRealtime: (() => void) | null = null;
    // WebSocket 只传递轻量通知，收到后通过 HTTP 按服务端序号合并正文；固定周期
    // 对账负责补齐断线或 Redis Pub/Sub 未投递的窗口。未配置实时地址时退回短轮询。
    // 必须先建立本地初始序号再订阅，否则首屏请求与实时回补并发完成时，较旧的
    // 首屏响应可能覆盖刚合并的新消息。
    void loadRecent("initial").then(() => {
      if (!active) return;
      const realtime = subscribeConversationRealtime(
        conversation.conversation_id,
        () => void loadRecent("poll"),
      );
      closeRealtime = realtime.close;
      reconcileTimer = setInterval(
        () => void loadRecent("poll"),
        realtime.configured ? MESSAGE_RECONCILE_INTERVAL_MS : MESSAGE_POLL_INTERVAL_MS,
      );
    });
    return () => {
      active = false;
      mountedRef.current = false;
      if (reconcileTimer) clearInterval(reconcileTimer);
      closeRealtime?.();
    };
  }, [conversation.conversation_id, loadRecent]);

  const loadOlder = useCallback(async () => {
    if (loadingOlder || nextBeforeSeq === null) return;
    setLoadingOlder(true);
    try {
      const resp = await listConversationMessages({
        conversationId: conversation.conversation_id,
        beforeSeq: nextBeforeSeq,
        limit: 50,
      });
      const merged = mergeMessages(resp.items, messagesRef.current);
      messagesRef.current = merged;
      setMessages(merged);
      setNextBeforeSeq(resp.next_before_seq);
      console.info("[conversation-screen] older messages loaded", {
        conversationId: conversation.conversation_id,
        count: resp.items.length,
        hasOlder: resp.next_before_seq !== null,
      });
    } catch (loadError) {
      const message = loadError instanceof Error ? loadError.message : "更早消息加载失败";
      console.warn("[conversation-screen] older messages load failed", { reason: message });
      setError(message);
    } finally {
      setLoadingOlder(false);
    }
  }, [conversation.conversation_id, loadingOlder, nextBeforeSeq]);

  const send = useCallback(async () => {
    const body = draft.trim();
    if ((!body && !pendingImages.length) || sending || !conversation.capabilities.can_send) return;
    setSending(true);
    setError("");
    try {
      const sent = await sendConversationMessage({
        conversationId: conversation.conversation_id,
        body,
        imageAssetIds: pendingImages.map((image) => image.assetId),
        quoteMessageId: quotedMessage?.message_id ?? null,
      });
      const merged = mergeMessages(messagesRef.current, [sent]);
      messagesRef.current = merged;
      setMessages(merged);
      setDraft("");
      setPendingImages([]);
      setQuotedMessage(null);
      setSubscribed(true);
      requestAnimationFrame(() => listRef.current?.scrollToEnd({ animated: true }));
      console.info("[conversation-screen] message sent", {
        conversationId: conversation.conversation_id,
        messageId: sent.message_id,
        seq: sent.seq.toString(),
      });
    } catch (sendError) {
      const message = sendError instanceof Error ? sendError.message : "消息发送失败";
      console.warn("[conversation-screen] message send failed", { reason: message });
      setError(message);
    } finally {
      setSending(false);
    }
  }, [conversation.capabilities.can_send, conversation.conversation_id, draft, pendingImages, quotedMessage, sending]);

  const pickImages = useCallback(async () => {
    if (uploadingImages || pendingImages.length >= 4 || !conversation.capabilities.can_upload) return;
    setUploadingImages(true);
    setError("");
    try {
      const permission = await ImagePicker.requestMediaLibraryPermissionsAsync();
      if (!permission.granted) throw new Error("需要相册权限才能发送图片");
      const result = await ImagePicker.launchImageLibraryAsync({
        mediaTypes: ["images"],
        allowsMultipleSelection: true,
        selectionLimit: 4 - pendingImages.length,
        quality: 0.9,
      });
      if (result.canceled) return;
      const assets = result.assets.slice(0, 4 - pendingImages.length);
      const uploaded = await Promise.all(assets.map(async (asset) => {
        const media = await uploadLocalImageAsset({
          uri: asset.uri,
          mimeType: asset.mimeType ?? null,
          fileName: asset.fileName ?? null,
          width: asset.width,
          height: asset.height,
          file: asset.file ?? null,
        });
        return { localUri: asset.uri, assetId: media.asset_id };
      }));
      setPendingImages((current) => [...current, ...uploaded].slice(0, 4));
      console.info("[conversation-screen] message images uploaded", {
        conversationId: conversation.conversation_id,
        count: uploaded.length,
      });
    } catch (uploadError) {
      const message = uploadError instanceof Error ? uploadError.message : "图片上传失败";
      console.warn("[conversation-screen] message image upload failed", { reason: message });
      setError(message);
    } finally {
      setUploadingImages(false);
    }
  }, [conversation.capabilities.can_upload, conversation.conversation_id, pendingImages.length, uploadingImages]);

  const toggleSubscription = useCallback(async () => {
    if (subscriptionBusy) return;
    const desired = !subscribed;
    setSubscriptionBusy(true);
    try {
      await setConversationSubscription({ conversationId: conversation.conversation_id, subscribed: desired });
      setSubscribed(desired);
      onConversationChange?.({ ...conversation, subscribed: desired });
      console.info("[conversation-screen] subscription changed", {
        conversationId: conversation.conversation_id,
        subscribed: desired,
      });
    } catch (subscriptionError) {
      const message = subscriptionError instanceof Error ? subscriptionError.message : "订阅设置失败";
      console.warn("[conversation-screen] subscription change failed", { reason: message });
      setError(message);
    } finally {
      setSubscriptionBusy(false);
    }
  }, [conversation, onConversationChange, subscribed, subscriptionBusy]);

  const inputColor = isDark ? "#ECEDEE" : "#11181C";
  return (
    <SafeAreaView style={styles.safeArea} edges={["top", "bottom", "left", "right"]}>
      <KeyboardAvoidingView style={styles.flex} behavior={Platform.OS === "ios" ? "padding" : undefined}>
        <ThemedView style={styles.flex}>
          <View style={[styles.header, { borderBottomColor: isDark ? "#2D353C" : "#E8EBEE" }]}>
            <Pressable accessibilityRole="button" accessibilityLabel="返回" onPress={onBack} style={styles.backButton}>
              <IconSymbol name="chevron.left" size={26} color={inputColor} />
            </Pressable>
            <View style={styles.headerTitle}>
              <ThemedText type="defaultSemiBold" numberOfLines={1}>{conversation.title}</ThemedText>
              <ThemedText style={styles.headerMeta}>{conversation.message_count.toString()} 条消息</ThemedText>
            </View>
            <Pressable accessibilityRole="button" disabled={subscriptionBusy} onPress={() => void toggleSubscription()} style={styles.subscribeButton}>
              {subscriptionBusy ? <ActivityIndicator size="small" color="#0A7EA4" /> : <ThemedText style={styles.subscribeText}>{subscribed ? "已加入" : "加入"}</ThemedText>}
            </Pressable>
          </View>

          {headerContext}
          {error ? <Pressable onPress={() => { setError(""); void loadRecent("initial"); }} style={styles.errorBar}><ThemedText style={styles.errorText}>{error}，点击重试</ThemedText></Pressable> : null}

          {loading ? (
            <View style={styles.centerState}><ActivityIndicator color="#0A7EA4" /><ThemedText style={styles.muted}>正在加载讨论...</ThemedText></View>
          ) : (
            <FlashList
              ref={listRef}
              data={messages}
              keyExtractor={(item) => item.message_id}
              renderItem={({ item, index }) => (
                <MessageBubble
                  message={item}
                  own={item.sender_id === currentUserId}
                  showSender={index === 0 || messages[index - 1]?.sender_id !== item.sender_id}
                  quoted={item.quote_message_id ? messages.find((candidate) => candidate.message_id === item.quote_message_id) ?? null : null}
                  onQuote={() => conversation.capabilities.can_quote && setQuotedMessage(item)}
                />
              )}
              contentContainerStyle={styles.messageList}
              ListHeaderComponent={nextBeforeSeq !== null ? <Pressable disabled={loadingOlder} onPress={() => void loadOlder()} style={styles.olderButton}>{loadingOlder ? <ActivityIndicator size="small" color="#0A7EA4" /> : <ThemedText style={styles.olderText}>加载更早消息</ThemedText>}</Pressable> : null}
              ListEmptyComponent={<View style={styles.emptyState}><ThemedText type="defaultSemiBold">还没有人发言</ThemedText><ThemedText style={styles.muted}>发出第一条消息，开始这个讨论线程</ThemedText></View>}
              onContentSizeChange={() => {
                if (!initialScrollDoneRef.current && messages.length) {
                  initialScrollDoneRef.current = true;
                  listRef.current?.scrollToEnd({ animated: false });
                }
              }}
            />
          )}

          {pendingImages.length ? <View style={styles.pendingImages}>{pendingImages.map((image) => <View key={image.assetId} style={styles.pendingImageCell}><Image source={image.localUri} contentFit="cover" style={styles.pendingImage} /><Pressable onPress={() => setPendingImages((current) => current.filter((candidate) => candidate.assetId !== image.assetId))} style={styles.pendingImageRemove}><ThemedText style={styles.pendingImageRemoveText}>×</ThemedText></Pressable></View>)}</View> : null}
          {quotedMessage ? <View style={[styles.quoteComposer, { borderTopColor: isDark ? "#303941" : "#E5E9ED" }]}><View style={styles.quoteComposerText}><ThemedText style={styles.quoteName}>回复 {quotedMessage.sender_username}</ThemedText><ThemedText numberOfLines={1} style={styles.quoteBody}>{quotedMessage.body || "[图片]"}</ThemedText></View><Pressable onPress={() => setQuotedMessage(null)} hitSlop={10}><ThemedText style={styles.closeQuote}>×</ThemedText></Pressable></View> : null}
          <View style={[styles.composer, { borderTopColor: isDark ? "#303941" : "#E5E9ED" }]}>
            <Pressable accessibilityRole="button" accessibilityLabel="添加图片" disabled={uploadingImages || pendingImages.length >= 4 || !conversation.capabilities.can_upload} onPress={() => void pickImages()} style={styles.imageButton}>
              {uploadingImages ? <ActivityIndicator size="small" color="#0A7EA4" /> : <IconSymbol name="photo" size={23} color="#0A7EA4" />}
            </Pressable>
            <TextInput
              value={draft}
              onChangeText={setDraft}
              editable={conversation.capabilities.can_send && !sending}
              multiline
              maxLength={2_000}
              placeholder={conversation.capabilities.can_send ? "发送消息…" : conversation.capabilities.read_only_reason ?? "当前会话只读"}
              placeholderTextColor="#8A94A6"
              style={[styles.messageInput, { color: inputColor, backgroundColor: isDark ? "#242C33" : "#F1F4F6" }]}
            />
            <Pressable accessibilityRole="button" accessibilityLabel="发送消息" disabled={(!draft.trim() && !pendingImages.length) || sending || !conversation.capabilities.can_send} onPress={() => void send()} style={[styles.sendButton, ((!draft.trim() && !pendingImages.length) || sending || !conversation.capabilities.can_send) ? styles.disabled : undefined]}>
              {sending ? <ActivityIndicator size="small" color="#FFFFFF" /> : <IconSymbol name="paperplane.fill" size={19} color="#FFFFFF" />}
            </Pressable>
          </View>
        </ThemedView>
      </KeyboardAvoidingView>
    </SafeAreaView>
  );
}

function MessageBubble({ message, own, showSender, quoted, onQuote }: { message: ConversationMessageResp; own: boolean; showSender: boolean; quoted: ConversationMessageResp | null; onQuote: () => void }) {
  return (
    <View style={[styles.messageRow, own ? styles.ownRow : styles.otherRow]}>
      {!own && showSender ? <UserAvatar username={message.sender_username} avatarAssetId={message.sender_avatar_asset_id} size={30} style={styles.messageAvatar} /> : !own ? <View style={styles.avatarSpacer} /> : null}
      <View style={styles.bubbleColumn}>
        {!own && showSender ? <ThemedText style={styles.senderName}>{message.sender_username}</ThemedText> : null}
        <Pressable onLongPress={onQuote} delayLongPress={300} style={[styles.bubble, own ? styles.ownBubble : styles.otherBubble]}>
          {quoted ? <View style={styles.quotedBlock}><ThemedText lightColor="#08789C" darkColor="#08789C" numberOfLines={1} style={styles.quotedName}>{quoted.sender_username}</ThemedText><ThemedText lightColor="#42515B" darkColor="#42515B" numberOfLines={1} style={styles.quotedBody}>{quoted.body || "[图片]"}</ThemedText></View> : null}
          {message.deleted_at ? <ThemedText lightColor="#5D6970" darkColor="#5D6970" style={styles.deletedText}>消息已删除</ThemedText> : <ThemedText lightColor={own ? "#18371A" : "#24313A"} darkColor={own ? "#18371A" : "#24313A"}>{message.body}</ThemedText>}
          {!message.deleted_at && message.image_asset_ids.length ? <MessageImageGrid assetIds={message.image_asset_ids} /> : null}
          <ThemedText lightColor={own ? "#355F38" : "#52616A"} darkColor={own ? "#355F38" : "#52616A"} style={styles.messageTime}>{formatMessageTime(message.created_at)}</ThemedText>
        </Pressable>
      </View>
    </View>
  );
}

function MessageImageGrid({ assetIds }: { assetIds: string[] }) {
  const [uris, setUris] = useState<(string | null)[]>([]);
  useEffect(() => {
    let active = true;
    void Promise.all(assetIds.map(async (assetId) => {
      try {
        return (await getMediaDownloadUrl(assetId)).download_url;
      } catch (error) {
        console.warn("[conversation-screen] message image URL load failed", {
          assetId,
          reason: error instanceof Error ? error.message : String(error),
        });
        return null;
      }
    })).then((nextUris) => active && setUris(nextUris));
    return () => { active = false; };
  }, [assetIds]);
  return <View style={styles.messageImages}>{uris.map((uri, index) => uri ? <Image key={assetIds[index]} source={uri} contentFit="cover" style={styles.messageImage} /> : null)}</View>;
}

function mergeMessages(first: ConversationMessageResp[], second: ConversationMessageResp[]): ConversationMessageResp[] {
  const byId = new Map<string, ConversationMessageResp>();
  for (const message of [...first, ...second]) byId.set(message.message_id, message);
  return [...byId.values()].sort((left, right) => left.seq < right.seq ? -1 : left.seq > right.seq ? 1 : 0);
}

function formatMessageTime(value: string): string {
  const date = new Date(value);
  if (!Number.isFinite(date.getTime())) return "";
  return new Intl.DateTimeFormat("zh-CN", { hour: "2-digit", minute: "2-digit", hour12: false }).format(date);
}

const styles = StyleSheet.create({
  safeArea: { flex: 1 }, flex: { flex: 1 },
  header: { height: 58, paddingHorizontal: 10, flexDirection: "row", alignItems: "center", borderBottomWidth: StyleSheet.hairlineWidth },
  backButton: { width: 42, height: 42, justifyContent: "center" }, headerTitle: { flex: 1, alignItems: "center" },
  headerMeta: { opacity: 0.5, fontSize: 11, lineHeight: 14 }, subscribeButton: { minWidth: 52, height: 34, alignItems: "center", justifyContent: "center" },
  subscribeText: { color: "#0A7EA4", fontSize: 14, fontWeight: "700" },
  messageList: { paddingHorizontal: 12, paddingTop: 12, paddingBottom: 14 },
  messageRow: { flexDirection: "row", marginBottom: 4, alignItems: "flex-start" }, ownRow: { justifyContent: "flex-end" }, otherRow: { justifyContent: "flex-start" },
  messageAvatar: { marginRight: 7 }, avatarSpacer: { width: 37 }, bubbleColumn: { maxWidth: "78%" },
  senderName: { color: "#0A7EA4", fontSize: 11, lineHeight: 15, marginLeft: 4, marginBottom: 2 },
  bubble: { borderRadius: 15, paddingHorizontal: 12, paddingVertical: 8, minWidth: 74 }, ownBubble: { backgroundColor: "#DDF3DE", borderBottomRightRadius: 4 }, otherBubble: { backgroundColor: "#EDF1F4", borderBottomLeftRadius: 4 },
  messageTime: { alignSelf: "flex-end", fontSize: 10, lineHeight: 13, opacity: 0.48, marginTop: 2 },
  quotedBlock: { borderLeftWidth: 3, borderLeftColor: "#0A7EA4", paddingLeft: 7, marginBottom: 5 }, quotedName: { color: "#08789C", fontSize: 11, lineHeight: 14, fontWeight: "700" }, quotedBody: { color: "#42515B", fontSize: 11, lineHeight: 14 }, deletedText: { fontStyle: "italic", opacity: 0.55 },
  olderButton: { alignSelf: "center", height: 34, justifyContent: "center", marginBottom: 12 }, olderText: { color: "#0A7EA4", fontSize: 13 },
  centerState: { flex: 1, alignItems: "center", justifyContent: "center", gap: 10 }, emptyState: { padding: 40, alignItems: "center", gap: 7 }, muted: { opacity: 0.58, textAlign: "center" },
  errorBar: { paddingVertical: 7, paddingHorizontal: 12, backgroundColor: "#FFF0F0" }, errorText: { color: "#C23C3C", fontSize: 12, textAlign: "center" },
  quoteComposer: { flexDirection: "row", alignItems: "center", borderTopWidth: StyleSheet.hairlineWidth, paddingHorizontal: 16, paddingVertical: 7 }, quoteComposerText: { flex: 1, borderLeftWidth: 3, borderLeftColor: "#0A7EA4", paddingLeft: 8 }, quoteName: { color: "#0A7EA4", fontSize: 12, lineHeight: 16, fontWeight: "700" }, quoteBody: { fontSize: 12, lineHeight: 16, opacity: 0.62 }, closeQuote: { fontSize: 25, lineHeight: 28, opacity: 0.5 },
  pendingImages: { flexDirection: "row", gap: 6, paddingHorizontal: 12, paddingTop: 7 }, pendingImageCell: { width: 58, height: 58, borderRadius: 8, overflow: "hidden" }, pendingImage: { width: "100%", height: "100%" }, pendingImageRemove: { position: "absolute", right: 2, top: 2, width: 19, height: 19, borderRadius: 10, alignItems: "center", justifyContent: "center", backgroundColor: "rgba(0,0,0,0.65)" }, pendingImageRemoveText: { color: "#FFFFFF", lineHeight: 17, fontWeight: "700" },
  composer: { flexDirection: "row", alignItems: "flex-end", gap: 8, borderTopWidth: StyleSheet.hairlineWidth, paddingHorizontal: 10, paddingVertical: 8 },
  imageButton: { width: 32, height: 40, alignItems: "center", justifyContent: "center" },
  messageInput: { flex: 1, minHeight: 40, maxHeight: 120, borderRadius: 20, paddingHorizontal: 14, paddingTop: 9, paddingBottom: 9, fontSize: 16 },
  sendButton: { width: 40, height: 40, borderRadius: 20, backgroundColor: "#0A7EA4", alignItems: "center", justifyContent: "center" }, disabled: { opacity: 0.38 },
  messageImages: { flexDirection: "row", flexWrap: "wrap", gap: 4, marginTop: 5 }, messageImage: { width: 126, height: 126, borderRadius: 8, backgroundColor: "#DCE2E5" },
});
