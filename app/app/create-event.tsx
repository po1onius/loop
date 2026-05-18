import { router } from "expo-router";
import * as ImagePicker from "expo-image-picker";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ActivityIndicator,
  KeyboardAvoidingView,
  Modal,
  Platform,
  Pressable,
  ScrollView,
  StyleSheet,
  TextInput,
  View,
} from "react-native";
import DateTimePicker, {
  type DateTimePickerEvent,
} from "@react-native-community/datetimepicker";
import { SafeAreaView } from "react-native-safe-area-context";
import WebView, { type WebViewMessageEvent } from "react-native-webview";

import { ThemedText } from "@/components/themed-text";
import { ThemedView } from "@/components/themed-view";
import { IconSymbol } from "@/components/ui/icon-symbol";
import type { CreateEventRequest, EventContentDoc } from "@/lib/dto";
import { createEvent } from "@/lib/event-api";
import { uploadLocalImageAsset } from "@/lib/media-api";
import { EVENT_RICH_EDITOR_HTML } from "@/lib/rich-editor-html";

type NativeDateTimePickerMode = "date" | "time" | "datetime";

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

type DateTimePickerTarget = "start" | "end";
type CreateEventTab = "meta" | "content";

export default function CreateEventScreen() {
  const webViewRef = useRef<WebView>(null);
  const pendingRequestIdRef = useRef<string | null>(null);
  const exportTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [editorReady, setEditorReady] = useState(false);
  const [activeTab, setActiveTab] = useState<CreateEventTab>("meta");
  const [title, setTitle] = useState("");
  const [startAt, setStartAt] = useState<Date | null>(null);
  const [endAt, setEndAt] = useState<Date | null>(null);
  const [dateTimePickerTarget, setDateTimePickerTarget] =
    useState<DateTimePickerTarget | null>(null);
  const [dateTimePickerMode, setDateTimePickerMode] =
    useState<NativeDateTimePickerMode | null>(null);
  const [draftDateTime, setDraftDateTime] = useState(() =>
    createDefaultEventDate(),
  );
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
  const pickerTitle =
    dateTimePickerTarget === "start" ? "选择开始时间" : "选择结束时间";

  const applyDateTime = useCallback(
    (target: DateTimePickerTarget, value: Date) => {
      const normalized = normalizePickerDate(value);
      if (target === "start") {
        setStartAt(normalized);
        return;
      }
      setEndAt(normalized);
    },
    [],
  );

  const openDateTimePicker = useCallback(
    (target: DateTimePickerTarget) => {
      const fallback =
        target === "start"
          ? startAt ?? createDefaultEventDate()
          : endAt ?? (startAt ? addHours(startAt, 2) : createDefaultEventDate());
      setDraftDateTime(normalizePickerDate(fallback));
      setDateTimePickerTarget(target);
      setDateTimePickerMode(Platform.OS === "ios" ? "datetime" : "date");
    },
    [endAt, startAt],
  );

  const closeDateTimePicker = useCallback(() => {
    setDateTimePickerTarget(null);
    setDateTimePickerMode(null);
  }, []);

  const clearDateTime = useCallback((target: DateTimePickerTarget) => {
    if (target === "start") {
      setStartAt(null);
      return;
    }
    setEndAt(null);
  }, []);

  const clearActiveDateTime = useCallback(() => {
    if (dateTimePickerTarget) {
      clearDateTime(dateTimePickerTarget);
    }
    closeDateTimePicker();
  }, [clearDateTime, closeDateTimePicker, dateTimePickerTarget]);

  const confirmDateTimePicker = useCallback(() => {
    if (dateTimePickerTarget) {
      applyDateTime(dateTimePickerTarget, draftDateTime);
    }
    closeDateTimePicker();
  }, [applyDateTime, closeDateTimePicker, dateTimePickerTarget, draftDateTime]);

  const handleDateTimePickerChange = useCallback(
    (event: DateTimePickerEvent, selectedDate?: Date) => {
      if (!dateTimePickerTarget) {
        return;
      }

      if (Platform.OS === "ios") {
        if (selectedDate) {
          setDraftDateTime(normalizePickerDate(selectedDate));
        }
        return;
      }

      if (event.type === "dismissed" || !selectedDate) {
        closeDateTimePicker();
        return;
      }

      if (dateTimePickerMode === "date") {
        setDraftDateTime((current) => mergeDatePart(current, selectedDate));
        setDateTimePickerMode("time");
        return;
      }

      if (dateTimePickerMode === "time") {
        const selectedValue = mergeTimePart(draftDateTime, selectedDate);
        applyDateTime(dateTimePickerTarget, selectedValue);
        closeDateTimePicker();
      }
    },
    [
      applyDateTime,
      closeDateTimePicker,
      dateTimePickerMode,
      dateTimePickerTarget,
      draftDateTime,
    ],
  );

  const showNativeDateTimePicker =
    dateTimePickerTarget !== null && dateTimePickerMode !== null;
  const androidDateTimePickerMode =
    dateTimePickerMode === "time" ? "time" : "date";

  const nativeDateTimePicker =
    showNativeDateTimePicker && Platform.OS === "android" ? (
      <DateTimePicker
        value={draftDateTime}
        mode={androidDateTimePickerMode}
        display={androidDateTimePickerMode === "time" ? "clock" : "calendar"}
        minuteInterval={5}
        onChange={handleDateTimePickerChange}
      />
    ) : null;

  const iosDateTimePicker =
    showNativeDateTimePicker && Platform.OS === "ios" ? (
      <Modal
        animationType="slide"
        transparent
        visible
        onRequestClose={closeDateTimePicker}
      >
        <View style={styles.modalBackdrop}>
          <Pressable style={styles.modalScrim} onPress={closeDateTimePicker} />
          <View style={styles.dateTimeSheet}>
            <View style={styles.sheetHeader}>
              <ThemedText type="defaultSemiBold" style={styles.sheetTitle}>
                {pickerTitle}
              </ThemedText>
              <Pressable accessibilityRole="button" onPress={clearActiveDateTime}>
                <ThemedText style={styles.clearText}>清除</ThemedText>
              </Pressable>
            </View>
            <DateTimePicker
              value={draftDateTime}
              mode="datetime"
              display="spinner"
              minuteInterval={5}
              onChange={handleDateTimePickerChange}
            />
            <View style={styles.sheetActions}>
              <Pressable
                accessibilityRole="button"
                onPress={closeDateTimePicker}
                style={[styles.sheetButton, styles.secondarySheetButton]}
              >
                <ThemedText style={styles.secondarySheetButtonText}>取消</ThemedText>
              </Pressable>
              <Pressable
                accessibilityRole="button"
                onPress={confirmDateTimePicker}
                style={[styles.sheetButton, styles.primarySheetButton]}
              >
                <ThemedText style={styles.primarySheetButtonText}>确定</ThemedText>
              </Pressable>
            </View>
          </View>
        </View>
      </Modal>
    ) : null;

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
          startAt,
          endAt,
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
        const message = e instanceof Error ? e.message : "发布失败";
        setActiveTab(tabForValidationError(message));
        setError(message);
      } finally {
        setSubmitting(false);
      }
    },
    [
      capacityText,
      endAt,
      locationAddress,
      locationName,
      startAt,
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
      setActiveTab("meta");
      setError("请输入任务标题");
      return;
    }
    if (!editorReady) {
      setActiveTab("content");
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

          <View style={styles.tabBar}>
            <CreateEventTabButton
              active={activeTab === "meta"}
              label="信息"
              onPress={() => setActiveTab("meta")}
            />
            <CreateEventTabButton
              active={activeTab === "content"}
              label="正文"
              onPress={() => setActiveTab("content")}
            />
          </View>

          <View style={styles.tabBody}>
            <ScrollView
              keyboardShouldPersistTaps="handled"
              showsVerticalScrollIndicator={false}
              style={[
                styles.metaScroll,
                activeTab !== "meta" ? styles.hiddenTab : undefined,
              ]}
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
                <DateTimeField
                  label="开始时间"
                  value={startAt}
                  onPress={() => openDateTimePicker("start")}
                  onClear={() => clearDateTime("start")}
                  disabled={busy}
                />
                <DateTimeField
                  label="结束时间"
                  value={endAt}
                  onPress={() => openDateTimePicker("end")}
                  onClear={() => clearDateTime("end")}
                  disabled={busy}
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

            <View
              style={[
                styles.editorFrame,
                activeTab !== "content" ? styles.hiddenTab : undefined,
              ]}
            >
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

          {nativeDateTimePicker}
          {iosDateTimePicker}
        </ThemedView>
      </KeyboardAvoidingView>
    </SafeAreaView>
  );
}

function CreateEventTabButton({
  active,
  label,
  onPress,
}: {
  active: boolean;
  label: string;
  onPress: () => void;
}) {
  return (
    <Pressable
      accessibilityRole="tab"
      accessibilityState={{ selected: active }}
      onPress={onPress}
      style={[styles.tabButton, active ? styles.tabButtonActive : undefined]}
    >
      <ThemedText
        style={[styles.tabButtonText, active ? styles.tabButtonTextActive : undefined]}
      >
        {label}
      </ThemedText>
    </Pressable>
  );
}

function DateTimeField({
  label,
  value,
  disabled,
  onPress,
  onClear,
}: {
  label: string;
  value: Date | null;
  disabled: boolean;
  onPress: () => void;
  onClear: () => void;
}) {
  return (
    <Pressable
      accessibilityRole="button"
      onPress={onPress}
      disabled={disabled}
      style={[
        styles.timeField,
        styles.rowInput,
        disabled ? styles.buttonDisabled : undefined,
      ]}
    >
      <ThemedText style={styles.timeFieldLabel}>{label}</ThemedText>
      <ThemedText
        numberOfLines={1}
        style={[
          styles.timeFieldValue,
          !value ? styles.timeFieldPlaceholder : undefined,
        ]}
      >
        {value ? formatLocalDateTime(value) : "请选择"}
      </ThemedText>
      {value ? (
        <Pressable
          accessibilityRole="button"
          onPress={(event) => {
            event.stopPropagation();
            onClear();
          }}
          disabled={disabled}
          style={styles.timeFieldClear}
        >
          <ThemedText style={styles.timeFieldClearText}>清除</ThemedText>
        </Pressable>
      ) : null}
    </Pressable>
  );
}

function buildCreateEventRequest({
  title,
  doc,
  startAt,
  endAt,
  locationName,
  locationAddress,
  capacityText,
  tagText,
}: {
  title: string;
  doc: EventContentDoc;
  startAt: Date | null;
  endAt: Date | null;
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

  if (startAt && endAt && endAt.getTime() <= startAt.getTime()) {
    throw new Error("结束时间必须晚于开始时间");
  }

  return {
    title: normalizedTitle,
    content: doc,
    start_at: dateToRfc3339(startAt),
    end_at: dateToRfc3339(endAt),
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

function dateToRfc3339(value: Date | null): string | null {
  return value ? value.toISOString() : null;
}

function createDefaultEventDate(): Date {
  const next = addHours(new Date(), 1);
  return roundToNextMinuteStep(next, 15);
}

function addHours(value: Date, hours: number): Date {
  const next = new Date(value);
  next.setHours(next.getHours() + hours);
  return normalizePickerDate(next);
}

function addMinutes(value: Date, minutes: number): Date {
  const next = new Date(value);
  next.setMinutes(next.getMinutes() + minutes);
  return normalizePickerDate(next);
}

function roundToNextMinuteStep(value: Date, step: number): Date {
  const next = normalizePickerDate(value);
  const remainder = next.getMinutes() % step;
  if (remainder === 0) {
    return next;
  }
  return addMinutes(next, step - remainder);
}

function normalizePickerDate(value: Date): Date {
  const next = new Date(value);
  next.setSeconds(0, 0);
  return next;
}

function mergeDatePart(current: Date, selectedDate: Date): Date {
  const next = new Date(current);
  next.setFullYear(
    selectedDate.getFullYear(),
    selectedDate.getMonth(),
    selectedDate.getDate(),
  );
  return normalizePickerDate(next);
}

function mergeTimePart(current: Date, selectedTime: Date): Date {
  const next = new Date(current);
  next.setHours(selectedTime.getHours(), selectedTime.getMinutes(), 0, 0);
  return normalizePickerDate(next);
}

function formatLocalDateTime(value: Date): string {
  return `${formatDate(value)} ${pad2(value.getHours())}:${pad2(value.getMinutes())}`;
}

function formatDate(value: Date): string {
  return `${value.getFullYear()}-${pad2(value.getMonth() + 1)}-${pad2(value.getDate())}`;
}

function pad2(value: number): string {
  return String(value).padStart(2, "0");
}

function tabForValidationError(message: string): CreateEventTab {
  return message.includes("正文") || message.includes("图片") ? "content" : "meta";
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
  tabBar: {
    minHeight: 42,
    flexDirection: "row",
    gap: 6,
    padding: 4,
    borderRadius: 10,
    backgroundColor: "#EEF3F7",
  },
  tabButton: {
    flex: 1,
    minHeight: 34,
    borderRadius: 8,
    alignItems: "center",
    justifyContent: "center",
  },
  tabButtonActive: {
    backgroundColor: "#FFFFFF",
    borderWidth: 1,
    borderColor: "#D9E0EA",
  },
  tabButtonText: {
    fontSize: 14,
    lineHeight: 20,
    color: "#687076",
    fontWeight: "700",
  },
  tabButtonTextActive: {
    color: "#0A7EA4",
  },
  tabBody: {
    flex: 1,
    minHeight: 0,
  },
  hiddenTab: {
    display: "none",
  },
  metaScroll: {
    flex: 1,
  },
  metaContent: {
    gap: 8,
    paddingBottom: 18,
  },
  row: {
    flexDirection: "row",
    gap: 8,
  },
  rowInput: {
    flex: 1,
  },
  timeField: {
    minHeight: 56,
    borderRadius: 10,
    borderWidth: 1,
    borderColor: "#CDD3DD",
    paddingHorizontal: 12,
    paddingVertical: 8,
    justifyContent: "center",
    backgroundColor: "#FFFFFF",
  },
  timeFieldLabel: {
    fontSize: 12,
    lineHeight: 16,
    color: "#687076",
  },
  timeFieldValue: {
    marginTop: 2,
    paddingRight: 34,
    fontSize: 15,
    lineHeight: 20,
    color: "#11181C",
    fontWeight: "600",
  },
  timeFieldPlaceholder: {
    color: "#8A94A6",
    fontWeight: "400",
  },
  timeFieldClear: {
    position: "absolute",
    right: 10,
    top: 8,
    minHeight: 26,
    justifyContent: "center",
  },
  timeFieldClearText: {
    fontSize: 12,
    lineHeight: 16,
    color: "#D64545",
    fontWeight: "600",
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
  modalBackdrop: {
    flex: 1,
    justifyContent: "flex-end",
  },
  modalScrim: {
    ...StyleSheet.absoluteFillObject,
    backgroundColor: "rgba(17, 24, 28, 0.42)",
  },
  dateTimeSheet: {
    borderTopLeftRadius: 18,
    borderTopRightRadius: 18,
    paddingHorizontal: 16,
    paddingTop: 16,
    paddingBottom: 18,
    backgroundColor: "#FFFFFF",
    gap: 14,
  },
  sheetHeader: {
    minHeight: 34,
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    gap: 12,
  },
  sheetTitle: {
    flex: 1,
    color: "#11181C",
  },
  clearText: {
    fontSize: 14,
    lineHeight: 20,
    color: "#D64545",
    fontWeight: "600",
  },
  sheetActions: {
    flexDirection: "row",
    gap: 10,
  },
  sheetButton: {
    flex: 1,
    height: 46,
    borderRadius: 10,
    alignItems: "center",
    justifyContent: "center",
  },
  secondarySheetButton: {
    borderWidth: 1,
    borderColor: "#CDD3DD",
    backgroundColor: "#FFFFFF",
  },
  secondarySheetButtonText: {
    color: "#11181C",
    fontWeight: "700",
  },
  primarySheetButton: {
    backgroundColor: "#0A7EA4",
  },
  primarySheetButtonText: {
    color: "#FFFFFF",
    fontWeight: "700",
  },
});
