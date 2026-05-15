import { router } from "expo-router";
import { useMemo, useState } from "react";
import {
  KeyboardAvoidingView,
  Platform,
  Pressable,
  ScrollView,
  StyleSheet,
  TextInput,
  View,
  type NativeSyntheticEvent,
  type TextInputSelectionChangeEventData,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";

import { ThemedText } from "@/components/themed-text";
import { ThemedView } from "@/components/themed-view";
import type {
  CreateEventRequest,
  EventContentBlock,
  EventContentDoc,
  EventFeatureBlock,
  EventTextColor,
  EventTextMark,
} from "@/lib/dto";
import { createEvent } from "@/lib/event-api";

type TextBlockType = "paragraph" | "heading" | "quote";
type DraftTextColor = "default" | EventTextColor;
type FeatureType = "schedule" | "location" | "notice" | "ticket";
type TextSelection = { start: number; end: number };

type DraftTextBlock = {
  id: string;
  type: TextBlockType;
  text: string;
  level: 1 | 2 | 3;
  bold: boolean;
  color: DraftTextColor;
};

type DraftImageBlock = {
  id: string;
  type: "image";
  assetId: string;
  width: string;
  height: string;
  caption: string;
};

type DraftDividerBlock = {
  id: string;
  type: "divider";
};

type DraftFeatureBlock = {
  id: string;
  type: "feature";
  featureType: FeatureType;
  title: string;
  body: string;
};

type DraftBlock =
  | DraftTextBlock
  | DraftImageBlock
  | DraftDividerBlock
  | DraftFeatureBlock;

const COLOR_OPTIONS: {
  label: string;
  value: DraftTextColor;
  color: string;
}[] = [
  { label: "默认", value: "default", color: "#11181C" },
  { label: "主题", value: "accent", color: "#0A7EA4" },
  { label: "提醒", value: "warning", color: "#C05621" },
  { label: "成功", value: "success", color: "#0F7C3B" },
  { label: "辅助", value: "muted", color: "#687076" },
];

const FEATURE_OPTIONS: { label: string; value: FeatureType }[] = [
  { label: "日程", value: "schedule" },
  { label: "地点", value: "location" },
  { label: "须知", value: "notice" },
  { label: "票务", value: "ticket" },
];

let draftIdSeed = 0;

export default function CreateEventScreen() {
  const initialBlock = useMemo(() => createTextBlock("paragraph"), []);
  const [title, setTitle] = useState("");
  const [blocks, setBlocks] = useState<DraftBlock[]>([initialBlock]);
  const [activeBlockId, setActiveBlockId] = useState(initialBlock.id);
  const [selections, setSelections] = useState<Record<string, TextSelection>>({});
  const [startAt, setStartAt] = useState("");
  const [locationName, setLocationName] = useState("");
  const [locationAddress, setLocationAddress] = useState("");
  const [capacity, setCapacity] = useState("");
  const [tagsText, setTagsText] = useState("");
  const [error, setError] = useState("");
  const [submitting, setSubmitting] = useState(false);

  const activeBlock = blocks.find((block) => block.id === activeBlockId);
  const canSubmit = useMemo(
    () => title.trim().length > 0 && blocks.some(isPublishableBlock),
    [blocks, title],
  );

  const updateBlock = (id: string, patch: Partial<DraftBlock>) => {
    setBlocks((prev) =>
      prev.map((block) =>
        block.id === id ? ({ ...block, ...patch } as DraftBlock) : block,
      ),
    );
  };

  const insertBlock = (block: DraftBlock) => {
    setBlocks((prev) => insertBlockAtCursor(prev, block, activeBlockId, selections));
    setActiveBlockId(block.id);
  };

  const removeBlock = (id: string) => {
    setBlocks((prev) => {
      if (prev.length === 1) {
        const replacement = createTextBlock("paragraph");
        setActiveBlockId(replacement.id);
        return [replacement];
      }

      const index = prev.findIndex((block) => block.id === id);
      const next = prev.filter((block) => block.id !== id);
      const nextActive = next[Math.max(0, Math.min(index, next.length - 1))];
      setActiveBlockId(nextActive.id);
      return next;
    });
  };

  const moveBlock = (id: string, direction: -1 | 1) => {
    setBlocks((prev) => {
      const index = prev.findIndex((block) => block.id === id);
      const target = index + direction;
      if (index < 0 || target < 0 || target >= prev.length) {
        return prev;
      }
      const next = [...prev];
      const [item] = next.splice(index, 1);
      next.splice(target, 0, item);
      return next;
    });
  };

  const handleTextSelection = (
    id: string,
    event: NativeSyntheticEvent<TextInputSelectionChangeEventData>,
  ) => {
    setSelections((prev) => ({
      ...prev,
      [id]: event.nativeEvent.selection,
    }));
  };

  const handleSubmit = async () => {
    if (!canSubmit || submitting) {
      return;
    }

    setSubmitting(true);
    setError("");
    try {
      const parsedCapacity = capacity.trim()
        ? Number.parseInt(capacity, 10)
        : null;
      if (
        parsedCapacity !== null &&
        (!Number.isFinite(parsedCapacity) || parsedCapacity <= 0)
      ) {
        setError("人数上限不正确");
        return;
      }

      const req: CreateEventRequest = {
        title: title.trim(),
        content: buildContentDoc(blocks),
        start_at: startAt.trim() || null,
        end_at: null,
        location_name: locationName.trim() || null,
        location_address: locationAddress.trim() || null,
        capacity: parsedCapacity,
        tags: parseTags(tagsText),
      };
      await createEvent(req);
      router.replace("/(tabs)");
    } catch (e) {
      setError(e instanceof Error ? e.message : "活动发布失败");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <SafeAreaView style={styles.safeArea} edges={["top", "left", "right"]}>
      <KeyboardAvoidingView
        behavior={Platform.OS === "ios" ? "padding" : undefined}
        style={styles.flex}
      >
        <ThemedView style={styles.container}>
          <View style={styles.header}>
            <Pressable onPress={() => router.back()} style={styles.navButton}>
              <ThemedText style={styles.navButtonText}>取消</ThemedText>
            </Pressable>
            <ThemedText type="subtitle">发布活动</ThemedText>
            <Pressable
              onPress={handleSubmit}
              disabled={!canSubmit || submitting}
              style={[
                styles.navButton,
                styles.submitButton,
                !canSubmit || submitting ? styles.buttonDisabled : undefined,
              ]}
            >
              <ThemedText style={styles.submitButtonText}>
                {submitting ? "发布中" : "发布"}
              </ThemedText>
            </Pressable>
          </View>

          <View style={styles.toolbar}>
            <ToolbarButton
              label="正文"
              onPress={() => insertBlock(createTextBlock("paragraph"))}
            />
            <ToolbarButton
              label="小标题"
              onPress={() => insertBlock(createTextBlock("heading"))}
            />
            <ToolbarButton
              label="图片"
              onPress={() => insertBlock(createImageBlock())}
            />
            <ToolbarButton
              label="引用"
              onPress={() => insertBlock(createTextBlock("quote"))}
            />
            <ToolbarButton label="分割" onPress={() => insertBlock(createDividerBlock())} />
            <ToolbarButton
              label="功能块"
              onPress={() => insertBlock(createFeatureBlock())}
            />
          </View>

          {isTextBlock(activeBlock) ? (
            <View style={styles.textToolbar}>
              <ToolbarButton
                label="B"
                active={activeBlock.bold}
                onPress={() => updateBlock(activeBlock.id, { bold: !activeBlock.bold })}
              />
              {activeBlock.type === "heading" ? (
                <>
                  {[1, 2, 3].map((level) => (
                    <ToolbarButton
                      key={level}
                      label={`H${level}`}
                      active={activeBlock.level === level}
                      onPress={() =>
                        updateBlock(activeBlock.id, { level: level as 1 | 2 | 3 })
                      }
                    />
                  ))}
                </>
              ) : null}
              {COLOR_OPTIONS.map((option) => (
                <Pressable
                  key={option.value}
                  onPress={() => updateBlock(activeBlock.id, { color: option.value })}
                  style={[
                    styles.colorButton,
                    { borderColor: option.color },
                    activeBlock.color === option.value ? styles.colorButtonActive : undefined,
                  ]}
                >
                  <View style={[styles.colorDot, { backgroundColor: option.color }]} />
                  <ThemedText style={styles.colorLabel}>{option.label}</ThemedText>
                </Pressable>
              ))}
            </View>
          ) : null}

          <ScrollView
            keyboardShouldPersistTaps="handled"
            showsVerticalScrollIndicator={false}
            contentContainerStyle={styles.content}
          >
            <TextInput
              style={[styles.input, styles.titleInput]}
              placeholder="活动标题"
              placeholderTextColor="#8A94A6"
              value={title}
              onChangeText={setTitle}
              maxLength={80}
            />

            {blocks.map((block, index) => (
              <DraftBlockEditor
                key={block.id}
                block={block}
                index={index}
                total={blocks.length}
                active={activeBlockId === block.id}
                onFocus={() => setActiveBlockId(block.id)}
                onChange={(patch) => updateBlock(block.id, patch)}
                onRemove={() => removeBlock(block.id)}
                onMoveUp={() => moveBlock(block.id, -1)}
                onMoveDown={() => moveBlock(block.id, 1)}
                onSelectionChange={(event) => handleTextSelection(block.id, event)}
              />
            ))}

            <ThemedView style={styles.section} lightColor="#F5F8FC" darkColor="#1E252C">
              <ThemedText type="defaultSemiBold">活动信息</ThemedText>
              <TextInput
                style={styles.input}
                placeholder="开始时间 2026-05-20T19:00:00+08:00"
                placeholderTextColor="#8A94A6"
                value={startAt}
                onChangeText={setStartAt}
                autoCapitalize="none"
              />
              <TextInput
                style={styles.input}
                placeholder="地点名称"
                placeholderTextColor="#8A94A6"
                value={locationName}
                onChangeText={setLocationName}
              />
              <TextInput
                style={styles.input}
                placeholder="详细地址"
                placeholderTextColor="#8A94A6"
                value={locationAddress}
                onChangeText={setLocationAddress}
              />
              <TextInput
                style={styles.input}
                placeholder="人数上限"
                placeholderTextColor="#8A94A6"
                keyboardType="number-pad"
                value={capacity}
                onChangeText={setCapacity}
              />
              <TextInput
                style={styles.input}
                placeholder="标签，用空格或逗号分隔"
                placeholderTextColor="#8A94A6"
                value={tagsText}
                onChangeText={setTagsText}
              />
            </ThemedView>

            {error ? <ThemedText style={styles.errorText}>{error}</ThemedText> : null}
          </ScrollView>
        </ThemedView>
      </KeyboardAvoidingView>
    </SafeAreaView>
  );
}

function DraftBlockEditor({
  block,
  index,
  total,
  active,
  onFocus,
  onChange,
  onRemove,
  onMoveUp,
  onMoveDown,
  onSelectionChange,
}: {
  block: DraftBlock;
  index: number;
  total: number;
  active: boolean;
  onFocus: () => void;
  onChange: (patch: Partial<DraftBlock>) => void;
  onRemove: () => void;
  onMoveUp: () => void;
  onMoveDown: () => void;
  onSelectionChange: (
    event: NativeSyntheticEvent<TextInputSelectionChangeEventData>,
  ) => void;
}) {
  return (
    <ThemedView
      style={[styles.block, active ? styles.blockActive : undefined]}
      lightColor="#F5F8FC"
      darkColor="#1E252C"
    >
      <View style={styles.blockHeader}>
        <ThemedText style={styles.blockTypeLabel}>{blockLabel(block)}</ThemedText>
        <View style={styles.blockActions}>
          <ActionText label="上移" disabled={index === 0} onPress={onMoveUp} />
          <ActionText label="下移" disabled={index === total - 1} onPress={onMoveDown} />
          <ActionText label="删除" destructive onPress={onRemove} />
        </View>
      </View>

      {isTextBlock(block) ? (
        <TextInput
          style={[styles.input, styles.blockInput, textBlockStyle(block)]}
          placeholder={textPlaceholder(block)}
          placeholderTextColor="#8A94A6"
          multiline
          textAlignVertical="top"
          value={block.text}
          onFocus={onFocus}
          onSelectionChange={onSelectionChange}
          onChangeText={(text) => onChange({ text } as Partial<DraftBlock>)}
        />
      ) : null}

      {block.type === "image" ? (
        <View style={styles.blockFields}>
          <TextInput
            style={styles.input}
            placeholder="图片资源 ID"
            placeholderTextColor="#8A94A6"
            value={block.assetId}
            onFocus={onFocus}
            onChangeText={(assetId) => onChange({ assetId } as Partial<DraftBlock>)}
          />
          <View style={styles.row}>
            <TextInput
              style={[styles.input, styles.flexInput]}
              placeholder="宽"
              placeholderTextColor="#8A94A6"
              keyboardType="number-pad"
              value={block.width}
              onFocus={onFocus}
              onChangeText={(width) => onChange({ width } as Partial<DraftBlock>)}
            />
            <TextInput
              style={[styles.input, styles.flexInput]}
              placeholder="高"
              placeholderTextColor="#8A94A6"
              keyboardType="number-pad"
              value={block.height}
              onFocus={onFocus}
              onChangeText={(height) => onChange({ height } as Partial<DraftBlock>)}
            />
          </View>
          <TextInput
            style={styles.input}
            placeholder="图片说明"
            placeholderTextColor="#8A94A6"
            value={block.caption}
            onFocus={onFocus}
            onChangeText={(caption) => onChange({ caption } as Partial<DraftBlock>)}
          />
        </View>
      ) : null}

      {block.type === "divider" ? (
        <Pressable onPress={onFocus} style={styles.dividerPreview}>
          <View style={styles.dividerLine} />
        </Pressable>
      ) : null}

      {block.type === "feature" ? (
        <View style={styles.blockFields}>
          <View style={styles.featureTabs}>
            {FEATURE_OPTIONS.map((option) => (
              <ToolbarButton
                key={option.value}
                label={option.label}
                active={block.featureType === option.value}
                onPress={() =>
                  onChange({ featureType: option.value } as Partial<DraftBlock>)
                }
              />
            ))}
          </View>
          <TextInput
            style={styles.input}
            placeholder="功能块标题"
            placeholderTextColor="#8A94A6"
            value={block.title}
            onFocus={onFocus}
            onChangeText={(title) => onChange({ title } as Partial<DraftBlock>)}
          />
          <TextInput
            style={[styles.input, styles.featureBodyInput]}
            placeholder={featureBodyPlaceholder(block.featureType)}
            placeholderTextColor="#8A94A6"
            value={block.body}
            multiline={block.featureType !== "location"}
            textAlignVertical="top"
            onFocus={onFocus}
            onChangeText={(body) => onChange({ body } as Partial<DraftBlock>)}
          />
        </View>
      ) : null}
    </ThemedView>
  );
}

function ToolbarButton({
  label,
  active,
  onPress,
}: {
  label: string;
  active?: boolean;
  onPress: () => void;
}) {
  return (
    <Pressable
      onPress={onPress}
      style={[styles.toolbarButton, active ? styles.toolbarButtonActive : undefined]}
    >
      <ThemedText
        style={[styles.toolbarButtonText, active ? styles.toolbarButtonTextActive : undefined]}
      >
        {label}
      </ThemedText>
    </Pressable>
  );
}

function ActionText({
  label,
  disabled,
  destructive,
  onPress,
}: {
  label: string;
  disabled?: boolean;
  destructive?: boolean;
  onPress: () => void;
}) {
  return (
    <Pressable onPress={onPress} disabled={disabled}>
      <ThemedText
        style={[
          styles.actionText,
          destructive ? styles.actionTextDanger : undefined,
          disabled ? styles.actionTextDisabled : undefined,
        ]}
      >
        {label}
      </ThemedText>
    </Pressable>
  );
}

function createTextBlock(type: TextBlockType): DraftTextBlock {
  return {
    id: nextBlockId(type),
    type,
    text: "",
    level: type === "heading" ? 2 : 1,
    bold: type === "heading",
    color: "default",
  };
}

function createImageBlock(): DraftImageBlock {
  return {
    id: nextBlockId("image"),
    type: "image",
    assetId: "",
    width: "1080",
    height: "1080",
    caption: "",
  };
}

function createDividerBlock(): DraftDividerBlock {
  return {
    id: nextBlockId("divider"),
    type: "divider",
  };
}

function createFeatureBlock(): DraftFeatureBlock {
  return {
    id: nextBlockId("feature"),
    type: "feature",
    featureType: "notice",
    title: "",
    body: "",
  };
}

function nextBlockId(prefix: string): string {
  draftIdSeed += 1;
  return `${prefix}-${Date.now()}-${draftIdSeed}`;
}

function insertBlockAtCursor(
  blocks: DraftBlock[],
  block: DraftBlock,
  activeBlockId: string,
  selections: Record<string, TextSelection>,
): DraftBlock[] {
  const index = blocks.findIndex((item) => item.id === activeBlockId);
  if (index < 0) {
    return [...blocks, block];
  }

  const activeBlock = blocks[index];
  if (!isTextBlock(activeBlock)) {
    const next = [...blocks];
    next.splice(index + 1, 0, block);
    return next;
  }

  const selection = selections[activeBlock.id] ?? {
    start: activeBlock.text.length,
    end: activeBlock.text.length,
  };
  const start = Math.max(0, Math.min(selection.start, activeBlock.text.length));
  const end = Math.max(start, Math.min(selection.end, activeBlock.text.length));
  const before = activeBlock.text.slice(0, start);
  const after = activeBlock.text.slice(end);
  const next = [...blocks];

  if (start === 0) {
    const afterBlock =
      after.length > 0
        ? { ...activeBlock, id: nextBlockId(activeBlock.type), text: after }
        : createTextBlock("paragraph");
    next.splice(index, 1, block, afterBlock);
    return next;
  }

  if (end < activeBlock.text.length) {
    next.splice(
      index,
      1,
      { ...activeBlock, text: before },
      block,
      { ...activeBlock, id: nextBlockId(activeBlock.type), text: after },
    );
    return next;
  }

  next.splice(index, 1, { ...activeBlock, text: before }, block);
  return next;
}

function buildContentDoc(blocks: DraftBlock[]): EventContentDoc {
  return {
    schema_version: 2,
    blocks: blocks.flatMap(toContentBlock),
  };
}

function toContentBlock(block: DraftBlock): EventContentBlock[] {
  if (isTextBlock(block)) {
    const text = block.text.trim();
    if (!text) {
      return [];
    }
    const children = [
      {
        type: "text" as const,
        text,
        marks: textMarks(block),
      },
    ];
    if (block.type === "heading") {
      return [
        {
          type: "heading",
          id: block.id,
          level: block.level,
          children,
        },
      ];
    }
    if (block.type === "quote") {
      return [
        {
          type: "quote",
          id: block.id,
          children,
        },
      ];
    }
    return [
      {
        type: "paragraph",
        id: block.id,
        children,
      },
    ];
  }

  if (block.type === "image") {
    const width = Number.parseInt(block.width, 10);
    const height = Number.parseInt(block.height, 10);
    if (!block.assetId.trim() || !Number.isFinite(width) || !Number.isFinite(height)) {
      return [];
    }
    return [
      {
        type: "image",
        id: block.id,
        item: {
          asset_id: block.assetId.trim(),
          width,
          height,
          alt: null,
        },
        caption: block.caption.trim() || null,
      },
    ];
  }

  if (block.type === "divider") {
    return [{ type: "divider", id: block.id }];
  }

  const feature = toFeatureBlock(block);
  return feature ? [{ type: "feature", id: block.id, feature }] : [];
}

function textMarks(block: DraftTextBlock): EventTextMark[] {
  const marks: EventTextMark[] = [];
  if (block.bold) {
    marks.push({ type: "bold" });
  }
  if (block.color !== "default") {
    marks.push({ type: "color", value: block.color });
  }
  return marks;
}

function toFeatureBlock(block: DraftFeatureBlock): EventFeatureBlock | null {
  const title = block.title.trim();
  const body = block.body.trim();
  if (!title || !body) {
    return null;
  }
  if (block.featureType === "location") {
    return { feature_type: "location", title, address: body };
  }
  if (block.featureType === "ticket") {
    return { feature_type: "ticket", title, description: body };
  }
  const items = body
    .split(/\n+/)
    .map((item) => item.trim())
    .filter(Boolean);
  if (!items.length) {
    return null;
  }
  return block.featureType === "schedule"
    ? { feature_type: "schedule", title, items }
    : { feature_type: "notice", title, items };
}

function parseTags(value: string): string[] {
  return value
    .split(/[\s,，]+/)
    .map((tag) => tag.trim().replace(/^#/, ""))
    .filter(Boolean);
}

function isPublishableBlock(block: DraftBlock): boolean {
  if (isTextBlock(block)) {
    return block.text.trim().length > 0;
  }
  if (block.type === "image") {
    return block.assetId.trim().length > 0;
  }
  if (block.type === "feature") {
    return block.title.trim().length > 0 && block.body.trim().length > 0;
  }
  return false;
}

function isTextBlock(block: DraftBlock | undefined): block is DraftTextBlock {
  return (
    block?.type === "paragraph" ||
    block?.type === "heading" ||
    block?.type === "quote"
  );
}

function blockLabel(block: DraftBlock): string {
  switch (block.type) {
    case "heading":
      return "小标题";
    case "paragraph":
      return "正文";
    case "quote":
      return "引用";
    case "image":
      return "图片";
    case "divider":
      return "分割线";
    case "feature":
      return "功能块";
  }
}

function textPlaceholder(block: DraftTextBlock): string {
  if (block.type === "heading") {
    return "输入小标题";
  }
  if (block.type === "quote") {
    return "输入引用内容";
  }
  return "输入活动文案";
}

function featureBodyPlaceholder(type: FeatureType): string {
  if (type === "location") {
    return "地点地址";
  }
  if (type === "ticket") {
    return "票务或签到说明";
  }
  return "每行一条内容";
}

function textBlockStyle(block: DraftTextBlock) {
  const colorOption = COLOR_OPTIONS.find((option) => option.value === block.color);
  return {
    minHeight: block.type === "heading" ? 56 : block.type === "quote" ? 74 : 92,
    fontSize: block.type === "heading" ? headingFontSize(block.level) : 16,
    lineHeight: block.type === "heading" ? headingLineHeight(block.level) : 23,
    fontWeight: block.bold || block.type === "heading" ? "700" as const : "400" as const,
    color: colorOption?.color ?? "#11181C",
    borderLeftWidth: block.type === "quote" ? 4 : 1,
    borderLeftColor: block.type === "quote" ? "#0A7EA4" : "#CDD3DD",
  };
}

function headingFontSize(level: 1 | 2 | 3): number {
  return level === 1 ? 24 : level === 2 ? 20 : 18;
}

function headingLineHeight(level: 1 | 2 | 3): number {
  return level === 1 ? 30 : level === 2 ? 26 : 24;
}

const styles = StyleSheet.create({
  safeArea: {
    flex: 1,
  },
  flexInput: {
    flex: 1,
  },
  container: {
    flex: 1,
  },
  header: {
    height: 56,
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    paddingHorizontal: 16,
  },
  navButton: {
    minWidth: 64,
    height: 36,
    borderRadius: 10,
    alignItems: "center",
    justifyContent: "center",
  },
  navButtonText: {
    color: "#0A7EA4",
    fontWeight: "700",
  },
  submitButton: {
    backgroundColor: "#0A7EA4",
  },
  submitButtonText: {
    color: "#FFFFFF",
    fontWeight: "700",
  },
  buttonDisabled: {
    opacity: 0.55,
  },
  toolbar: {
    minHeight: 50,
    flexDirection: "row",
    alignItems: "center",
    gap: 8,
    paddingHorizontal: 16,
    paddingBottom: 8,
    flexWrap: "wrap",
  },
  textToolbar: {
    minHeight: 48,
    flexDirection: "row",
    alignItems: "center",
    gap: 8,
    paddingHorizontal: 16,
    paddingBottom: 8,
    flexWrap: "wrap",
  },
  toolbarButton: {
    height: 34,
    minWidth: 48,
    borderRadius: 9,
    borderWidth: 1,
    borderColor: "#C8D3DD",
    backgroundColor: "#FFFFFF",
    alignItems: "center",
    justifyContent: "center",
    paddingHorizontal: 10,
  },
  toolbarButtonActive: {
    borderColor: "#0A7EA4",
    backgroundColor: "#E7F5FA",
  },
  toolbarButtonText: {
    fontSize: 13,
    lineHeight: 18,
    fontWeight: "700",
  },
  toolbarButtonTextActive: {
    color: "#0A7EA4",
  },
  colorButton: {
    height: 34,
    borderRadius: 9,
    borderWidth: 1,
    backgroundColor: "#FFFFFF",
    flexDirection: "row",
    alignItems: "center",
    gap: 5,
    paddingHorizontal: 8,
  },
  colorButtonActive: {
    backgroundColor: "#F2FBFF",
  },
  colorDot: {
    width: 10,
    height: 10,
    borderRadius: 5,
  },
  colorLabel: {
    fontSize: 12,
    lineHeight: 16,
    fontWeight: "600",
  },
  content: {
    paddingHorizontal: 16,
    paddingBottom: 32,
    gap: 12,
  },
  input: {
    minHeight: 46,
    borderRadius: 10,
    borderWidth: 1,
    borderColor: "#CDD3DD",
    paddingHorizontal: 12,
    paddingVertical: 10,
    fontSize: 15,
    color: "#11181C",
    backgroundColor: "#FFFFFF",
  },
  titleInput: {
    fontSize: 22,
    lineHeight: 28,
    fontWeight: "700",
  },
  block: {
    borderRadius: 12,
    padding: 12,
    gap: 10,
    borderWidth: 1,
    borderColor: "transparent",
  },
  blockActive: {
    borderColor: "#0A7EA4",
  },
  blockHeader: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    gap: 10,
  },
  blockTypeLabel: {
    fontSize: 13,
    lineHeight: 18,
    fontWeight: "700",
    opacity: 0.72,
  },
  blockActions: {
    flexDirection: "row",
    alignItems: "center",
    gap: 10,
  },
  actionText: {
    color: "#0A7EA4",
    fontSize: 12,
    lineHeight: 16,
    fontWeight: "700",
  },
  actionTextDanger: {
    color: "#D64545",
  },
  actionTextDisabled: {
    opacity: 0.35,
  },
  blockInput: {
    textAlignVertical: "top",
  },
  blockFields: {
    gap: 8,
  },
  row: {
    flexDirection: "row",
    gap: 8,
  },
  flex: {
    flex: 1,
  },
  dividerPreview: {
    height: 44,
    justifyContent: "center",
  },
  dividerLine: {
    height: 1,
    backgroundColor: "#B9C0CC",
  },
  featureTabs: {
    flexDirection: "row",
    gap: 8,
    flexWrap: "wrap",
  },
  featureBodyInput: {
    minHeight: 86,
    textAlignVertical: "top",
  },
  section: {
    borderRadius: 12,
    padding: 12,
    gap: 10,
  },
  errorText: {
    color: "#D64545",
  },
});
