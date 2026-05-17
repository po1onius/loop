import { router } from "expo-router";
import * as ImagePicker from "expo-image-picker";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ActivityIndicator,
  KeyboardAvoidingView,
  Platform,
  Pressable,
  ScrollView,
  StyleSheet,
  TextInput,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import WebView, { type WebViewMessageEvent } from "react-native-webview";

import { ThemedText } from "@/components/themed-text";
import { ThemedView } from "@/components/themed-view";
import { IconSymbol } from "@/components/ui/icon-symbol";
import type { CreateEventRequest, EventContentDoc } from "@/lib/dto";
import { createEvent } from "@/lib/event-api";
import { uploadLocalImageAsset } from "@/lib/media-api";
import { EVENT_RICH_EDITOR_HTML } from "@/lib/rich-editor-html";

type EditorMessage =
  | { type: "ready" }
  | { type: "pick_image" }
  | {
      type: "content";
      requestId: string;
      doc: EventContentDoc;
      textLength: number;
      imageCount: number;
    }
  | {
      type: "log";
      level: "info" | "warn" | "error";
      message: string;
      extra: unknown;
    };

type UploadedImagePayload = {
  assetId: string;
  uri: string;
  publicUrl?: string;
  width: number;
  height: number;
  alt?: string;
};

export default function CreateEventScreen() {
  const webViewRef = useRef<WebView>(null);
  const pendingRequestIdRef = useRef<string | null>(null);
  const exportTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [editorReady, setEditorReady] = useState(false);
  const [title, setTitle] = useState("");
  const [startAtText, setStartAtText] = useState("");
  const [endAtText, setEndAtText] = useState("");
  const [locationName, setLocationName] = useState("");
  const [locationAddress, setLocationAddress] = useState("");
  const [capacityText, setCapacityText] = useState("");
  const [tagText, setTagText] = useState("");
  const [error, setError] = useState("");
  const [status, setStatus] = useState("");
  const [uploadingImage, setUploadingImage] = useState(false);
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    return () => {
      if (exportTimeoutRef.current) {
        clearTimeout(exportTimeoutRef.current);
      }
    };
  }, []);

  const busy = uploadingImage || submitting;
  const canSubmit = useMemo(
    () => editorReady && title.trim().length > 0 && !busy,
    [busy, editorReady, title],
  );

  const injectUploadedImage = useCallback((payload: UploadedImagePayload) => {
    webViewRef.current?.injectJavaScript(
      `window.loopEditor?.insertUploadedImage(${JSON.stringify(payload)}); true;`,
    );
  }, []);

  const handlePickImages = useCallback(async () => {
    if (busy) {
      return;
    }

    setError("");
    setStatus("");
    setUploadingImage(true);
    try {
      const permission = await ImagePicker.requestMediaLibraryPermissionsAsync();
      if (!permission.granted) {
        throw new Error("需要相册权限才能插入图片");
      }

      const result = await ImagePicker.launchImageLibraryAsync({
        mediaTypes: ["images"],
        allowsMultipleSelection: true,
        orderedSelection: true,
        selectionLimit: 9,
        quality: 0.92,
      });
      if (result.canceled || !result.assets.length) {
        return;
      }

      for (const [index, asset] of result.assets.entries()) {
        setStatus(`正在上传图片 ${index + 1}/${result.assets.length}...`);
        console.info("[create-event] uploading picked image", {
          index,
          width: asset.width,
          height: asset.height,
          mimeType: asset.mimeType,
          fileSize: asset.fileSize,
        });

        const uploaded = await uploadLocalImageAsset({
          uri: asset.uri,
          mimeType: asset.mimeType,
          fileName: asset.fileName,
          fileSize: asset.fileSize,
          width: asset.width,
          height: asset.height,
          file: asset.file ?? null,
        });

        injectUploadedImage({
          assetId: uploaded.asset_id,
          uri: asset.uri,
          publicUrl: uploaded.public_url ?? undefined,
          width: uploaded.width ?? asset.width,
          height: uploaded.height ?? asset.height,
          alt: asset.fileName ?? "任务图片",
        });
      }

      setStatus(`${result.assets.length} 张图片已插入正文`);
    } catch (e) {
      console.warn("[create-event] image pick/upload failed", e);
      setError(e instanceof Error ? e.message : "图片上传失败");
    } finally {
      setUploadingImage(false);
    }
  }, [busy, injectUploadedImage]);

  const submitDoc = useCallback(
    async (doc: EventContentDoc, textLength: number, imageCount: number) => {
      try {
        const req = buildCreateEventRequest({
          title,
          doc,
          startAtText,
          endAtText,
          locationName,
          locationAddress,
          capacityText,
          tagText,
        });

        setStatus("正在发布任务...");
        console.info("[create-event] submitting event", {
          titleLength: req.title.length,
          textLength,
          imageCount,
          blockCount: req.content.blocks.length,
        });
        await createEvent(req);
        setStatus("发布成功");
        router.replace("/(tabs)" as never);
      } catch (e) {
        console.warn("[create-event] event submit failed", e);
        setError(e instanceof Error ? e.message : "发布失败");
      } finally {
        setSubmitting(false);
      }
    },
    [
      capacityText,
      endAtText,
      locationAddress,
      locationName,
      startAtText,
      tagText,
      title,
    ],
  );

  const handleEditorMessage = useCallback(
    (event: WebViewMessageEvent) => {
      const message = parseEditorMessage(event.nativeEvent.data);
      if (!message) {
        return;
      }

      if (message.type === "ready") {
        setEditorReady(true);
        setStatus("编辑器已就绪");
        return;
      }

      if (message.type === "pick_image") {
        void handlePickImages();
        return;
      }

      if (message.type === "log") {
        console[message.level]("[rich-editor]", message.message, message.extra);
        if (message.level === "error") {
          setError("编辑器处理正文失败");
          setSubmitting(false);
        }
        return;
      }

      if (message.requestId !== pendingRequestIdRef.current) {
        return;
      }

      if (exportTimeoutRef.current) {
        clearTimeout(exportTimeoutRef.current);
        exportTimeoutRef.current = null;
      }
      pendingRequestIdRef.current = null;
      void submitDoc(message.doc, message.textLength, message.imageCount);
    },
    [handlePickImages, submitDoc],
  );

  const handleSubmit = useCallback(() => {
    if (!title.trim()) {
      setError("请输入任务标题");
      return;
    }
    if (!editorReady) {
      setError("编辑器尚未加载完成，请稍后再试");
      return;
    }
    if (busy) {
      return;
    }

    setError("");
    setStatus("正在整理正文...");
    setSubmitting(true);
    const requestId = `submit_${Date.now()}`;
    pendingRequestIdRef.current = requestId;
    webViewRef.current?.injectJavaScript(
      `window.loopEditor?.exportContent(${JSON.stringify(requestId)}); true;`,
    );

    exportTimeoutRef.current = setTimeout(() => {
      if (pendingRequestIdRef.current !== requestId) {
        return;
      }
      pendingRequestIdRef.current = null;
      setSubmitting(false);
      setError("编辑器响应超时，请重试");
    }, 8000);
  }, [busy, editorReady, title]);

  return (
    <SafeAreaView style={styles.safeArea} edges={["top", "left", "right"]}>
      <KeyboardAvoidingView
        behavior={Platform.OS === "ios" ? "padding" : undefined}
        style={styles.flex}
      >
        <ThemedView style={styles.container}>
          <View style={styles.header}>
            <Pressable
              accessibilityRole="button"
              onPress={() => router.back()}
              style={styles.iconButton}
            >
              <IconSymbol size={22} name="chevron.left" color="#11181C" />
            </Pressable>
            <View style={styles.headerTitleWrap}>
              <ThemedText type="subtitle">创建任务</ThemedText>
              <ThemedText style={styles.headerMeta}>
                富文本正文会按后端内容块结构发布
              </ThemedText>
            </View>
            <Pressable
              accessibilityRole="button"
              onPress={handleSubmit}
              disabled={!canSubmit}
              style={[
                styles.publishButton,
                !canSubmit ? styles.buttonDisabled : undefined,
              ]}
            >
              {submitting ? (
                <ActivityIndicator color="#FFFFFF" size="small" />
              ) : (
                <ThemedText style={styles.publishButtonText}>发布</ThemedText>
              )}
            </Pressable>
          </View>

          <ScrollView
            keyboardShouldPersistTaps="handled"
            showsVerticalScrollIndicator={false}
            style={styles.metaScroll}
            contentContainerStyle={styles.metaContent}
          >
            <TextInput
              style={[styles.input, styles.titleInput]}
              placeholder="任务标题（最多 80 字）"
              placeholderTextColor="#8A94A6"
              value={title}
              maxLength={80}
              onChangeText={setTitle}
            />
            <View style={styles.row}>
              <TextInput
                style={[styles.input, styles.rowInput]}
                placeholder="开始时间，如 2026-05-17 19:30"
                placeholderTextColor="#8A94A6"
                value={startAtText}
                onChangeText={setStartAtText}
              />
              <TextInput
                style={[styles.input, styles.rowInput]}
                placeholder="结束时间"
                placeholderTextColor="#8A94A6"
                value={endAtText}
                onChangeText={setEndAtText}
              />
            </View>
            <View style={styles.row}>
              <TextInput
                style={[styles.input, styles.rowInput]}
                placeholder="地点名称"
                placeholderTextColor="#8A94A6"
                value={locationName}
                onChangeText={setLocationName}
              />
              <TextInput
                style={[styles.input, styles.rowInput]}
                placeholder="人数上限"
                placeholderTextColor="#8A94A6"
                value={capacityText}
                keyboardType="number-pad"
                onChangeText={setCapacityText}
              />
            </View>
            <TextInput
              style={styles.input}
              placeholder="详细地址"
              placeholderTextColor="#8A94A6"
              value={locationAddress}
              onChangeText={setLocationAddress}
            />
            <TextInput
              style={styles.input}
              placeholder="标签，用空格或逗号分隔"
              placeholderTextColor="#8A94A6"
              value={tagText}
              onChangeText={setTagText}
            />
          </ScrollView>

          <View style={styles.editorFrame}>
            <WebView
              ref={webViewRef}
              source={{ html: EVENT_RICH_EDITOR_HTML }}
              style={styles.webView}
              originWhitelist={["*"]}
              javaScriptEnabled
              domStorageEnabled
              allowFileAccess
              allowFileAccessFromFileURLs
              keyboardDisplayRequiresUserAction={false}
              setSupportMultipleWindows={false}
              onMessage={handleEditorMessage}
              onError={(event) => {
                console.warn("[create-event] editor webview error", event.nativeEvent);
                setError("编辑器加载失败");
              }}
            />
          </View>

          {error || status || uploadingImage ? (
            <View style={styles.statusBar}>
              {busy ? <ActivityIndicator size="small" color="#0A7EA4" /> : null}
              <ThemedText
                style={[styles.statusText, error ? styles.errorText : undefined]}
              >
                {error || status}
              </ThemedText>
            </View>
          ) : null}
        </ThemedView>
      </KeyboardAvoidingView>
    </SafeAreaView>
  );
}

function buildCreateEventRequest({
  title,
  doc,
  startAtText,
  endAtText,
  locationName,
  locationAddress,
  capacityText,
  tagText,
}: {
  title: string;
  doc: EventContentDoc;
  startAtText: string;
  endAtText: string;
  locationName: string;
  locationAddress: string;
  capacityText: string;
  tagText: string;
}): CreateEventRequest {
  const normalizedTitle = title.trim();
  if (!normalizedTitle) {
    throw new Error("请输入任务标题");
  }
  if (doc.blocks.length === 0) {
    throw new Error("请先填写任务正文或插入图片");
  }

  const startAt = toOptionalRfc3339(startAtText, "开始时间");
  const endAt = toOptionalRfc3339(endAtText, "结束时间");
  if (startAt && endAt && new Date(endAt).getTime() <= new Date(startAt).getTime()) {
    throw new Error("结束时间必须晚于开始时间");
  }

  return {
    title: normalizedTitle,
    content: doc,
    start_at: startAt,
    end_at: endAt,
    location_name: emptyToNull(locationName),
    location_address: emptyToNull(locationAddress),
    capacity: parseCapacity(capacityText),
    tags: parseTags(tagText),
  };
}

function parseEditorMessage(raw: string): EditorMessage | null {
  try {
    const value = JSON.parse(raw) as unknown;
    if (!value || typeof value !== "object") {
      return null;
    }

    const message = value as Record<string, unknown>;
    if (message.type === "ready" || message.type === "pick_image") {
      return { type: message.type };
    }
    if (message.type === "log" && typeof message.message === "string") {
      const level =
        message.level === "warn" || message.level === "error"
          ? message.level
          : "info";
      return {
        type: "log",
        level,
        message: message.message,
        extra: message.extra,
      };
    }
    if (
      message.type === "content" &&
      typeof message.requestId === "string" &&
      isEventContentDoc(message.doc)
    ) {
      return {
        type: "content",
        requestId: message.requestId,
        doc: message.doc,
        textLength: toNumber(message.textLength),
        imageCount: toNumber(message.imageCount),
      };
    }
  } catch (e) {
    console.warn("[create-event] invalid editor message", e);
  }
  return null;
}

function isEventContentDoc(value: unknown): value is EventContentDoc {
  return (
    Boolean(value) &&
    typeof value === "object" &&
    Array.isArray((value as EventContentDoc).blocks)
  );
}

function toNumber(value: unknown): number {
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

function toOptionalRfc3339(value: string, fieldName: string): string | null {
  const text = value.trim();
  if (!text) {
    return null;
  }

  const normalized = text.includes("T") ? text : text.replace(/\s+/, "T");
  const date = new Date(normalized);
  if (Number.isNaN(date.getTime())) {
    throw new Error(`${fieldName}格式需为 YYYY-MM-DD HH:mm`);
  }
  return date.toISOString();
}

function emptyToNull(value: string): string | null {
  const text = value.trim();
  return text ? text : null;
}

function parseCapacity(value: string): number | null {
  const text = value.trim();
  if (!text) {
    return null;
  }

  const capacity = Number(text);
  if (!Number.isInteger(capacity) || capacity <= 0) {
    throw new Error("人数上限必须是正整数");
  }
  return capacity;
}

function parseTags(value: string): string[] {
  const tags = value
    .split(/[\s,，、]+/)
    .map((item) => item.trim().replace(/^#+/, ""))
    .filter(Boolean);
  const uniqueTags = Array.from(new Set(tags));
  if (uniqueTags.length > 10) {
    throw new Error("标签最多 10 个");
  }
  return uniqueTags;
}

const styles = StyleSheet.create({
  safeArea: {
    flex: 1,
  },
  flex: {
    flex: 1,
  },
  container: {
    flex: 1,
    paddingHorizontal: 12,
    paddingBottom: 10,
    gap: 10,
  },
  header: {
    minHeight: 52,
    flexDirection: "row",
    alignItems: "center",
    gap: 10,
  },
  iconButton: {
    width: 40,
    height: 40,
    borderRadius: 10,
    alignItems: "center",
    justifyContent: "center",
    backgroundColor: "#EEF3F7",
  },
  headerTitleWrap: {
    flex: 1,
  },
  headerMeta: {
    marginTop: 2,
    fontSize: 12,
    lineHeight: 16,
    opacity: 0.62,
  },
  publishButton: {
    width: 74,
    height: 40,
    borderRadius: 10,
    alignItems: "center",
    justifyContent: "center",
    backgroundColor: "#0A7EA4",
  },
  publishButtonText: {
    color: "#FFFFFF",
    fontWeight: "700",
  },
  buttonDisabled: {
    opacity: 0.52,
  },
  metaScroll: {
    flexGrow: 0,
    maxHeight: 232,
  },
  metaContent: {
    gap: 8,
    paddingBottom: 2,
  },
  row: {
    flexDirection: "row",
    gap: 8,
  },
  rowInput: {
    flex: 1,
  },
  input: {
    minHeight: 44,
    borderRadius: 10,
    borderWidth: 1,
    borderColor: "#CDD3DD",
    paddingHorizontal: 12,
    paddingVertical: 9,
    fontSize: 15,
    lineHeight: 20,
    color: "#11181C",
    backgroundColor: "#FFFFFF",
  },
  titleInput: {
    minHeight: 48,
    fontSize: 17,
    fontWeight: "600",
  },
  editorFrame: {
    flex: 1,
    minHeight: 330,
    overflow: "hidden",
    borderWidth: 1,
    borderColor: "#D9E0EA",
    borderRadius: 12,
    backgroundColor: "#FFFFFF",
  },
  webView: {
    flex: 1,
    backgroundColor: "transparent",
  },
  statusBar: {
    minHeight: 34,
    flexDirection: "row",
    alignItems: "center",
    gap: 8,
    paddingHorizontal: 4,
  },
  statusText: {
    flex: 1,
    fontSize: 13,
    lineHeight: 18,
    opacity: 0.72,
  },
  errorText: {
    color: "#D64545",
    opacity: 1,
  },
});
