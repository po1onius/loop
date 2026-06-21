import { useLocalSearchParams, router } from "expo-router";
import { useCallback, useEffect, useMemo, useState } from "react";
import {
  ActivityIndicator,
  Pressable,
  ScrollView,
  StyleSheet,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";

import {
  collectEventImageAssetIds,
  EventContentView,
  type EventImageSource,
  toImageHeaders,
} from "@/components/event-content-view";
import { ThemedText } from "@/components/themed-text";
import { ThemedView } from "@/components/themed-view";
import { IconSymbol } from "@/components/ui/icon-symbol";
import type { EventResp } from "@/lib/dto";
import { getEvent } from "@/lib/event-api";
import { getMediaDownloadUrl } from "@/lib/media-api";

type EventMetaItem = {
  id: string;
  label: string;
};

type EventImageLoadResult =
  | {
      assetId: string;
      source: EventImageSource;
    }
  | {
      assetId: string;
      error: string;
    };

export default function EventDetailScreen() {
  const params = useLocalSearchParams<{ eventId?: string | string[] }>();
  const eventId = normalizeRouteParam(params.eventId);
  const [event, setEvent] = useState<EventResp | null>(null);
  const [loadingEvent, setLoadingEvent] = useState(false);
  const [eventError, setEventError] = useState("");
  const [imageSources, setImageSources] = useState<
    Record<string, EventImageSource | undefined>
  >({});
  const [imageErrors, setImageErrors] = useState<
    Record<string, string | undefined>
  >({});

  const loadEvent = useCallback(async () => {
    if (!eventId) {
      setEventError("缺少活动 ID");
      setEvent(null);
      return;
    }

    setLoadingEvent(true);
    setEventError("");
    console.info("[event-detail] loading event", { eventId });
    try {
      const resp = await getEvent(eventId);
      setEvent(resp);
      console.info("[event-detail] event loaded", {
        eventId: resp.event_id,
        titleLength: resp.title.length,
        blockCount: resp.content.blocks.length,
      });
    } catch (error) {
      const reason = error instanceof Error ? error.message : "活动详情加载失败";
      console.warn("[event-detail] event load failed", { eventId, reason });
      setEvent(null);
      setEventError(reason);
    } finally {
      setLoadingEvent(false);
    }
  }, [eventId]);

  useEffect(() => {
    void loadEvent();
  }, [loadEvent]);

  useEffect(() => {
    if (!event) {
      setImageSources({});
      setImageErrors({});
      return;
    }

    const assetIds = collectEventImageAssetIds(event.content);
    setImageSources({});
    setImageErrors({});
    if (!assetIds.length) {
      return;
    }

    let cancelled = false;
    const currentEventId = event.event_id;

    // 正文图片只保存 asset_id，详情页按需换取短期下载 URL，避免把过期 URL 写入活动内容。
    async function loadEventImages() {
      console.info("[event-detail] loading event images", {
        eventId: currentEventId,
        assetCount: assetIds.length,
      });

      const results = await Promise.all(
        assetIds.map((assetId) => loadImageSource(assetId, currentEventId)),
      );
      if (cancelled) {
        return;
      }

      const nextSources: Record<string, EventImageSource | undefined> = {};
      const nextErrors: Record<string, string | undefined> = {};
      for (const result of results) {
        if ("source" in result) {
          nextSources[result.assetId] = result.source;
        } else {
          nextErrors[result.assetId] = result.error;
        }
      }
      setImageSources(nextSources);
      setImageErrors(nextErrors);
    }

    void loadEventImages();

    return () => {
      cancelled = true;
    };
  }, [event]);

  const metaItems = useMemo(() => (event ? buildMetaItems(event) : []), [event]);

  return (
    <SafeAreaView style={styles.safeArea} edges={["top", "left", "right"]}>
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
            <ThemedText type="subtitle">活动详情</ThemedText>
            <ThemedText numberOfLines={1} style={styles.headerMeta}>
              {event?.title ?? "查看完整活动内容"}
            </ThemedText>
          </View>
        </View>

        {loadingEvent ? (
          <View style={styles.centerState}>
            <ActivityIndicator color="#0A7EA4" />
            <ThemedText style={styles.stateText}>正在加载活动详情...</ThemedText>
          </View>
        ) : eventError ? (
          <View style={styles.centerState}>
            <ThemedText type="defaultSemiBold">活动详情加载失败</ThemedText>
            <ThemedText style={styles.stateText}>{eventError}</ThemedText>
            <Pressable
              accessibilityRole="button"
              onPress={loadEvent}
              style={styles.primaryButton}
            >
              <ThemedText style={styles.primaryButtonText}>重试</ThemedText>
            </Pressable>
          </View>
        ) : event ? (
          <ScrollView
            showsVerticalScrollIndicator={false}
            contentContainerStyle={styles.content}
          >
            <View style={styles.titleBlock}>
              <ThemedText style={styles.title}>{event.title}</ThemedText>
              {event.summary ? (
                <ThemedText style={styles.summary}>{event.summary}</ThemedText>
              ) : null}
            </View>

            {metaItems.length ? (
              <View style={styles.metaGrid}>
                {metaItems.map((item) => (
                  <View key={item.id} style={styles.metaChip}>
                    <ThemedText style={styles.metaText}>{item.label}</ThemedText>
                  </View>
                ))}
              </View>
            ) : null}

            {event.location_address ? (
              <View style={styles.infoPanel}>
                <ThemedText type="defaultSemiBold">详细地址</ThemedText>
                <ThemedText style={styles.infoText}>
                  {event.location_address}
                </ThemedText>
              </View>
            ) : null}

            {event.tags.length ? (
              <View style={styles.tags}>
                {event.tags.map((tag) => (
                  <View key={tag} style={styles.tag}>
                    <ThemedText style={styles.tagText}>#{tag}</ThemedText>
                  </View>
                ))}
              </View>
            ) : null}

            <EventContentView
              content={event.content}
              imageErrors={imageErrors}
              imageSources={imageSources}
            />
          </ScrollView>
        ) : null}
      </ThemedView>
    </SafeAreaView>
  );
}

async function loadImageSource(
  assetId: string,
  eventId: string,
): Promise<EventImageLoadResult> {
  try {
    const resp = await getMediaDownloadUrl(assetId);
    const source: EventImageSource = {
      uri: resp.download_url,
    };
    const headers = toImageHeaders(resp.download_headers);
    if (headers) {
      source.headers = headers;
    }
    return {
      assetId,
      source,
    };
  } catch (error) {
    const reason = error instanceof Error ? error.message : "图片下载地址获取失败";
    console.warn("[event-detail] image url load failed", {
      assetId,
      eventId,
      reason,
    });
    return { assetId, error: reason };
  }
}

function normalizeRouteParam(value: string | string[] | undefined): string {
  if (Array.isArray(value)) {
    return value[0]?.trim() ?? "";
  }
  return value?.trim() ?? "";
}

function buildMetaItems(event: EventResp): EventMetaItem[] {
  return [
    event.start_at || event.end_at
      ? { id: "time", label: formatEventTimeRange(event) }
      : undefined,
    event.location_name ? { id: "location", label: event.location_name } : undefined,
    event.capacity ? { id: "capacity", label: `${event.capacity} 人上限` } : undefined,
  ].filter((item): item is EventMetaItem => Boolean(item));
}

function formatEventTimeRange(event: EventResp): string {
  if (event.start_at && event.end_at) {
    return `${formatDateTime(event.start_at)} - ${formatDateTime(event.end_at)}`;
  }
  if (event.start_at) {
    return formatDateTime(event.start_at);
  }
  return event.end_at ? `截至 ${formatDateTime(event.end_at)}` : "";
}

function formatDateTime(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }

  // 使用系统 locale 展示本地时间，避免前后端时区不同导致用户看到 UTC 原始值。
  return date.toLocaleString(undefined, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

const styles = StyleSheet.create({
  safeArea: {
    flex: 1,
  },
  container: {
    flex: 1,
    paddingHorizontal: 16,
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
    color: "#687076",
  },
  centerState: {
    flex: 1,
    alignItems: "center",
    justifyContent: "center",
    gap: 12,
    paddingHorizontal: 24,
  },
  stateText: {
    textAlign: "center",
    color: "#687076",
  },
  primaryButton: {
    height: 42,
    minWidth: 92,
    borderRadius: 10,
    alignItems: "center",
    justifyContent: "center",
    backgroundColor: "#0A7EA4",
  },
  primaryButtonText: {
    color: "#FFFFFF",
    fontWeight: "700",
  },
  content: {
    paddingTop: 10,
    paddingBottom: 32,
    gap: 16,
  },
  titleBlock: {
    gap: 8,
  },
  title: {
    fontSize: 28,
    lineHeight: 36,
    fontWeight: "800",
    color: "#11181C",
  },
  summary: {
    fontSize: 16,
    lineHeight: 24,
    color: "#46515B",
  },
  metaGrid: {
    flexDirection: "row",
    flexWrap: "wrap",
    gap: 8,
  },
  metaChip: {
    maxWidth: "100%",
    borderRadius: 10,
    paddingHorizontal: 10,
    paddingVertical: 7,
    backgroundColor: "#EEF3F7",
  },
  metaText: {
    fontSize: 13,
    lineHeight: 18,
    color: "#46515B",
    fontWeight: "600",
  },
  infoPanel: {
    gap: 6,
    borderRadius: 12,
    borderWidth: 1,
    borderColor: "#D9E0EA",
    padding: 12,
    backgroundColor: "#F8FAFC",
  },
  infoText: {
    color: "#46515B",
  },
  tags: {
    flexDirection: "row",
    flexWrap: "wrap",
    gap: 8,
  },
  tag: {
    borderRadius: 999,
    paddingHorizontal: 10,
    paddingVertical: 6,
    backgroundColor: "#EAF6FA",
  },
  tagText: {
    fontSize: 13,
    lineHeight: 18,
    color: "#0A7EA4",
    fontWeight: "700",
  },
});
