import * as ImagePicker from "expo-image-picker";
import { router } from "expo-router";
import { useEffect, useMemo, useState } from "react";
import {
  ActivityIndicator,
  Image as NativeImage,
  KeyboardAvoidingView,
  Platform,
  Pressable,
  ScrollView,
  StyleSheet,
  TextInput,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";

import { postTypeLabel } from "@/components/community-post-card";
import { ThemedText } from "@/components/themed-text";
import { ThemedView } from "@/components/themed-view";
import { IconSymbol } from "@/components/ui/icon-symbol";
import { useColorScheme } from "@/hooks/use-color-scheme";
import { createCommunityPost, listCommunitySections } from "@/lib/community-api";
import type { CommunityPostType, CommunitySectionResp, EventResp } from "@/lib/dto";
import { listEvents } from "@/lib/event-api";
import { uploadLocalImageAsset } from "@/lib/media-api";

const POST_TYPES: CommunityPostType[] = ["event_idea", "event_discussion", "general"];

type SelectedImage = {
  localUri: string;
  assetId: string;
};

export default function CreatePostScreen() {
  const isDark = useColorScheme() === "dark";
  const [sections, setSections] = useState<CommunitySectionResp[]>([]);
  const [events, setEvents] = useState<EventResp[]>([]);
  const [sectionId, setSectionId] = useState("");
  const [postType, setPostType] = useState<CommunityPostType>("event_idea");
  const [discussedEventId, setDiscussedEventId] = useState<string | null>(null);
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [images, setImages] = useState<SelectedImage[]>([]);
  const [loadingOptions, setLoadingOptions] = useState(true);
  const [uploading, setUploading] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    let active = true;
    void Promise.all([listCommunitySections(), listEvents(30, 0)])
      .then(([nextSections, eventResp]) => {
        if (!active) return;
        setSections(nextSections);
        setSectionId((current) => current || nextSections[0]?.section_id || "");
        setEvents(eventResp.items);
        console.info("[create-post] editor options loaded", {
          sectionCount: nextSections.length,
          eventCount: eventResp.items.length,
        });
      })
      .catch((loadError) => {
        if (!active) return;
        const message = loadError instanceof Error ? loadError.message : "发帖选项加载失败";
        console.warn("[create-post] editor options load failed", { reason: message });
        setError(message);
      })
      .finally(() => active && setLoadingOptions(false));
    return () => {
      active = false;
    };
  }, []);

  const busy = loadingOptions || uploading || submitting;
  const canSubmit = useMemo(() => {
    return Boolean(sectionId && title.trim() && body.trim()) && !busy &&
      (postType !== "event_discussion" || Boolean(discussedEventId));
  }, [body, busy, discussedEventId, postType, sectionId, title]);

  const selectImages = async () => {
    if (busy || images.length >= 9) return;
    setError("");
    setUploading(true);
    try {
      const permission = await ImagePicker.requestMediaLibraryPermissionsAsync();
      if (!permission.granted) throw new Error("需要相册权限才能添加图片");
      const result = await ImagePicker.launchImageLibraryAsync({
        mediaTypes: ["images"],
        allowsMultipleSelection: true,
        selectionLimit: 9 - images.length,
        quality: 0.9,
      });
      if (result.canceled) return;
      const selected = result.assets.slice(0, 9 - images.length);
      console.info("[create-post] uploading selected images", { count: selected.length });
      // 对象上传相互独立；并行上传可以显著缩短多图发帖的等待时间。
      const uploaded = await Promise.all(selected.map(async (asset) => {
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
      setImages((current) => [...current, ...uploaded].slice(0, 9));
      console.info("[create-post] selected images uploaded", { count: uploaded.length });
    } catch (uploadError) {
      const message = uploadError instanceof Error ? uploadError.message : "图片上传失败";
      console.warn("[create-post] image upload failed", { reason: message });
      setError(message);
    } finally {
      setUploading(false);
    }
  };

  const submit = async () => {
    if (!canSubmit) return;
    setSubmitting(true);
    setError("");
    try {
      const post = await createCommunityPost({
        section_id: sectionId,
        post_type: postType,
        title: title.trim(),
        body: body.trim(),
        image_asset_ids: images.map((image) => image.assetId),
        discussed_event_id: postType === "event_discussion" ? discussedEventId : null,
      });
      console.info("[create-post] post published", {
        postId: post.post_id,
        conversationId: post.discussion_conversation_id,
      });
      router.replace(`/community/post/${encodeURIComponent(post.post_id)}` as never);
    } catch (submitError) {
      const message = submitError instanceof Error ? submitError.message : "帖子发布失败";
      console.warn("[create-post] post publish failed", { reason: message });
      setError(message);
    } finally {
      setSubmitting(false);
    }
  };

  const fieldBackground = isDark ? "#20272E" : "#F3F6F8";
  const inputColor = isDark ? "#ECEDEE" : "#11181C";
  return (
    <SafeAreaView style={styles.safeArea} edges={["top", "bottom", "left", "right"]}>
      <KeyboardAvoidingView style={styles.flex} behavior={Platform.OS === "ios" ? "padding" : undefined}>
        <ThemedView style={styles.flex}>
          <View style={styles.header}>
            <Pressable accessibilityRole="button" onPress={() => router.back()} style={styles.headerButton}>
              <IconSymbol name="chevron.left" size={25} color={inputColor} />
            </Pressable>
            <ThemedText type="subtitle">发布帖子</ThemedText>
            <Pressable accessibilityRole="button" accessibilityState={{ disabled: !canSubmit }} disabled={!canSubmit} onPress={() => void submit()} style={[styles.publishButton, !canSubmit ? styles.disabled : undefined]}>
              {submitting ? <ActivityIndicator size="small" color="#FFFFFF" /> : <ThemedText style={styles.publishText}>发布</ThemedText>}
            </Pressable>
          </View>

          <ScrollView keyboardShouldPersistTaps="handled" contentContainerStyle={styles.content}>
            <EditorSection title="帖子用途">
              <View style={styles.wrapRow}>
                {POST_TYPES.map((type) => <ChoiceChip key={type} label={postTypeLabel(type)} selected={postType === type} onPress={() => { setPostType(type); if (type !== "event_discussion") setDiscussedEventId(null); }} isDark={isDark} />)}
              </View>
              <ThemedText style={styles.hint}>{postTypeHint(postType)}</ThemedText>
            </EditorSection>

            <EditorSection title="社区分区">
              <View style={styles.wrapRow}>
                {sections.map((section) => <ChoiceChip key={section.section_id} label={section.name} selected={sectionId === section.section_id} onPress={() => setSectionId(section.section_id)} isDark={isDark} />)}
              </View>
            </EditorSection>

            {postType === "event_discussion" ? (
              <EditorSection title="关联活动">
                <ScrollView horizontal showsHorizontalScrollIndicator={false} contentContainerStyle={styles.eventChoices}>
                  {events.map((event) => <ChoiceChip key={event.event_id} label={event.title} selected={discussedEventId === event.event_id} onPress={() => setDiscussedEventId(event.event_id)} isDark={isDark} />)}
                </ScrollView>
                {!events.length && !loadingOptions ? <ThemedText style={styles.hint}>暂时没有可关联的已发布活动</ThemedText> : null}
              </EditorSection>
            ) : null}

            <EditorSection title="标题">
              <TextInput value={title} onChangeText={setTitle} maxLength={120} placeholder="一句话说清你想讨论什么" placeholderTextColor="#8A94A6" style={[styles.input, { backgroundColor: fieldBackground, color: inputColor }]} />
              <ThemedText style={styles.counter}>{title.length}/120</ThemedText>
            </EditorSection>

            <EditorSection title="正文">
              <TextInput value={body} onChangeText={setBody} maxLength={10_000} multiline textAlignVertical="top" placeholder="补充你的想法、经历或希望大家回应的问题……" placeholderTextColor="#8A94A6" style={[styles.input, styles.bodyInput, { backgroundColor: fieldBackground, color: inputColor }]} />
              <ThemedText style={styles.counter}>{body.length}/10000</ThemedText>
            </EditorSection>

            <EditorSection title={`图片（${images.length}/9）`}>
              <View style={styles.imageList}>
                {images.map((image) => (
                  <View key={image.assetId} style={[styles.imagePreview, { backgroundColor: fieldBackground }]}>
                    <ImagePickerImage uri={image.localUri} />
                    <Pressable accessibilityRole="button" accessibilityLabel="移除图片" onPress={() => setImages((current) => current.filter((item) => item.assetId !== image.assetId))} style={styles.removeImage}>
                      <ThemedText style={styles.removeImageText}>×</ThemedText>
                    </Pressable>
                  </View>
                ))}
                {images.length < 9 ? <Pressable accessibilityRole="button" disabled={busy} onPress={() => void selectImages()} style={[styles.addImage, { backgroundColor: fieldBackground }]}>{uploading ? <ActivityIndicator color="#0A7EA4" /> : <><ThemedText style={styles.addImagePlus}>＋</ThemedText><ThemedText style={styles.addImageText}>添加图片</ThemedText></>}</Pressable> : null}
              </View>
            </EditorSection>

            {error ? <ThemedText style={styles.error}>{error}</ThemedText> : null}
          </ScrollView>
        </ThemedView>
      </KeyboardAvoidingView>
    </SafeAreaView>
  );
}

function ImagePickerImage({ uri }: { uri: string }) {
  // 原生 Image 对选择器返回的本地 file:// 预览最直接，上传后的远程图则统一使用 Expo Image。
  return <NativeImage source={{ uri }} resizeMode="cover" style={styles.previewImage} />;
}

function EditorSection({ title, children }: { title: string; children: React.ReactNode }) {
  return <View style={styles.section}><ThemedText type="defaultSemiBold">{title}</ThemedText>{children}</View>;
}

function ChoiceChip({ label, selected, onPress, isDark }: { label: string; selected: boolean; onPress: () => void; isDark: boolean }) {
  return <Pressable onPress={onPress} style={[styles.choiceChip, { backgroundColor: selected ? "#0A7EA4" : isDark ? "#273039" : "#EEF2F5" }]}><ThemedText numberOfLines={1} style={[styles.choiceText, selected ? styles.choiceSelected : undefined]}>{label}</ThemedText></Pressable>;
}

function postTypeHint(type: CommunityPostType): string {
  if (type === "event_idea") return "提出活动想法，用“感兴趣”观察反响，成熟后再创建活动。";
  if (type === "event_discussion") return "围绕已经发布的活动分享见闻，需要选择关联活动。";
  return "不限定活动，适合社区内的一般交流。";
}

const styles = StyleSheet.create({
  safeArea: { flex: 1 }, flex: { flex: 1 },
  header: { height: 58, paddingHorizontal: 12, flexDirection: "row", alignItems: "center", justifyContent: "space-between" },
  headerButton: { width: 42, height: 42, justifyContent: "center" },
  publishButton: { minWidth: 62, height: 36, borderRadius: 18, alignItems: "center", justifyContent: "center", backgroundColor: "#0A7EA4", paddingHorizontal: 14 },
  publishText: { color: "#FFFFFF", fontWeight: "700", fontSize: 14 },
  disabled: { opacity: 0.38 },
  content: { padding: 16, paddingBottom: 40, gap: 24 },
  section: { gap: 10 }, wrapRow: { flexDirection: "row", flexWrap: "wrap", gap: 8 },
  choiceChip: { maxWidth: 230, borderRadius: 999, paddingHorizontal: 14, paddingVertical: 8 },
  choiceText: { fontSize: 14, lineHeight: 19 }, choiceSelected: { color: "#FFFFFF", fontWeight: "700" },
  hint: { opacity: 0.58, fontSize: 13, lineHeight: 19 }, eventChoices: { gap: 8 },
  input: { borderRadius: 12, paddingHorizontal: 14, paddingVertical: 12, fontSize: 16, lineHeight: 23 },
  bodyInput: { minHeight: 190 }, counter: { alignSelf: "flex-end", opacity: 0.48, fontSize: 12, lineHeight: 16 },
  imageList: { flexDirection: "row", flexWrap: "wrap", gap: 8 },
  imagePreview: { width: 96, height: 96, borderRadius: 10, overflow: "hidden" }, previewImage: { width: "100%", height: "100%" },
  removeImage: { position: "absolute", right: 4, top: 4, width: 24, height: 24, borderRadius: 12, alignItems: "center", justifyContent: "center", backgroundColor: "rgba(0,0,0,0.62)" },
  removeImageText: { color: "#FFFFFF", fontSize: 19, lineHeight: 20 },
  addImage: { width: 96, height: 96, borderRadius: 10, alignItems: "center", justifyContent: "center" },
  addImagePlus: { color: "#0A7EA4", fontSize: 26, lineHeight: 30 }, addImageText: { color: "#0A7EA4", fontSize: 12, lineHeight: 16 },
  error: { color: "#C23C3C", textAlign: "center" },
});
