import { router } from "expo-router";
import * as ImagePicker from "expo-image-picker";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ActivityIndicator,
  Keyboard,
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
import { SafeAreaView, useSafeAreaInsets } from "react-native-safe-area-context";
import WebView, { type WebViewMessageEvent } from "react-native-webview";

import { ThemedText } from "@/components/themed-text";
import { ThemedView } from "@/components/themed-view";
import { IconSymbol } from "@/components/ui/icon-symbol";
import type {
  CreateEventDraftRequest,
  EventContentDoc,
  UpdateEventDraftRequest,
} from "@/lib/dto";
import {
  createEventDraft,
  deleteEventDraft,
  publishEventDraft,
  updateEventDraft,
} from "@/lib/event-api";
import { uploadLocalImageAsset } from "@/lib/media-api";
import { EVENT_RICH_EDITOR_HTML } from "@/lib/rich-editor-html";
import { useThemeColor } from "@/hooks/use-theme-color";

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

type LocalImagePayload = {
  localId: string;
  previewUri: string;
  width: number;
  height: number;
  alt?: string;
};

type UploadedImagePayload = {
  localId: string;
  assetId: string;
  uri?: string;
  publicUrl?: string;
  width: number;
  height: number;
  alt?: string;
};

type FailedImagePayload = {
  localId: string;
  reason?: string;
};

type ImagePreviewSource = {
  uri: string;
  kind: "data-uri" | "file-uri";
};

type DateTimePickerTarget = "start" | "end";
type CreateEventTab = "meta" | "content";
type ExportPurpose = "autosave" | "publish";

const EVENT_CONTENT_VERSION = 1;
const AUTOSAVE_DEBOUNCE_MS = 1500;
const AUTOSAVE_INTERVAL_MS = 15000;
const EVENT_TIME_MINUTE_INTERVAL = 5;
const EDITOR_KEYBOARD_EXTRA_BOTTOM_INSET = 64;
const IOS_PICKER_TEXT_COLOR = "#11181C";
const IOS_PICKER_ACCENT_COLOR = "#0A7EA4";
const IOS_PICKER_LOCALE = "zh-Hans-CN";
const MAX_EDITOR_PREVIEW_DATA_URI_CHARS = 4_000_000;

export default function CreateEventScreen() {
  // 背景铺到 SafeArea/KeyboardAvoidingView，避免状态栏和键盘圆角露出底层原生背景。
  const safeAreaInsets = useSafeAreaInsets();
  const screenBackground = useThemeColor({}, "background");
  const editorBackground = useThemeColor(
    { light: "#FFFFFF", dark: "#151718" },
    "background",
  );
  const editorBorderColor = useThemeColor(
    { light: "#D9E0EA", dark: "#2F3A45" },
    "background",
  );
  const webViewRef = useRef<WebView>(null);
  const pendingRequestIdRef = useRef<string | null>(null);
  const pendingExportPurposeRef = useRef<ExportPurpose | null>(null);
  const exportTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const autosaveTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const autosaveIntervalRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const mountedRef = useRef(true);
  const draftWorkCountRef = useRef(0);
  const draftEventIdRef = useRef<string | null>(null);
  const draftCreatePromiseRef = useRef<Promise<string> | null>(null);
  const lastContentDocRef = useRef<EventContentDoc>(createEmptyContentDoc());
  const savedContentSignatureRef = useRef("");
  const draftHasMeaningfulContentRef = useRef(false);
  const publishedRef = useRef(false);
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
  const [editorKeyboardInset, setEditorKeyboardInset] = useState(0);
  const [uploadingImage, setUploadingImage] = useState(false);
  const [savingDraft, setSavingDraft] = useState(false);
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    return () => {
      mountedRef.current = false;
      const draftEventId = draftEventIdRef.current;
      if (
        draftEventId &&
        !publishedRef.current &&
        !draftHasMeaningfulContentRef.current
      ) {
        void deleteEventDraft(draftEventId).catch((e) => {
          console.warn("[create-event] empty draft cleanup failed", e);
        });
      }
      if (exportTimeoutRef.current) {
        clearTimeout(exportTimeoutRef.current);
      }
      if (autosaveTimeoutRef.current) {
        clearTimeout(autosaveTimeoutRef.current);
      }
      if (autosaveIntervalRef.current) {
        clearInterval(autosaveIntervalRef.current);
      }
    };
  }, []);

  const busy = uploadingImage || submitting || savingDraft;
  const bottomSafeGap = Math.max(safeAreaInsets.bottom, 8);
  const statusMessage = error
    ? error
    : savingDraft
      ? "正在保存草稿..."
      : status;
  const headerStatusMessage = statusMessage || "支持图文排版，草稿会自动保存";
  const showHeaderStatusSpinner = Boolean(statusMessage) && busy;
  const canSubmit = useMemo(
    () => editorReady && title.trim().length > 0 && !busy,
    [busy, editorReady, title],
  );
  const pickerTitle =
    dateTimePickerTarget === "start" ? "选择开始时间" : "选择结束时间";

  const beginDraftWork = useCallback(() => {
    draftWorkCountRef.current += 1;
    if (mountedRef.current) {
      setSavingDraft(true);
    }
  }, []);

  const endDraftWork = useCallback(() => {
    draftWorkCountRef.current = Math.max(0, draftWorkCountRef.current - 1);
    if (draftWorkCountRef.current === 0 && mountedRef.current) {
      setSavingDraft(false);
    }
  }, []);

  const applyDateTime = useCallback(
    (target: DateTimePickerTarget, value: Date) => {
      const normalized = normalizePickerDate(value);
      console.info("[create-event] date time applied", {
        target,
        value: normalized.toISOString(),
      });
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
      const normalizedFallback = normalizePickerDate(fallback);
      console.info("[create-event] opening date time picker", {
        target,
        platform: Platform.OS,
        value: normalizedFallback.toISOString(),
      });
      setDraftDateTime(normalizedFallback);
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
    console.info("[create-event] date time cleared", { target });
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
        console.info("[create-event] date time picker dismissed", {
          target: dateTimePickerTarget,
          mode: dateTimePickerMode,
        });
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
        minuteInterval={EVENT_TIME_MINUTE_INTERVAL}
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
            <View style={styles.iosDateTimePickerWrap}>
              {/* iOS wheel picker 会继承系统深浅色外观。弹层固定为白底时，
                  需要显式指定浅色主题和文字色，避免深色模式下出现白底白字，
                  真机上看起来只剩中间的选中高亮条。 */}
              <DateTimePicker
                value={draftDateTime}
                mode="datetime"
                display="spinner"
                locale={IOS_PICKER_LOCALE}
                minuteInterval={EVENT_TIME_MINUTE_INTERVAL}
                textColor={IOS_PICKER_TEXT_COLOR}
                accentColor={IOS_PICKER_ACCENT_COLOR}
                themeVariant="light"
                style={styles.iosDateTimePicker}
                onChange={handleDateTimePickerChange}
              />
            </View>
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

  const startRemoteDraftCreation = useCallback(
    (params: CreateEventDraftRequest): Promise<string> => {
      if (draftEventIdRef.current) {
        return Promise.resolve(draftEventIdRef.current);
      }
      if (draftCreatePromiseRef.current) {
        return draftCreatePromiseRef.current;
      }

      beginDraftWork();
      setStatus("正在创建活动草稿...");
      const createPromise = createEventDraft(params)
        .then((draft) => {
          draftEventIdRef.current = draft.event_id;
          if (mountedRef.current) {
            setError("");
            setStatus("草稿已创建");
          }
          return draft.event_id;
        })
        .finally(() => {
          draftCreatePromiseRef.current = null;
          endDraftWork();
        });
      draftCreatePromiseRef.current = createPromise;
      return createPromise;
    },
    [beginDraftWork, endDraftWork],
  );

  const ensureRemoteDraft = useCallback(async (): Promise<string> => {
    if (draftEventIdRef.current) {
      return draftEventIdRef.current;
    }

    return startRemoteDraftCreation({
      title: emptyToNull(title),
      content: lastContentDocRef.current,
      start_at: dateToRfc3339(startAt),
      end_at: dateToRfc3339(endAt),
      location_name: emptyToNull(locationName),
      location_address: emptyToNull(locationAddress),
      capacity: parseCapacity(capacityText),
      tags: parseTags(tagText),
    });
  }, [
    capacityText,
    endAt,
    locationAddress,
    locationName,
    startAt,
    startRemoteDraftCreation,
    tagText,
    title,
  ]);

  const saveDraft = useCallback(
    async (doc: EventContentDoc): Promise<string> => {
      beginDraftWork();
      const normalizedDoc = normalizeContentDoc(doc);
      lastContentDocRef.current = normalizedDoc;
      try {
        const req = buildDraftUpdateRequest({
          title,
          doc: normalizedDoc,
          startAt,
          endAt,
          locationName,
          locationAddress,
          capacityText,
          tagText,
        });
        const signature = draftRequestSignature(req);
        if (signature === savedContentSignatureRef.current && draftEventIdRef.current) {
          setStatus("草稿已保存");
          return draftEventIdRef.current;
        }
        if (!hasMeaningfulDraftContent(req) && !draftEventIdRef.current) {
          setStatus("");
          return "";
        }
        const hasMeaningfulContent = hasMeaningfulDraftContent(req);
        const hadRemoteDraft = Boolean(draftEventIdRef.current);
        const eventId = await ensureRemoteDraft();
        if (!hadRemoteDraft && hasMeaningfulContent) {
          draftHasMeaningfulContentRef.current = true;
        }
        const saved = await updateEventDraft(eventId, req);
        draftEventIdRef.current = saved.event_id;
        savedContentSignatureRef.current = signature;
        draftHasMeaningfulContentRef.current = hasMeaningfulContent;
        setError("");
        setStatus("草稿已保存");
        return saved.event_id;
      } finally {
        endDraftWork();
      }
    },
    [
      beginDraftWork,
      capacityText,
      endAt,
      endDraftWork,
      ensureRemoteDraft,
      locationAddress,
      locationName,
      startAt,
      tagText,
      title,
    ],
  );

  const publishDoc = useCallback(
    async (doc: EventContentDoc, textLength: number, imageCount: number) => {
      try {
        const normalizedDoc = normalizeContentDoc(doc);
        const req = buildDraftUpdateRequest({
          title,
          doc: normalizedDoc,
          startAt,
          endAt,
          locationName,
          locationAddress,
          capacityText,
          tagText,
        });
        validatePublishRequest(req, textLength, imageCount);

        setStatus("正在保存草稿...");
        const eventId = await saveDraft(normalizedDoc);
        if (!eventId) {
          throw new Error("草稿尚未创建，请重试");
        }

        setStatus("正在发布活动...");
        console.info("[create-event] publishing event draft", {
          eventId,
          titleLength: req.title.length,
          textLength,
          imageCount,
          blockCount: req.content.blocks.length,
        });
        const published = await publishEventDraft(eventId);
        publishedRef.current = true;
        setStatus("发布成功");
        router.replace(`/event/${encodeURIComponent(published.event_id)}` as never);
      } catch (e) {
        console.warn("[create-event] event publish failed", e);
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
      saveDraft,
      startAt,
      tagText,
      title,
    ],
  );

  const requestEditorExport = useCallback(
    (purpose: ExportPurpose) => {
      if (!editorReady) {
        return;
      }
      if (
        purpose === "autosave" &&
        (uploadingImage || submitting || draftWorkCountRef.current > 0)
      ) {
        return;
      }
      if (purpose === "autosave" && pendingRequestIdRef.current) {
        return;
      }

      if (exportTimeoutRef.current) {
        clearTimeout(exportTimeoutRef.current);
        exportTimeoutRef.current = null;
      }

      const requestId = `${purpose}_${Date.now()}`;
      pendingRequestIdRef.current = requestId;
      pendingExportPurposeRef.current = purpose;
      webViewRef.current?.injectJavaScript(
        `window.loopEditor?.exportContent(${JSON.stringify(requestId)}); true;`,
      );

      exportTimeoutRef.current = setTimeout(() => {
        if (pendingRequestIdRef.current !== requestId) {
          return;
        }
        const timedOutPurpose = pendingExportPurposeRef.current;
        pendingRequestIdRef.current = null;
        pendingExportPurposeRef.current = null;
        if (timedOutPurpose === "publish") {
          setSubmitting(false);
          setError("编辑器响应超时，请重试");
          return;
        }
        setSavingDraft(false);
        console.warn("[create-event] autosave export timed out");
      }, 8000);
    },
    [editorReady, submitting, uploadingImage],
  );

  const scheduleAutosave = useCallback(() => {
    if (!editorReady || submitting || uploadingImage) {
      return;
    }
    if (autosaveTimeoutRef.current) {
      clearTimeout(autosaveTimeoutRef.current);
    }
    autosaveTimeoutRef.current = setTimeout(() => {
      requestEditorExport("autosave");
    }, AUTOSAVE_DEBOUNCE_MS);
  }, [editorReady, requestEditorExport, submitting, uploadingImage]);

  useEffect(() => {
    scheduleAutosave();
  }, [
    capacityText,
    endAt,
    locationAddress,
    locationName,
    scheduleAutosave,
    startAt,
    tagText,
    title,
  ]);

  useEffect(() => {
    if (!editorReady) {
      return;
    }
    autosaveIntervalRef.current = setInterval(() => {
      requestEditorExport("autosave");
    }, AUTOSAVE_INTERVAL_MS);
    return () => {
      if (autosaveIntervalRef.current) {
        clearInterval(autosaveIntervalRef.current);
        autosaveIntervalRef.current = null;
      }
    };
  }, [editorReady, requestEditorExport]);

  useEffect(() => {
    if (Platform.OS !== "ios") {
      return;
    }

    const showSub = Keyboard.addListener("keyboardWillShow", () => {
      setEditorKeyboardInset(EDITOR_KEYBOARD_EXTRA_BOTTOM_INSET);
    });
    const hideSub = Keyboard.addListener("keyboardWillHide", () => {
      setEditorKeyboardInset(0);
    });

    return () => {
      showSub.remove();
      hideSub.remove();
    };
  }, []);

  const injectLocalImage = useCallback((payload: LocalImagePayload) => {
    webViewRef.current?.injectJavaScript(
      `window.loopEditor?.insertLocalImage(${JSON.stringify(payload)}); true;`,
    );
  }, []);

  const updateUploadedImage = useCallback((payload: UploadedImagePayload) => {
    webViewRef.current?.injectJavaScript(
      `window.loopEditor?.updateUploadedImage(${JSON.stringify(payload)}); true;`,
    );
  }, []);

  const markImageUploadFailed = useCallback((payload: FailedImagePayload) => {
    webViewRef.current?.injectJavaScript(
      `window.loopEditor?.markImageUploadFailed(${JSON.stringify(payload)}); true;`,
    );
  }, []);

  const updateEditorViewportInsets = useCallback((bottom: number) => {
    const safeBottom = Math.max(0, Math.round(bottom));
    webViewRef.current?.injectJavaScript(
      `window.loopEditor?.setViewportInsets({ bottom: ${safeBottom} }); true;`,
    );
  }, []);

  useEffect(() => {
    if (!editorReady) {
      return;
    }
    updateEditorViewportInsets(editorKeyboardInset);
  }, [editorKeyboardInset, editorReady, updateEditorViewportInsets]);

  const handlePickImages = useCallback(async () => {
    if (busy || draftWorkCountRef.current > 0) {
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
        base64: true,
        quality: 0.92,
      });
      if (result.canceled || !result.assets.length) {
        return;
      }

      const pickedImages = result.assets.map((asset, index) => {
        const localId = createLocalImageId(index);
        const preview = createEditorImagePreviewSource(asset);
        const alt = asset.fileName ?? "活动图片";
        return { alt, asset, index, localId, preview };
      });

      // 先把全部本地预览插入正文，再逐张上传；多选大图时用户不用等第一张上传完成。
      for (const { alt, asset, index, localId, preview } of pickedImages) {
        injectLocalImage({
          localId,
          previewUri: preview.uri,
          width: asset.width,
          height: asset.height,
          alt,
        });
        console.info("[create-event] inserted local image preview", {
          localId,
          index,
          previewKind: preview.kind,
          width: asset.width,
          height: asset.height,
        });
      }

      let uploadedCount = 0;
      let failedCount = 0;
      for (const { alt, asset, index, localId } of pickedImages) {
        setStatus(`正在上传图片 ${index + 1}/${result.assets.length}...`);
        console.info("[create-event] uploading picked image", {
          localId,
          index,
          width: asset.width,
          height: asset.height,
          mimeType: asset.mimeType,
          fileSize: asset.fileSize,
        });

        try {
          const uploaded = await uploadLocalImageAsset({
            uri: asset.uri,
            mimeType: asset.mimeType ?? null,
            fileName: asset.fileName ?? null,
            width: asset.width,
            height: asset.height,
            file: asset.file ?? null,
          });

          const imagePayload: UploadedImagePayload = {
            localId,
            assetId: uploaded.asset_id,
            width: uploaded.width ?? asset.width,
            height: uploaded.height ?? asset.height,
            alt,
          };
          if (uploaded.public_url) {
            imagePayload.publicUrl = uploaded.public_url;
          }
          updateUploadedImage(imagePayload);
          uploadedCount += 1;
        } catch (uploadError) {
          failedCount += 1;
          const reason =
            uploadError instanceof Error ? uploadError.message : "图片上传失败";
          console.warn("[create-event] picked image upload failed", {
            localId,
            index,
            reason,
          });
          markImageUploadFailed({ localId, reason });
        }
      }

      if (uploadedCount > 0) {
        setStatus(
          failedCount > 0
            ? `${uploadedCount} 张图片已插入正文，${failedCount} 张上传失败`
            : `${uploadedCount} 张图片已插入正文`,
        );
        scheduleAutosave();
      }
      if (failedCount > 0 && uploadedCount === 0) {
        setError(`${failedCount} 张图片上传失败，失败图片不会发布`);
      }
    } catch (e) {
      console.warn("[create-event] image pick/upload failed", e);
      setError(e instanceof Error ? e.message : "图片上传失败");
    } finally {
      setUploadingImage(false);
    }
  }, [
    busy,
    injectLocalImage,
    markImageUploadFailed,
    scheduleAutosave,
    updateUploadedImage,
  ]);

  const handleEditorMessage = useCallback(
    (event: WebViewMessageEvent) => {
      const message = parseEditorMessage(event.nativeEvent.data);
      if (!message) {
        return;
      }

      if (message.type === "ready") {
        setEditorReady(true);
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
      const purpose = pendingExportPurposeRef.current;
      pendingExportPurposeRef.current = null;
      if (purpose === "publish") {
        void publishDoc(message.doc, message.textLength, message.imageCount);
        return;
      }
      void saveDraft(message.doc).catch((e) => {
        console.warn("[create-event] autosave failed", e);
        setSavingDraft(false);
        setStatus("");
        setError(e instanceof Error ? e.message : "草稿保存失败");
      });
    },
    [handlePickImages, publishDoc, saveDraft],
  );

  const handleSubmit = useCallback(() => {
    if (!title.trim()) {
      setActiveTab("meta");
      setError("请输入活动标题");
      return;
    }
    if (!editorReady) {
      setActiveTab("content");
      setError("编辑器尚未加载完成，请稍后再试");
      return;
    }
    if (busy || draftWorkCountRef.current > 0) {
      return;
    }

    setError("");
    setStatus("正在整理正文...");
    setSubmitting(true);
    requestEditorExport("publish");
  }, [busy, editorReady, requestEditorExport, title]);

  return (
    <SafeAreaView
      style={[styles.safeArea, { backgroundColor: screenBackground }]}
      edges={["top", "left", "right"]}
    >
      <KeyboardAvoidingView
        behavior={Platform.OS === "ios" ? "padding" : undefined}
        style={[styles.flex, { backgroundColor: screenBackground }]}
      >
        <ThemedView
          style={[styles.container, { backgroundColor: screenBackground }]}
        >
          <View style={styles.header}>
            <Pressable
              accessibilityRole="button"
              onPress={() => router.back()}
              style={styles.iconButton}
            >
              <IconSymbol size={22} name="chevron.left" color="#11181C" />
            </Pressable>
            <View style={styles.headerTitleWrap}>
              <ThemedText type="subtitle">创建活动</ThemedText>
              <View style={styles.headerMetaRow}>
                {showHeaderStatusSpinner ? (
                  <ActivityIndicator size="small" color="#0A7EA4" />
                ) : null}
                <ThemedText
                  numberOfLines={1}
                  style={[styles.headerMeta, error ? styles.errorText : undefined]}
                >
                  {headerStatusMessage}
                </ThemedText>
              </View>
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
                placeholder="活动标题（最多 80 字）"
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
                {
                  backgroundColor: editorBackground,
                  borderColor: editorBorderColor,
                },
                activeTab !== "content" ? styles.hiddenTab : undefined,
              ]}
            >
              <WebView
                ref={webViewRef}
                source={{ html: EVENT_RICH_EDITOR_HTML }}
                style={[styles.webView, { backgroundColor: editorBackground }]}
                containerStyle={{ backgroundColor: editorBackground }}
                originWhitelist={["*"]}
                javaScriptEnabled
                domStorageEnabled
                allowFileAccess
                allowFileAccessFromFileURLs
                keyboardDisplayRequiresUserAction={false}
                hideKeyboardAccessoryView
                automaticallyAdjustContentInsets={false}
                contentInsetAdjustmentBehavior="never"
                bounces={false}
                setSupportMultipleWindows={false}
                onMessage={handleEditorMessage}
                onError={(event) => {
                  console.warn("[create-event] editor webview error", event.nativeEvent);
                  setError("编辑器加载失败");
                }}
              />
            </View>
          </View>

          {/* 正文编辑器在部分圆角屏设备上会贴近物理屏幕底部。
              这里保留稳定的底部缓冲，避免编辑器下方圆角被屏幕圆角裁掉。 */}
          <View style={[styles.bottomSafeGap, { height: bottomSafeGap }]} />

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

function buildDraftUpdateRequest({
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
}): UpdateEventDraftRequest {
  if (startAt && endAt && endAt.getTime() <= startAt.getTime()) {
    throw new Error("结束时间必须晚于开始时间");
  }

  return {
    title: title.trim(),
    content: normalizeContentDoc(doc),
    start_at: dateToRfc3339(startAt),
    end_at: dateToRfc3339(endAt),
    location_name: emptyToNull(locationName),
    location_address: emptyToNull(locationAddress),
    capacity: parseCapacity(capacityText),
    tags: parseTags(tagText),
  };
}

function validatePublishRequest(
  req: UpdateEventDraftRequest,
  textLength: number,
  imageCount: number,
) {
  if (!req.title.trim()) {
    throw new Error("请输入活动标题");
  }
  if (req.content.blocks.length === 0 || (textLength <= 0 && imageCount <= 0)) {
    throw new Error("请先填写活动正文或插入图片");
  }
}

function draftRequestSignature(req: UpdateEventDraftRequest): string {
  return JSON.stringify(req);
}

function hasMeaningfulDraftContent(req: UpdateEventDraftRequest): boolean {
  return (
    Boolean(req.title.trim()) ||
    req.content.blocks.length > 0 ||
    Boolean(req.start_at) ||
    Boolean(req.end_at) ||
    Boolean(req.location_name?.trim()) ||
    Boolean(req.location_address?.trim()) ||
    req.capacity !== null ||
    req.tags.length > 0
  );
}

function createEmptyContentDoc(): EventContentDoc {
  return {
    version: EVENT_CONTENT_VERSION,
    blocks: [],
  };
}

function normalizeContentDoc(doc: EventContentDoc): EventContentDoc {
  return {
    version: EVENT_CONTENT_VERSION,
    blocks: doc.blocks,
  };
}

function createLocalImageId(index: number): string {
  return `local_img_${Date.now().toString(36)}_${index}_${Math.random()
    .toString(36)
    .slice(2, 8)}`;
}

function createEditorImagePreviewSource(
  asset: ImagePicker.ImagePickerAsset,
): ImagePreviewSource {
  // WebView 对 file:// 图片的解码兼容性不稳定，优先用 data URI；过大时回退文件 URI，避免 JS 注入超长脚本。
  const dataUri = createImageDataUri(asset);
  if (dataUri && dataUri.length <= MAX_EDITOR_PREVIEW_DATA_URI_CHARS) {
    return { uri: dataUri, kind: "data-uri" };
  }

  if (dataUri) {
    console.info("[create-event] image preview data uri too large, using file uri", {
      length: dataUri.length,
      limit: MAX_EDITOR_PREVIEW_DATA_URI_CHARS,
      fileName: asset.fileName ?? "",
    });
  }
  return { uri: asset.uri, kind: "file-uri" };
}

function createImageDataUri(asset: ImagePicker.ImagePickerAsset): string | null {
  const base64 = asset.base64?.trim();
  if (!base64) {
    return null;
  }
  const mimeType = normalizePreviewMimeType(
    asset.mimeType,
    asset.fileName ?? asset.uri,
  );
  return `data:${mimeType};base64,${base64}`;
}

function normalizePreviewMimeType(
  value: string | null | undefined,
  fallbackName: string,
): string {
  const normalized = value?.trim().toLowerCase();
  if (normalized?.startsWith("image/")) {
    return normalized;
  }

  const lowerName = fallbackName.toLowerCase();
  if (lowerName.endsWith(".png")) {
    return "image/png";
  }
  if (lowerName.endsWith(".webp")) {
    return "image/webp";
  }
  if (lowerName.endsWith(".gif")) {
    return "image/gif";
  }
  return "image/jpeg";
}

function parseEditorMessage(raw: string): EditorMessage | null {
  try {
    const value = JSON.parse(raw) as unknown;
    if (!value || typeof value !== "object") {
      return null;
    }

    const message = value as Record<string, unknown>;
    const messageType = message["type"];
    if (messageType === "ready" || messageType === "pick_image") {
      return { type: messageType };
    }
    if (messageType === "log" && typeof message["message"] === "string") {
      const level =
        message["level"] === "warn" || message["level"] === "error"
          ? message["level"]
          : "info";
      return {
        type: "log",
        level,
        message: message["message"],
        extra: message["extra"],
      };
    }
    if (
      messageType === "content" &&
      typeof message["requestId"] === "string" &&
      isEventContentDoc(message["doc"])
    ) {
      return {
        type: "content",
        requestId: message["requestId"],
        doc: message["doc"],
        textLength: toNumber(message["textLength"]),
        imageCount: toNumber(message["imageCount"]),
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
  headerMetaRow: {
    minHeight: 18,
    flexDirection: "row",
    alignItems: "center",
    gap: 6,
  },
  headerMeta: {
    marginTop: 2,
    flex: 1,
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
  errorText: {
    color: "#D64545",
    opacity: 1,
  },
  bottomSafeGap: {
    height: 8,
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
  iosDateTimePickerWrap: {
    minHeight: 216,
    overflow: "hidden",
    borderRadius: 12,
    backgroundColor: "#FFFFFF",
  },
  iosDateTimePicker: {
    width: "100%",
    height: 216,
    backgroundColor: "#FFFFFF",
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
