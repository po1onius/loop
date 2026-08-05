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
import type { EventContentDoc, EventResp, UpdateEventDraftRequest } from "@/lib/dto";
import {
  createEventDraft,
  listAllMyEventDrafts,
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
type ExportPurpose = "save-draft" | "publish";
type DraftEntryState = "checking" | "choosing" | "opening" | "error" | "editing";
type SaveDraftOptions = {
  force?: boolean;
  source: "manual" | "publish";
};

const EVENT_CONTENT_VERSION = 1;
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
  const mountedRef = useRef(true);
  const editorReadyRef = useRef(false);
  const draftEntryAttemptRef = useRef(0);
  const draftWorkCountRef = useRef(0);
  const draftOpenedRef = useRef(false);
  const draftEventIdRef = useRef<string | null>(null);
  const lastContentDocRef = useRef<EventContentDoc>(createEmptyContentDoc());
  const pendingEditorContentRef = useRef<EventContentDoc | null>(null);
  const savedDraftHashRef = useRef("");
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
  const [draftOpened, setDraftOpened] = useState(false);
  const [draftEntryState, setDraftEntryState] =
    useState<DraftEntryState>("checking");
  const [existingDrafts, setExistingDrafts] = useState<EventResp[]>([]);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      if (exportTimeoutRef.current) {
        clearTimeout(exportTimeoutRef.current);
      }
    };
  }, []);

  const loadingDraft =
    draftEntryState === "checking" || draftEntryState === "opening";
  const busy = loadingDraft || uploadingImage || submitting || savingDraft;
  const bottomSafeGap = Math.max(safeAreaInsets.bottom, 8);
  const statusMessage = error
    ? error
    : loadingDraft
      ? "正在加载草稿..."
      : savingDraft
      ? "正在保存草稿..."
      : status;
  const headerStatusMessage =
    statusMessage || "请使用右上角“存草稿”主动保存";
  const showHeaderStatusSpinner = Boolean(statusMessage) && busy;
  const canSubmit = useMemo(
    () => draftOpened && editorReady && title.trim().length > 0 && !busy,
    [busy, draftOpened, editorReady, title],
  );
  const canSaveDraft = draftOpened && editorReady && !busy;
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

  const loadEditorContent = useCallback((doc: EventContentDoc) => {
    const normalizedDoc = normalizeContentDoc(doc);
    webViewRef.current?.injectJavaScript(
      `window.loopEditor?.loadContent(${JSON.stringify(normalizedDoc)}); true;`,
    );
  }, []);

  const applyDraftSnapshot = useCallback(
    (event: EventResp | null) => {
      // 选择草稿只把服务端保存的快照填入表单和正文编辑器，不建立编辑会话，
      // 也不对草稿加锁；后续只有用户主动点击“存草稿”才会写回数据库。
      const normalizedDoc = event
        ? normalizeContentDoc(event.content)
        : createEmptyContentDoc();
      const loadedReq = event
        ? eventToDraftUpdateRequest(event, normalizedDoc)
        : null;

      draftEventIdRef.current = event?.event_id ?? null;
      lastContentDocRef.current = normalizedDoc;
      pendingEditorContentRef.current = normalizedDoc;
      savedDraftHashRef.current = loadedReq ? draftRequestHash(loadedReq) : "";
      draftOpenedRef.current = true;

      setTitle(event?.title ?? "");
      setStartAt(dateFromRfc3339(event?.start_at ?? null));
      setEndAt(dateFromRfc3339(event?.end_at ?? null));
      setLocationName(event?.location_name ?? "");
      setLocationAddress(event?.location_address ?? "");
      setCapacityText(
        event?.capacity !== null && event?.capacity !== undefined
          ? String(event.capacity)
          : "",
      );
      setTagText(event?.tags.join(" ") ?? "");
      setDraftOpened(true);
      setError("");

      if (editorReadyRef.current) {
        loadEditorContent(normalizedDoc);
        pendingEditorContentRef.current = null;
      }

      console.info("[create-event] draft snapshot applied to editor", {
        eventId: event?.event_id ?? null,
        isNew: !event,
        blockCount: normalizedDoc.blocks.length,
        titleLength: event?.title.trim().length ?? 0,
      });
    },
    [loadEditorContent],
  );

  const openDraftForEditing = useCallback(
    (draft: EventResp | null) => {
      setDraftEntryState("opening");
      setError("");
      setStatus(draft ? "正在加载草稿..." : "正在新建活动...");
      console.info("[create-event] draft entry selected", {
        eventId: draft?.event_id ?? null,
        isNew: !draft,
      });

      applyDraftSnapshot(draft);
      setExistingDrafts([]);
      setDraftEntryState("editing");
      setStatus(draft ? "草稿已加载" : "已打开空白活动");
    },
    [applyDraftSnapshot],
  );

  const inspectDraftEntry = useCallback(async () => {
    const currentAttempt = (draftEntryAttemptRef.current += 1);
    setDraftEntryState("checking");
    setExistingDrafts([]);
    setError("");
    setStatus("正在检查活动草稿...");

    try {
      // 列表接口按 updated_at 倒序返回，最近保存的草稿排在最前面。
      const drafts = await listAllMyEventDrafts();
      if (
        !mountedRef.current ||
        draftEntryAttemptRef.current !== currentAttempt
      ) {
        return;
      }
      console.info("[create-event] draft entry inspected", {
        draftCount: drafts.length,
        eventIds: drafts.map((draft) => draft.event_id),
      });

      if (drafts.length > 0) {
        setExistingDrafts(drafts);
        setStatus("");
        setDraftEntryState("choosing");
        return;
      }
      openDraftForEditing(null);
    } catch (e) {
      console.warn("[create-event] inspect draft entry failed", e);
      if (
        mountedRef.current &&
        draftEntryAttemptRef.current === currentAttempt
      ) {
        setStatus("");
        setError(e instanceof Error ? e.message : "活动草稿检查失败");
        setDraftEntryState("error");
      }
    }
  }, [openDraftForEditing]);

  useEffect(() => {
    void inspectDraftEntry();
  }, [inspectDraftEntry]);

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

  const saveDraft = useCallback(
    async (
      doc: EventContentDoc,
      options: SaveDraftOptions,
    ): Promise<string> => {
      const normalizedDoc = normalizeContentDoc(doc);
      lastContentDocRef.current = normalizedDoc;
      if (!draftOpenedRef.current) {
        throw new Error("编辑器尚未准备完成，请稍后再试");
      }
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
      const draftHash = draftRequestHash(req);
      if (
        !options.force &&
        draftHash === savedDraftHashRef.current &&
        draftEventIdRef.current
      ) {
        console.info("[create-event] skip unchanged draft save", {
          source: options.source,
          eventId: draftEventIdRef.current,
          draftHash,
        });
        setStatus("草稿已保存");
        return draftEventIdRef.current;
      }
      beginDraftWork();
      try {
        const currentEventId = draftEventIdRef.current;
        // “+新建”只打开空白编辑器，首次主动保存时才创建数据库草稿；
        // 从列表选择的草稿则始终按 ID 覆盖。没有版本条件，最后完成的保存生效。
        const saved = currentEventId
          ? await updateEventDraft(currentEventId, req)
          : await createEventDraft(req);
        draftEventIdRef.current = saved.event_id;
        savedDraftHashRef.current = draftHash;
        setError("");
        setStatus("草稿已保存");
        console.info("[create-event] draft snapshot saved", {
          eventId: saved.event_id,
          operation: currentEventId ? "update" : "create",
          source: options.source,
          draftHash,
        });
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
        const eventId = await saveDraft(normalizedDoc, {
          force: true,
          source: "publish",
        });
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
      if (!editorReady || !draftOpened || pendingRequestIdRef.current) {
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
        setStatus("");
        setError("编辑器响应超时，草稿未保存，请重试");
        console.warn("[create-event] manual draft export timed out");
      }, 8000);
    },
    [draftOpened, editorReady],
  );

  useEffect(() => {
    if (!editorReady) {
      return;
    }
    const pendingDoc = pendingEditorContentRef.current;
    if (pendingDoc) {
      loadEditorContent(pendingDoc);
      pendingEditorContentRef.current = null;
    }
  }, [editorReady, loadEditorContent]);

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

  const blurEditor = useCallback(() => {
    webViewRef.current?.injectJavaScript(
      "window.loopEditor?.blurEditor?.(); true;",
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
    updateUploadedImage,
  ]);

  const handleEditorMessage = useCallback(
    (event: WebViewMessageEvent) => {
      const message = parseEditorMessage(event.nativeEvent.data);
      if (!message) {
        return;
      }

      if (message.type === "ready") {
        editorReadyRef.current = true;
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
      void saveDraft(message.doc, { source: "manual" })
        .catch((e) => {
          console.warn("[create-event] manual draft save failed", e);
          setStatus("");
          setError(e instanceof Error ? e.message : "草稿保存失败");
        })
        .finally(() => {
          // 未修改草稿会在发起网络请求前直接返回，因此仍需在这里统一关闭按钮 loading。
          if (mountedRef.current) {
            setSavingDraft(false);
          }
        });
    },
    [handlePickImages, publishDoc, saveDraft],
  );

  const handleTabChange = useCallback(
    (nextTab: CreateEventTab) => {
      if (nextTab === activeTab) {
        return;
      }

      if (activeTab === "content") {
        blurEditor();
        Keyboard.dismiss();
      }

      setActiveTab(nextTab);
    },
    [activeTab, blurEditor],
  );

  const handleSaveDraft = useCallback(() => {
    if (!draftOpened || !editorReady) {
      setError("草稿编辑器尚未加载完成，请稍后再试");
      return;
    }
    if (busy || draftWorkCountRef.current > 0) {
      return;
    }

    setError("");
    setStatus("正在整理草稿...");
    setSavingDraft(true);
    console.info("[create-event] manual draft save requested", {
      eventId: draftEventIdRef.current,
      activeTab,
    });
    requestEditorExport("save-draft");
  }, [activeTab, busy, draftOpened, editorReady, requestEditorExport]);

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
    if (!draftOpened) {
      setError("草稿尚未加载完成，请稍后再试");
      return;
    }
    if (busy || draftWorkCountRef.current > 0) {
      return;
    }

    setError("");
    setStatus("正在整理正文...");
    setSubmitting(true);
    requestEditorExport("publish");
  }, [busy, draftOpened, editorReady, requestEditorExport, title]);

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
            <View style={styles.headerActions}>
              <Pressable
                accessibilityRole="button"
                accessibilityLabel="存草稿"
                onPress={handleSaveDraft}
                disabled={!canSaveDraft}
                style={[
                  styles.saveDraftButton,
                  !canSaveDraft ? styles.buttonDisabled : undefined,
                ]}
              >
                {savingDraft && !submitting ? (
                  <ActivityIndicator color="#0A7EA4" size="small" />
                ) : (
                  <ThemedText style={styles.saveDraftButtonText}>存草稿</ThemedText>
                )}
              </Pressable>
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
          </View>

          <View style={styles.tabBar}>
            <CreateEventTabButton
              active={activeTab === "meta"}
              label="信息"
              onPress={() => handleTabChange("meta")}
            />
            <CreateEventTabButton
              active={activeTab === "content"}
              label="正文"
              onPress={() => handleTabChange("content")}
            />
          </View>

          <View style={styles.tabBody}>
            <ScrollView
              keyboardShouldPersistTaps="handled"
              pointerEvents={activeTab === "meta" ? "auto" : "none"}
              accessibilityElementsHidden={activeTab !== "meta"}
              importantForAccessibility={
                activeTab === "meta" ? "auto" : "no-hide-descendants"
              }
              showsVerticalScrollIndicator={false}
              style={[
                styles.tabPane,
                styles.metaScroll,
                activeTab !== "meta" ? styles.inactiveTabPane : undefined,
              ]}
              contentContainerStyle={styles.metaContent}
            >
              <TextInput
                style={[styles.input, styles.titleInput]}
                placeholder="活动标题（最多 80 字）"
                placeholderTextColor="#8A94A6"
                value={title}
                maxLength={80}
                editable={draftOpened}
                onChangeText={setTitle}
              />
              <View style={styles.row}>
                <DateTimeField
                  label="开始时间"
                  value={startAt}
                  onPress={() => openDateTimePicker("start")}
                  onClear={() => clearDateTime("start")}
                  disabled={busy || !draftOpened}
                />
                <DateTimeField
                  label="结束时间"
                  value={endAt}
                  onPress={() => openDateTimePicker("end")}
                  onClear={() => clearDateTime("end")}
                  disabled={busy || !draftOpened}
                />
              </View>
              <View style={styles.row}>
                <TextInput
                  style={[styles.input, styles.rowInput]}
                  placeholder="地点名称"
                  placeholderTextColor="#8A94A6"
                  value={locationName}
                  editable={draftOpened}
                  onChangeText={setLocationName}
                />
                <TextInput
                  style={[styles.input, styles.rowInput]}
                  placeholder="人数上限"
                  placeholderTextColor="#8A94A6"
                  value={capacityText}
                  keyboardType="number-pad"
                  editable={draftOpened}
                  onChangeText={setCapacityText}
                />
              </View>
              <TextInput
                style={styles.input}
                placeholder="详细地址"
                placeholderTextColor="#8A94A6"
                value={locationAddress}
                editable={draftOpened}
                onChangeText={setLocationAddress}
              />
              <TextInput
                style={styles.input}
                placeholder="标签，用空格或逗号分隔"
                placeholderTextColor="#8A94A6"
                value={tagText}
                editable={draftOpened}
                onChangeText={setTagText}
              />
            </ScrollView>

            <View
              pointerEvents={activeTab === "content" && draftOpened ? "auto" : "none"}
              accessibilityElementsHidden={activeTab !== "content" || !draftOpened}
              importantForAccessibility={
                activeTab === "content" && draftOpened
                  ? "auto"
                  : "no-hide-descendants"
              }
              style={[
                styles.tabPane,
                styles.editorFrame,
                {
                  backgroundColor: editorBackground,
                  borderColor: editorBorderColor,
                },
                activeTab !== "content" ? styles.inactiveTabPane : undefined,
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
          <DraftEntryModal
            state={draftEntryState}
            drafts={existingDrafts}
            error={error}
            onSelect={openDraftForEditing}
            onNew={() => openDraftForEditing(null)}
            onRetry={() => void inspectDraftEntry()}
            onCancel={() => router.back()}
          />
        </ThemedView>
      </KeyboardAvoidingView>
    </SafeAreaView>
  );
}

function DraftEntryModal({
  state,
  drafts,
  error,
  onSelect,
  onNew,
  onRetry,
  onCancel,
}: {
  state: DraftEntryState;
  drafts: EventResp[];
  error: string;
  onSelect: (draft: EventResp) => void;
  onNew: () => void;
  onRetry: () => void;
  onCancel: () => void;
}) {
  const isLoading = state === "checking" || state === "opening";
  return (
    <Modal
      animationType="fade"
      transparent
      visible={state !== "editing"}
      statusBarTranslucent
      onRequestClose={onCancel}
    >
      <View style={styles.draftEntryBackdrop}>
        <View style={styles.draftEntryCard}>
          {isLoading ? (
            <View style={styles.draftEntryLoading}>
              <ActivityIndicator size="large" color="#0A7EA4" />
              <ThemedText type="defaultSemiBold" style={styles.draftEntryTitle}>
                {state === "checking" ? "正在检查草稿" : "正在准备编辑器"}
              </ThemedText>
              <ThemedText style={styles.draftEntryDescription}>
                请稍候，不会在编辑过程中自动保存。
              </ThemedText>
            </View>
          ) : state === "choosing" && drafts.length > 0 ? (
            <>
              <ThemedText type="subtitle" style={styles.draftEntryTitle}>
                选择活动草稿
              </ThemedText>
              <ThemedText style={styles.draftEntryDescription}>
                选择草稿只会把已保存内容填入编辑器，修改后点击“存草稿”才会覆盖保存。
              </ThemedText>
              <ScrollView
                style={styles.draftList}
                contentContainerStyle={styles.draftListContent}
                showsVerticalScrollIndicator={false}
              >
                {drafts.map((draft, index) => (
                  <Pressable
                    key={draft.event_id}
                    accessibilityRole="button"
                    accessibilityHint="把这份草稿内容填入活动编辑器"
                    onPress={() => onSelect(draft)}
                    style={styles.draftPreview}
                  >
                    <ThemedText
                      type="defaultSemiBold"
                      numberOfLines={2}
                      style={styles.draftPreviewTitle}
                    >
                      {draft.title.trim() || `草稿${index + 1}`}
                    </ThemedText>
                    <ThemedText style={styles.draftPreviewMeta}>
                      保存于 {formatDraftUpdatedAt(draft.updated_at)}
                    </ThemedText>
                  </Pressable>
                ))}
              </ScrollView>
              <Pressable
                accessibilityRole="button"
                accessibilityHint="打开空白活动编辑器，不修改已有草稿"
                onPress={onNew}
                style={[styles.draftEntryButton, styles.draftContinueButton]}
              >
                <ThemedText style={styles.draftContinueButtonText}>
                  +新建
                </ThemedText>
              </Pressable>
              <Pressable
                accessibilityRole="button"
                onPress={onCancel}
                style={styles.draftCancelButton}
              >
                <ThemedText style={styles.draftCancelButtonText}>取消</ThemedText>
              </Pressable>
            </>
          ) : (
            <>
              <ThemedText type="subtitle" style={styles.draftEntryTitle}>
                草稿暂时无法打开
              </ThemedText>
              <ThemedText style={[styles.draftEntryDescription, styles.errorText]}>
                {error || "请检查网络后重试"}
              </ThemedText>
              <Pressable
                accessibilityRole="button"
                onPress={onRetry}
                style={[styles.draftEntryButton, styles.draftContinueButton]}
              >
                <ThemedText style={styles.draftContinueButtonText}>重试</ThemedText>
              </Pressable>
              <Pressable
                accessibilityRole="button"
                onPress={onCancel}
                style={styles.draftCancelButton}
              >
                <ThemedText style={styles.draftCancelButtonText}>返回</ThemedText>
              </Pressable>
            </>
          )}
        </View>
      </View>
    </Modal>
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

function draftRequestHash(req: UpdateEventDraftRequest): string {
  // 草稿内容先序列化为固定结构，再计算轻量 hash；这里只用于本地去重，不承担安全校验。
  return hashString(JSON.stringify(req));
}

function hashString(value: string): string {
  let hashA = 0x811c9dc5;
  let hashB = 0x9e3779b9;
  for (let i = 0; i < value.length; i += 1) {
    const code = value.charCodeAt(i);
    hashA ^= code;
    hashA = Math.imul(hashA, 0x01000193);
    hashB ^= code + i;
    hashB = Math.imul(hashB, 0x85ebca6b);
  }
  return `${toHex32(hashA)}${toHex32(hashB)}`;
}

function toHex32(value: number): string {
  return (value >>> 0).toString(16).padStart(8, "0");
}

function eventToDraftUpdateRequest(
  event: EventResp,
  doc = normalizeContentDoc(event.content),
): UpdateEventDraftRequest {
  return {
    title: event.title.trim(),
    content: doc,
    start_at: dateToRfc3339(dateFromRfc3339(event.start_at)),
    end_at: dateToRfc3339(dateFromRfc3339(event.end_at)),
    location_name: event.location_name,
    location_address: event.location_address,
    capacity: event.capacity,
    tags: event.tags,
  };
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

function dateFromRfc3339(value: string | null): Date | null {
  if (!value) {
    return null;
  }
  const timestamp = Date.parse(value);
  if (!Number.isFinite(timestamp)) {
    console.warn("[create-event] invalid draft date ignored", { value });
    return null;
  }
  return normalizePickerDate(new Date(timestamp));
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

function formatDraftUpdatedAt(value: string): string {
  const timestamp = Date.parse(value);
  if (!Number.isFinite(timestamp)) {
    console.warn("[create-event] invalid draft updated time", { value });
    return "未知时间";
  }
  return formatLocalDateTime(new Date(timestamp));
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
  headerActions: {
    flexDirection: "row",
    alignItems: "center",
    gap: 6,
  },
  saveDraftButton: {
    minWidth: 68,
    height: 40,
    paddingHorizontal: 10,
    borderRadius: 10,
    borderWidth: 1,
    borderColor: "#0A7EA4",
    alignItems: "center",
    justifyContent: "center",
    backgroundColor: "#FFFFFF",
  },
  saveDraftButtonText: {
    color: "#0A7EA4",
    fontWeight: "700",
  },
  publishButton: {
    width: 62,
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
    position: "relative",
  },
  tabPane: {
    ...StyleSheet.absoluteFillObject,
  },
  inactiveTabPane: {
    opacity: 0,
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
  draftEntryBackdrop: {
    flex: 1,
    paddingHorizontal: 24,
    alignItems: "center",
    justifyContent: "center",
    backgroundColor: "rgba(17, 24, 28, 0.52)",
  },
  draftEntryCard: {
    width: "100%",
    maxWidth: 420,
    padding: 20,
    borderRadius: 18,
    backgroundColor: "#FFFFFF",
    gap: 12,
  },
  draftEntryLoading: {
    minHeight: 180,
    alignItems: "center",
    justifyContent: "center",
    gap: 12,
  },
  draftEntryTitle: {
    color: "#11181C",
    textAlign: "center",
  },
  draftEntryDescription: {
    color: "#687076",
    fontSize: 14,
    lineHeight: 21,
    textAlign: "center",
  },
  draftList: {
    maxHeight: 320,
  },
  draftListContent: {
    gap: 8,
    paddingVertical: 4,
  },
  draftPreview: {
    padding: 14,
    borderRadius: 12,
    borderWidth: 1,
    borderColor: "#D9E0EA",
    backgroundColor: "#F6F8FB",
    gap: 5,
  },
  draftPreviewTitle: {
    color: "#11181C",
  },
  draftPreviewMeta: {
    color: "#687076",
    fontSize: 12,
    lineHeight: 17,
  },
  draftEntryButton: {
    minHeight: 46,
    paddingHorizontal: 16,
    borderRadius: 10,
    alignItems: "center",
    justifyContent: "center",
  },
  draftContinueButton: {
    backgroundColor: "#0A7EA4",
  },
  draftContinueButtonText: {
    color: "#FFFFFF",
    fontWeight: "700",
  },
  draftCancelButton: {
    minHeight: 38,
    alignItems: "center",
    justifyContent: "center",
  },
  draftCancelButtonText: {
    color: "#687076",
    fontWeight: "600",
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
