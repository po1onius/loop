import { Image, type ImageSource } from "expo-image";
import { Linking, StyleSheet, Text, View } from "react-native";

import { ThemedText } from "@/components/themed-text";
import type {
  EventContentBlock,
  EventContentDoc,
  EventContentImage,
  EventFeatureBlock,
  EventInlineNode,
  EventTextColor,
  EventTextMark,
} from "@/lib/dto";

export type EventImageSource = Pick<ImageSource, "headers" | "uri">;

type EventContentViewProps = {
  content: EventContentDoc;
  imageErrors?: Record<string, string | undefined>;
  imageSources: Record<string, EventImageSource | undefined>;
};

const TEXT_COLOR_BY_MARK: Record<EventTextColor, string> = {
  accent: "#0A7EA4",
  muted: "#687076",
  success: "#1C7C54",
  warning: "#C46B00",
};

export function EventContentView({
  content,
  imageErrors = {},
  imageSources,
}: EventContentViewProps) {
  if (!content.blocks.length) {
    return <ThemedText style={styles.emptyText}>暂无正文内容</ThemedText>;
  }

  return (
    <View style={styles.root}>
      {content.blocks.map((block) =>
        renderContentBlock(block, imageSources, imageErrors),
      )}
    </View>
  );
}

function renderContentBlock(
  block: EventContentBlock,
  imageSources: Record<string, EventImageSource | undefined>,
  imageErrors: Record<string, string | undefined>,
) {
  // 后端内容模型是块级结构，前端逐块渲染，避免把富文本 HTML 直接注入原生页面。
  switch (block.type) {
    case "heading":
      return (
        <ThemedText
          key={block.id}
          style={block.level <= 2 ? styles.headingLarge : styles.headingSmall}
        >
          {renderInlineNodes(block.children)}
        </ThemedText>
      );
    case "paragraph":
      return (
        <ThemedText key={block.id} style={styles.paragraph}>
          {renderInlineNodes(block.children)}
        </ThemedText>
      );
    case "quote":
      return (
        <View key={block.id} style={styles.quoteBlock}>
          <ThemedText style={styles.quoteText}>
            {renderInlineNodes(block.children)}
          </ThemedText>
        </View>
      );
    case "image":
      return (
        <EventImageBlock
          key={block.id}
          caption={block.caption}
          image={block.item}
          imageError={imageErrors[block.item.asset_id]}
          imageSource={imageSources[block.item.asset_id]}
        />
      );
    case "image_grid":
      return (
        <View key={block.id} style={styles.imageGrid}>
          {block.items.map((item) => (
            <EventImageBlock
              key={item.asset_id}
              compact
              image={item}
              imageError={imageErrors[item.asset_id]}
              imageSource={imageSources[item.asset_id]}
            />
          ))}
        </View>
      );
    case "divider":
      return <View key={block.id} style={styles.divider} />;
    case "feature":
      return <FeatureBlock key={block.id} feature={block.feature} />;
    default:
      return null;
  }
}

function EventImageBlock({
  caption,
  compact = false,
  image,
  imageError,
  imageSource,
}: {
  caption?: string | null;
  compact?: boolean;
  image: EventContentImage;
  imageError?: string | undefined;
  imageSource?: EventImageSource | undefined;
}) {
  const aspectRatio = normalizeAspectRatio(image.width, image.height);
  return (
    <View style={[styles.imageBlock, compact ? styles.imageBlockCompact : undefined]}>
      {imageSource?.uri ? (
        <Image
          accessibilityLabel={image.alt ?? caption ?? "活动图片"}
          contentFit="cover"
          source={imageSource}
          style={[styles.image, { aspectRatio }]}
          transition={160}
        />
      ) : (
        <View style={[styles.imagePlaceholder, { aspectRatio }]}>
          <ThemedText style={styles.imagePlaceholderText}>
            {imageError ? "图片加载失败" : "图片加载中..."}
          </ThemedText>
        </View>
      )}
      {caption ? <ThemedText style={styles.caption}>{caption}</ThemedText> : null}
    </View>
  );
}

function FeatureBlock({ feature }: { feature: EventFeatureBlock }) {
  if (feature.feature_type === "location") {
    return (
      <View style={styles.featureBlock}>
        <ThemedText type="defaultSemiBold">{feature.title}</ThemedText>
        {feature.address ? (
          <ThemedText style={styles.featureText}>{feature.address}</ThemedText>
        ) : null}
      </View>
    );
  }

  if (feature.feature_type === "ticket") {
    return (
      <View style={styles.featureBlock}>
        <ThemedText type="defaultSemiBold">{feature.title}</ThemedText>
        <ThemedText style={styles.featureText}>{feature.description}</ThemedText>
      </View>
    );
  }

  return (
    <View style={styles.featureBlock}>
      <ThemedText type="defaultSemiBold">{feature.title}</ThemedText>
      {feature.items.map((item, index) => (
        <View key={`${feature.feature_type}_${index}`} style={styles.featureRow}>
          <View style={styles.featureDot} />
          <ThemedText style={styles.featureText}>{item}</ThemedText>
        </View>
      ))}
    </View>
  );
}

function renderInlineNodes(nodes: EventInlineNode[]) {
  // 行内节点保留链接、话题和文字样式，保证详情页呈现与创建页导出的语义一致。
  return nodes.map((node, index) => {
    if (node.type === "link") {
      return (
        <Text
          key={`${node.type}_${index}`}
          onPress={() => openInlineLink(node.url)}
          style={styles.linkText}
        >
          {node.text}
        </Text>
      );
    }

    if (node.type === "mention") {
      return (
        <Text key={`${node.type}_${index}`} style={styles.mentionText}>
          @{node.label}
        </Text>
      );
    }

    if (node.type === "hashtag") {
      return (
        <Text key={`${node.type}_${index}`} style={styles.hashtagText}>
          {node.text.startsWith("#") ? node.text : `#${node.text}`}
        </Text>
      );
    }

    return (
      <Text key={`${node.type}_${index}`} style={styleForTextMarks(node.marks)}>
        {node.text}
      </Text>
    );
  });
}

function styleForTextMarks(marks: EventTextMark[]) {
  return marks.map((mark) => {
    switch (mark.type) {
      case "bold":
        return styles.boldText;
      case "italic":
        return styles.italicText;
      case "underline":
        return styles.underlineText;
      case "color":
        return { color: TEXT_COLOR_BY_MARK[mark.value] };
      default:
        return undefined;
    }
  });
}

function openInlineLink(url: string) {
  console.info("[event-content] opening inline link", { url });
  Linking.openURL(url).catch((error) => {
    console.warn("[event-content] open inline link failed", {
      url,
      reason: error instanceof Error ? error.message : String(error),
    });
  });
}

function normalizeAspectRatio(width: number, height: number): number {
  if (width <= 0 || height <= 0) {
    return 16 / 9;
  }
  return Math.min(Math.max(width / height, 0.56), 2.4);
}

export function collectEventImageAssetIds(content: EventContentDoc): string[] {
  // 图片块只保存媒体资产 ID；这里集中去重，详情页再并发换取真实下载地址。
  const assetIds = new Set<string>();
  for (const block of content.blocks) {
    if (block.type === "image") {
      assetIds.add(block.item.asset_id);
    }
    if (block.type === "image_grid") {
      block.items.forEach((item) => assetIds.add(item.asset_id));
    }
  }
  return Array.from(assetIds).filter(Boolean);
}

export function toImageHeaders(
  headers: { name: string; value: string }[],
): Record<string, string> | undefined {
  const result: Record<string, string> = {};
  for (const header of headers) {
    if (!header.name.trim()) {
      continue;
    }
    result[header.name] = header.value;
  }
  return Object.keys(result).length ? result : undefined;
}

const styles = StyleSheet.create({
  root: {
    gap: 14,
  },
  emptyText: {
    color: "#687076",
  },
  headingLarge: {
    marginTop: 6,
    fontSize: 24,
    lineHeight: 32,
    fontWeight: "800",
  },
  headingSmall: {
    marginTop: 4,
    fontSize: 20,
    lineHeight: 28,
    fontWeight: "800",
  },
  paragraph: {
    fontSize: 16,
    lineHeight: 27,
  },
  quoteBlock: {
    borderLeftWidth: 4,
    borderLeftColor: "#0A7EA4",
    borderRadius: 10,
    paddingHorizontal: 12,
    paddingVertical: 10,
    backgroundColor: "#EEF8FC",
  },
  quoteText: {
    fontSize: 16,
    lineHeight: 26,
  },
  linkText: {
    color: "#0A7EA4",
    textDecorationLine: "underline",
  },
  mentionText: {
    color: "#0A7EA4",
    fontWeight: "700",
  },
  hashtagText: {
    color: "#0A7EA4",
    fontWeight: "700",
  },
  boldText: {
    fontWeight: "700",
  },
  italicText: {
    fontStyle: "italic",
  },
  underlineText: {
    textDecorationLine: "underline",
  },
  imageBlock: {
    overflow: "hidden",
    borderRadius: 12,
    borderWidth: 1,
    borderColor: "#D9E0EA",
    backgroundColor: "#F3F6FA",
  },
  imageBlockCompact: {
    flex: 1,
    minWidth: 140,
  },
  image: {
    width: "100%",
    backgroundColor: "#E7ECF2",
  },
  imagePlaceholder: {
    width: "100%",
    alignItems: "center",
    justifyContent: "center",
    backgroundColor: "#E7ECF2",
  },
  imagePlaceholderText: {
    fontSize: 13,
    lineHeight: 18,
    color: "#687076",
  },
  caption: {
    paddingHorizontal: 10,
    paddingVertical: 8,
    fontSize: 13,
    lineHeight: 18,
    color: "#687076",
  },
  imageGrid: {
    flexDirection: "row",
    flexWrap: "wrap",
    gap: 10,
  },
  divider: {
    height: 1,
    backgroundColor: "#D9E0EA",
  },
  featureBlock: {
    gap: 8,
    borderRadius: 12,
    borderWidth: 1,
    borderColor: "#D9E0EA",
    padding: 12,
    backgroundColor: "#F8FAFC",
  },
  featureRow: {
    flexDirection: "row",
    alignItems: "flex-start",
    gap: 8,
  },
  featureDot: {
    width: 6,
    height: 6,
    borderRadius: 3,
    marginTop: 9,
    backgroundColor: "#0A7EA4",
  },
  featureText: {
    flex: 1,
    fontSize: 15,
    lineHeight: 23,
    color: "#46515B",
  },
});
