import { router, useLocalSearchParams } from "expo-router";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  ActivityIndicator,
  Pressable,
  ScrollView,
  StyleSheet,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";

import { ThemedText } from "@/components/themed-text";
import { ThemedView } from "@/components/themed-view";
import { IconSymbol } from "@/components/ui/icon-symbol";
import type { EventJoinRequestResp, EventJoinReviewDecision } from "@/lib/dto";
import {
  listEventJoinRequests,
  reviewEventJoinRequest,
} from "@/lib/event-api";

export default function EventJoinRequestsScreen() {
  const params = useLocalSearchParams<{ eventId?: string | string[] }>();
  const eventId = normalizeRouteParam(params.eventId);
  const activeReviewRef = useRef<string | null>(null);
  const [requests, setRequests] = useState<EventJoinRequestResp[]>([]);
  const [loading, setLoading] = useState(false);
  const [reviewingUserId, setReviewingUserId] = useState<string | null>(null);
  const [error, setError] = useState("");

  const loadRequests = useCallback(async () => {
    if (!eventId) {
      setError("缺少活动 ID");
      return;
    }
    setLoading(true);
    setError("");
    console.info("[event-join-requests] loading pending requests", { eventId });
    try {
      const resp = await listEventJoinRequests(eventId);
      setRequests(resp.items);
      console.info("[event-join-requests] pending requests loaded", {
        eventId,
        count: resp.items.length,
      });
    } catch (loadError) {
      const reason = joinRequestErrorMessage(loadError);
      console.warn("[event-join-requests] request list load failed", {
        eventId,
        reason,
      });
      setRequests([]);
      setError(reason);
    } finally {
      setLoading(false);
    }
  }, [eventId]);

  useEffect(() => {
    void loadRequests();
  }, [loadRequests]);

  const review = useCallback(
    async (request: EventJoinRequestResp, decision: EventJoinReviewDecision) => {
      if (!eventId || activeReviewRef.current) {
        return;
      }
      activeReviewRef.current = request.user_id;
      setReviewingUserId(request.user_id);
      setError("");
      console.info("[event-join-requests] reviewing request", {
        eventId,
        applicantId: request.user_id,
        decision,
      });
      try {
        const participation = await reviewEventJoinRequest(
          eventId,
          request.user_id,
          { decision },
        );
        // 列表只展示 pending，审核成功后立即移除，服务端仍保留 joined/rejected 历史记录。
        setRequests((current) =>
          current.filter((item) => item.user_id !== request.user_id),
        );
        console.info("[event-join-requests] request reviewed", {
          eventId,
          applicantId: request.user_id,
          status: participation.status,
        });
      } catch (reviewError) {
        const reason = joinRequestErrorMessage(reviewError);
        console.warn("[event-join-requests] request review failed", {
          eventId,
          applicantId: request.user_id,
          decision,
          reason,
        });
        setError(reason);
      } finally {
        activeReviewRef.current = null;
        setReviewingUserId(null);
      }
    },
    [eventId],
  );

  return (
    <SafeAreaView
      style={styles.safeArea}
      edges={["top", "bottom", "left", "right"]}
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
            <ThemedText type="subtitle">加入申请</ThemedText>
            <ThemedText style={styles.headerMeta}>仅展示等待审核的申请</ThemedText>
          </View>
          <Pressable
            accessibilityRole="button"
            disabled={loading || reviewingUserId !== null}
            onPress={loadRequests}
            style={styles.refreshButton}
          >
            <ThemedText style={styles.refreshButtonText}>刷新</ThemedText>
          </Pressable>
        </View>

        {loading ? (
          <View style={styles.centerState}>
            <ActivityIndicator color="#0A7EA4" />
            <ThemedText style={styles.stateText}>正在加载加入申请...</ThemedText>
          </View>
        ) : requests.length === 0 ? (
          <View style={styles.centerState}>
            <ThemedText type="defaultSemiBold">暂无待审核申请</ThemedText>
            <ThemedText style={styles.stateText}>
              用户申请加入后会出现在这里
            </ThemedText>
            {error ? <ThemedText style={styles.errorText}>{error}</ThemedText> : null}
          </View>
        ) : (
          <ScrollView
            showsVerticalScrollIndicator={false}
            contentContainerStyle={styles.listContent}
          >
            {error ? <ThemedText style={styles.errorText}>{error}</ThemedText> : null}
            {requests.map((request) => {
              const reviewing = reviewingUserId === request.user_id;
              const disabled = reviewingUserId !== null;
              return (
                <View key={request.user_id} style={styles.requestCard}>
                  <View style={styles.requestInfo}>
                    <ThemedText type="defaultSemiBold" numberOfLines={1}>
                      {request.username}
                    </ThemedText>
                    <ThemedText style={styles.requestMeta}>
                      用户 ID：{request.user_id}
                    </ThemedText>
                    <ThemedText style={styles.requestMeta}>
                      申请于 {formatDateTime(request.requested_at)}
                    </ThemedText>
                  </View>
                  <View style={styles.requestActions}>
                    <Pressable
                      accessibilityRole="button"
                      disabled={disabled}
                      onPress={() => void review(request, "reject")}
                      style={[
                        styles.reviewButton,
                        styles.rejectButton,
                        disabled ? styles.buttonDisabled : undefined,
                      ]}
                    >
                      <ThemedText style={styles.rejectButtonText}>拒绝</ThemedText>
                    </Pressable>
                    <Pressable
                      accessibilityRole="button"
                      disabled={disabled}
                      onPress={() => void review(request, "approve")}
                      style={[
                        styles.reviewButton,
                        styles.approveButton,
                        disabled ? styles.buttonDisabled : undefined,
                      ]}
                    >
                      {reviewing ? (
                        <ActivityIndicator size="small" color="#FFFFFF" />
                      ) : (
                        <ThemedText style={styles.approveButtonText}>同意</ThemedText>
                      )}
                    </Pressable>
                  </View>
                </View>
              );
            })}
          </ScrollView>
        )}
      </ThemedView>
    </SafeAreaView>
  );
}

function normalizeRouteParam(value: string | string[] | undefined): string {
  return Array.isArray(value) ? (value[0]?.trim() ?? "") : (value?.trim() ?? "");
}

function formatDateTime(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

function joinRequestErrorMessage(error: unknown): string {
  const reason = error instanceof Error ? error.message : "加入申请操作失败";
  if (reason.includes("event_capacity_reached")) {
    return "活动人数已满，无法再通过新的申请";
  }
  if (reason.includes("event_join_request_not_found")) {
    return "该申请不存在或已被处理，请刷新列表";
  }
  return reason;
}

const styles = StyleSheet.create({
  safeArea: { flex: 1 },
  container: { flex: 1, paddingHorizontal: 16 },
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
  headerTitleWrap: { flex: 1 },
  headerMeta: { marginTop: 2, color: "#687076", fontSize: 12, lineHeight: 16 },
  refreshButton: { paddingHorizontal: 10, paddingVertical: 8 },
  refreshButtonText: { color: "#0A7EA4", fontWeight: "700" },
  centerState: {
    flex: 1,
    alignItems: "center",
    justifyContent: "center",
    gap: 10,
    paddingHorizontal: 24,
  },
  stateText: { color: "#687076", textAlign: "center" },
  errorText: { color: "#D64545", textAlign: "center" },
  listContent: { gap: 12, paddingVertical: 12, paddingBottom: 24 },
  requestCard: {
    borderWidth: 1,
    borderColor: "#D9E0EA",
    borderRadius: 14,
    padding: 14,
    gap: 12,
    backgroundColor: "#FFFFFF",
  },
  requestInfo: { gap: 4 },
  requestMeta: { color: "#687076", fontSize: 12, lineHeight: 17 },
  requestActions: { flexDirection: "row", justifyContent: "flex-end", gap: 8 },
  reviewButton: {
    minWidth: 76,
    minHeight: 40,
    borderRadius: 10,
    alignItems: "center",
    justifyContent: "center",
    paddingHorizontal: 14,
  },
  rejectButton: { borderWidth: 1, borderColor: "#D64545", backgroundColor: "#FFFFFF" },
  rejectButtonText: { color: "#D64545", fontWeight: "700" },
  approveButton: { backgroundColor: "#0A7EA4" },
  approveButtonText: { color: "#FFFFFF", fontWeight: "700" },
  buttonDisabled: { opacity: 0.5 },
});
