import { useFocusEffect } from "@react-navigation/native";
import { Image } from "expo-image";
import * as ImagePicker from "expo-image-picker";
import { router } from "expo-router";
import { useCallback, useRef, useState } from "react";
import {
  ActivityIndicator,
  Pressable,
  RefreshControl,
  ScrollView,
  StyleSheet,
  View,
} from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";

import { ThemedText } from "@/components/themed-text";
import { ThemedView } from "@/components/themed-view";
import { IconSymbol } from "@/components/ui/icon-symbol";
import { useColorScheme } from "@/hooks/use-color-scheme";
import { getCurrentUser, updateCurrentUserAvatar } from "@/lib/auth-api";
import { hasAccessToken } from "@/lib/api-client";
import type { CurrentUserResp, EventResp } from "@/lib/dto";
import { listMyEvents, listMyJoinedEvents } from "@/lib/event-api";
import { getMediaDownloadUrl, uploadLocalImageAsset } from "@/lib/media-api";

type PersonalSection = "joined" | "published" | "posts";
type LoadMode = "background" | "refresh";

const PERSONAL_SECTIONS: {
  id: PersonalSection;
  label: string;
}[] = [
  { id: "joined", label: "参加的活动" },
  { id: "published", label: "发布的活动" },
  { id: "posts", label: "帖子" },
];

export default function MeScreen() {
  const isDark = useColorScheme() === "dark";
  const latestRequestIdRef = useRef(0);
  const [profile, setProfile] = useState<CurrentUserResp | null>(null);
  const [joinedEvents, setJoinedEvents] = useState<EventResp[]>([]);
  const [publishedEvents, setPublishedEvents] = useState<EventResp[]>([]);
  const [avatarUri, setAvatarUri] = useState<string | null>(null);
  const [activeSection, setActiveSection] =
    useState<PersonalSection>("joined");
  const [loading, setLoading] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [updatingAvatar, setUpdatingAvatar] = useState(false);
  const [avatarError, setAvatarError] = useState("");
  const [error, setError] = useState("");

  const loadPersonalCenter = useCallback(async (mode: LoadMode) => {
    if (!hasAccessToken()) {
      console.info("[me] personal center requires login");
      router.replace({
        pathname: "/login",
        params: { redirect: "/(tabs)/me" },
      } as never);
      return;
    }

    const requestId = latestRequestIdRef.current + 1;
    latestRequestIdRef.current = requestId;
    if (mode === "refresh") {
      setRefreshing(true);
    } else {
      setLoading(true);
    }
    setError("");

    try {
      console.info("[me] loading personal center", { mode, requestId });
      // 三块数据互不依赖，并行请求可以让资料卡和活动列表同时完成加载。
      const [profileBundle, joinedResp, publishedResp] = await Promise.all([
        getCurrentUser().then(async (nextProfile) => ({
          nextProfile,
          nextAvatarUri: await resolveAvatarDownloadUrl(
            nextProfile.avatar_asset_id,
          ),
        })),
        listMyJoinedEvents(),
        listMyEvents("published"),
      ]);
      if (latestRequestIdRef.current !== requestId) {
        console.info("[me] ignored stale personal center response", {
          requestId,
        });
        return;
      }

      const { nextProfile, nextAvatarUri } = profileBundle;
      setProfile(nextProfile);
      setAvatarUri(nextAvatarUri);
      setJoinedEvents(joinedResp.items);
      setPublishedEvents(publishedResp.items);
      console.info("[me] personal center loaded", {
        requestId,
        userId: nextProfile.user_id,
        joinedEventCount: joinedResp.items.length,
        publishedEventCount: publishedResp.items.length,
      });
    } catch (loadError) {
      if (latestRequestIdRef.current !== requestId) {
        return;
      }
      const message =
        loadError instanceof Error ? loadError.message : "个人中心加载失败";
      console.warn("[me] personal center load failed", {
        requestId,
        reason: message,
      });
      setError(message);
    } finally {
      if (latestRequestIdRef.current === requestId) {
        setLoading(false);
        setRefreshing(false);
      }
    }
  }, []);

  useFocusEffect(
    useCallback(() => {
      void loadPersonalCenter("background");
      return () => {
        // 页面离开后作废仍在途的响应，避免旧请求覆盖下一次进入页面的数据。
        latestRequestIdRef.current += 1;
        setLoading(false);
        setRefreshing(false);
      };
    }, [loadPersonalCenter]),
  );

  const handleSectionPress = useCallback(
    (section: PersonalSection) => {
      if (section === activeSection) {
        return;
      }
      console.info("[me] personal section changed", {
        from: activeSection,
        to: section,
      });
      setActiveSection(section);
    },
    [activeSection],
  );

  const handleAvatarPress = useCallback(async () => {
    if (loading || refreshing || updatingAvatar) {
      return;
    }

    setAvatarError("");
    setUpdatingAvatar(true);
    try {
      const permission = await ImagePicker.requestMediaLibraryPermissionsAsync();
      if (!permission.granted) {
        throw new Error("需要相册权限才能修改头像");
      }

      const result = await ImagePicker.launchImageLibraryAsync({
        mediaTypes: ["images"],
        allowsEditing: true,
        aspect: [1, 1],
        quality: 0.9,
      });
      if (result.canceled || !result.assets.length) {
        console.info("[me] avatar selection canceled");
        return;
      }

      const selected = result.assets[0];
      if (!selected) {
        throw new Error("没有读取到所选头像图片");
      }
      console.info("[me] uploading selected avatar", {
        width: selected.width,
        height: selected.height,
        mimeType: selected.mimeType,
        fileSize: selected.fileSize,
      });
      const uploaded = await uploadLocalImageAsset({
        uri: selected.uri,
        mimeType: selected.mimeType ?? null,
        fileName: selected.fileName ?? null,
        width: selected.width,
        height: selected.height,
        file: selected.file ?? null,
      });
      const updatedProfile = await updateCurrentUserAvatar({
        avatar_asset_id: uploaded.asset_id,
      });
      const nextAvatarUri =
        uploaded.public_url ??
        (await resolveAvatarDownloadUrl(updatedProfile.avatar_asset_id));

      setProfile(updatedProfile);
      setAvatarUri(nextAvatarUri);
      console.info("[me] avatar update completed", {
        userId: updatedProfile.user_id,
        avatarAssetId: updatedProfile.avatar_asset_id,
      });
    } catch (updateError) {
      const message =
        updateError instanceof Error ? updateError.message : "头像修改失败";
      console.warn("[me] avatar update failed", { reason: message });
      setAvatarError(message);
    } finally {
      setUpdatingAvatar(false);
    }
  }, [loading, refreshing, updatingAvatar]);

  const handleAvatarLoadError = useCallback(() => {
    console.warn("[me] avatar image render failed", {
      avatarAssetId: profile?.avatar_asset_id ?? null,
    });
    setAvatarUri(null);
    setAvatarError("头像加载失败，请稍后重试或重新选择图片");
  }, [profile?.avatar_asset_id]);

  const currentEvents =
    activeSection === "joined" ? joinedEvents : publishedEvents;

  return (
    <SafeAreaView style={styles.safeArea} edges={["top", "left", "right"]}>
      <ThemedView style={styles.container}>
        <ScrollView
          showsVerticalScrollIndicator={false}
          contentContainerStyle={styles.content}
          refreshControl={
            <RefreshControl
              refreshing={refreshing}
              tintColor="#0A7EA4"
              onRefresh={() => void loadPersonalCenter("refresh")}
            />
          }
        >
          <ThemedText type="title">我</ThemedText>

          <ProfileCard
            profile={profile}
            avatarUri={avatarUri}
            loading={loading}
            updatingAvatar={updatingAvatar}
            onAvatarPress={() => void handleAvatarPress()}
            onAvatarLoadError={handleAvatarLoadError}
          />

          {avatarError ? (
            <ThemedText style={styles.avatarErrorText}>{avatarError}</ThemedText>
          ) : null}

          {error ? (
            <ThemedView
              style={styles.errorCard}
              lightColor="#FFF2F2"
              darkColor="#3A2020"
            >
              <ThemedText style={styles.errorText}>{error}</ThemedText>
              <Pressable
                accessibilityRole="button"
                onPress={() => void loadPersonalCenter("background")}
                style={styles.retryButton}
              >
                <ThemedText style={styles.retryText}>重新加载</ThemedText>
              </Pressable>
            </ThemedView>
          ) : null}

          <View
            style={[
              styles.statsRow,
              { borderColor: isDark ? "#2F3A45" : "#DDE3EA" },
            ]}
          >
            <StatItem
              label="参加活动"
              value={profile ? joinedEvents.length : "—"}
            />
            <View
              style={[
                styles.statDivider,
                { backgroundColor: isDark ? "#2F3A45" : "#DDE3EA" },
              ]}
            />
            <StatItem
              label="发布活动"
              value={profile ? publishedEvents.length : "—"}
            />
            <View
              style={[
                styles.statDivider,
                { backgroundColor: isDark ? "#2F3A45" : "#DDE3EA" },
              ]}
            />
            <StatItem label="帖子" value="—" />
          </View>

          <View
            accessibilityRole="tablist"
            style={[
              styles.sectionTabs,
              { backgroundColor: isDark ? "#20272E" : "#EEF2F6" },
            ]}
          >
            {PERSONAL_SECTIONS.map((section) => {
              const selected = activeSection === section.id;
              return (
                <Pressable
                  key={section.id}
                  accessibilityRole="tab"
                  accessibilityState={{ selected }}
                  onPress={() => handleSectionPress(section.id)}
                  style={[
                    styles.sectionTab,
                    selected
                      ? [
                          styles.sectionTabActive,
                          {
                            backgroundColor: isDark ? "#2F3A45" : "#FFFFFF",
                          },
                        ]
                      : undefined,
                  ]}
                >
                  <ThemedText
                    numberOfLines={1}
                    style={[
                      styles.sectionTabText,
                      selected ? styles.sectionTabTextActive : undefined,
                    ]}
                  >
                    {section.label}
                  </ThemedText>
                </Pressable>
              );
            })}
          </View>

          {activeSection === "posts" ? (
            <EmptyContent
              title="帖子内容暂未接入"
              description="当前社区还没有帖子数据接口，接入后会在这里集中展示个人帖子"
              actionLabel="去社区看看"
              onAction={() => router.push("/(tabs)/community" as never)}
            />
          ) : loading && !profile ? (
            <View style={styles.loadingContent}>
              <ActivityIndicator color="#0A7EA4" />
              <ThemedText style={styles.mutedText}>正在加载个人内容...</ThemedText>
            </View>
          ) : error && !profile ? null : currentEvents.length ? (
            <View style={styles.eventList}>
              {currentEvents.map((event) => (
                <EventCard key={event.event_id} event={event} />
              ))}
            </View>
          ) : (
            <EmptyContent
              title={
                activeSection === "joined"
                  ? "还没有参加活动"
                  : "还没有发布活动"
              }
              description={
                activeSection === "joined"
                  ? "在活动页找到感兴趣的活动并完成报名"
                  : "发布第一个活动，认识更多志同道合的人"
              }
              actionLabel={activeSection === "joined" ? "发现活动" : "发布活动"}
              onAction={() =>
                activeSection === "joined"
                  ? router.push("/(tabs)" as never)
                  : router.push("/create-event" as never)
              }
            />
          )}
        </ScrollView>
      </ThemedView>
    </SafeAreaView>
  );
}

function ProfileCard({
  profile,
  avatarUri,
  loading,
  updatingAvatar,
  onAvatarPress,
  onAvatarLoadError,
}: {
  profile: CurrentUserResp | null;
  avatarUri: string | null;
  loading: boolean;
  updatingAvatar: boolean;
  onAvatarPress: () => void;
  onAvatarLoadError: () => void;
}) {
  return (
    <ThemedView
      style={styles.profileCard}
      lightColor="#EAF6FA"
      darkColor="#17303A"
    >
      <Pressable
        accessibilityRole="button"
        accessibilityLabel="修改用户头像"
        accessibilityState={{ disabled: loading || updatingAvatar }}
        disabled={loading || updatingAvatar}
        onPress={onAvatarPress}
        style={({ pressed }) => [
          styles.avatar,
          pressed ? styles.pressed : undefined,
        ]}
      >
        {avatarUri ? (
          <Image
            source={avatarUri}
            contentFit="cover"
            transition={160}
            onError={onAvatarLoadError}
            style={styles.avatarImage}
          />
        ) : (
          <IconSymbol size={54} name="person.crop.circle" color="#0A7EA4" />
        )}
        {updatingAvatar ? (
          <View style={styles.avatarUploadingMask}>
            <ActivityIndicator color="#FFFFFF" />
          </View>
        ) : null}
        <View style={styles.avatarEditBadge}>
          <IconSymbol size={13} name="square.and.pencil" color="#FFFFFF" />
        </View>
      </Pressable>
      <View style={styles.profileText}>
        {profile ? (
          <>
            <ThemedText type="subtitle" numberOfLines={1}>
              {profile.username}
            </ThemedText>
            <ThemedText style={styles.accountText} numberOfLines={1}>
              {profile.account}
            </ThemedText>
            <ThemedText style={styles.roleText}>
              {formatRole(profile.role)}
            </ThemedText>
            <ThemedText style={styles.avatarHint}>点击头像可修改</ThemedText>
          </>
        ) : (
          <View style={styles.profilePlaceholder}>
            {loading ? <ActivityIndicator color="#0A7EA4" /> : null}
            <ThemedText style={styles.mutedText}>
              {loading ? "正在加载个人信息..." : "个人信息暂不可用"}
            </ThemedText>
          </View>
        )}
      </View>
    </ThemedView>
  );
}

function StatItem({
  label,
  value,
}: {
  label: string;
  value: number | string;
}) {
  return (
    <View style={styles.statItem}>
      <ThemedText type="subtitle" style={styles.statValue}>
        {value}
      </ThemedText>
      <ThemedText style={styles.statLabel}>{label}</ThemedText>
    </View>
  );
}

function EventCard({ event }: { event: EventResp }) {
  const handlePress = () => {
    console.info("[me] personal event pressed", {
      eventId: event.event_id,
      eventStatus: event.status,
    });
    router.push(`/event/${encodeURIComponent(event.event_id)}` as never);
  };

  return (
    <Pressable
      accessibilityRole="button"
      onPress={handlePress}
      style={({ pressed }) => [
        styles.eventPressable,
        pressed ? styles.pressed : undefined,
      ]}
    >
      <ThemedView
        style={styles.eventCard}
        lightColor="#F3F6FA"
        darkColor="#1E252C"
      >
        <View style={styles.eventHeader}>
          <ThemedText
            type="defaultSemiBold"
            numberOfLines={1}
            style={styles.eventTitle}
          >
            {event.title}
          </ThemedText>
          <IconSymbol size={18} name="chevron.right" color="#8A94A6" />
        </View>
        <ThemedText style={styles.eventMeta} numberOfLines={2}>
          {formatEventMeta(event)}
        </ThemedText>
      </ThemedView>
    </Pressable>
  );
}

function EmptyContent({
  title,
  description,
  actionLabel,
  onAction,
}: {
  title: string;
  description: string;
  actionLabel: string;
  onAction: () => void;
}) {
  return (
    <ThemedView
      style={styles.emptyContent}
      lightColor="#F3F6FA"
      darkColor="#1E252C"
    >
      <ThemedText type="defaultSemiBold">{title}</ThemedText>
      <ThemedText style={styles.emptyDescription}>{description}</ThemedText>
      <Pressable
        accessibilityRole="button"
        onPress={onAction}
        style={styles.emptyAction}
      >
        <ThemedText style={styles.emptyActionText}>{actionLabel}</ThemedText>
      </Pressable>
    </ThemedView>
  );
}

function formatRole(role: string): string {
  switch (role) {
    case "admin":
      return "管理员";
    case "organizer":
      return "活动组织者";
    case "user":
      return "Loop 用户";
    default:
      return role;
  }
}

async function resolveAvatarDownloadUrl(
  avatarAssetId: string | null,
): Promise<string | null> {
  const normalizedAssetId = avatarAssetId?.trim();
  if (!normalizedAssetId) {
    return null;
  }

  try {
    const download = await getMediaDownloadUrl(normalizedAssetId);
    return download.download_url;
  } catch (error) {
    // 资料、活动和头像渲染相互独立；头像地址获取失败时保留默认头像，
    // 不阻断个人中心的其他内容。
    console.warn("[me] avatar download url load failed", {
      avatarAssetId: normalizedAssetId,
      reason: error instanceof Error ? error.message : String(error),
    });
    return null;
  }
}

function formatEventMeta(event: EventResp): string {
  const parts = [
    event.start_at ? formatDateTime(event.start_at) : undefined,
    event.location_name,
    event.summary,
  ].filter(Boolean);
  return parts.join(" · ") || "暂无活动简介";
}

function formatDateTime(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
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
  },
  content: {
    paddingHorizontal: 16,
    paddingTop: 16,
    paddingBottom: 32,
  },
  profileCard: {
    minHeight: 116,
    flexDirection: "row",
    alignItems: "center",
    gap: 14,
    marginTop: 16,
    borderRadius: 18,
    paddingHorizontal: 16,
    paddingVertical: 18,
  },
  avatar: {
    width: 64,
    height: 64,
    alignItems: "center",
    justifyContent: "center",
    borderRadius: 32,
    backgroundColor: "#FFFFFF",
    overflow: "visible",
  },
  avatarImage: {
    width: 64,
    height: 64,
    borderRadius: 32,
  },
  avatarUploadingMask: {
    ...StyleSheet.absoluteFillObject,
    alignItems: "center",
    justifyContent: "center",
    borderRadius: 32,
    backgroundColor: "rgba(17, 24, 28, 0.5)",
  },
  avatarEditBadge: {
    position: "absolute",
    right: -2,
    bottom: -2,
    width: 25,
    height: 25,
    alignItems: "center",
    justifyContent: "center",
    borderRadius: 13,
    borderWidth: 2,
    borderColor: "#FFFFFF",
    backgroundColor: "#0A7EA4",
  },
  profileText: {
    flex: 1,
  },
  profilePlaceholder: {
    minHeight: 58,
    flexDirection: "row",
    alignItems: "center",
    gap: 10,
  },
  accountText: {
    marginTop: 3,
    lineHeight: 20,
    opacity: 0.72,
  },
  roleText: {
    alignSelf: "flex-start",
    marginTop: 8,
    borderRadius: 99,
    paddingHorizontal: 9,
    paddingVertical: 3,
    color: "#0A7EA4",
    backgroundColor: "rgba(10, 126, 164, 0.12)",
    fontSize: 12,
    lineHeight: 16,
    fontWeight: "700",
  },
  avatarHint: {
    marginTop: 5,
    color: "#687076",
    fontSize: 12,
    lineHeight: 16,
  },
  avatarErrorText: {
    marginTop: 8,
    color: "#D64545",
    fontSize: 13,
    lineHeight: 18,
  },
  errorCard: {
    flexDirection: "row",
    alignItems: "center",
    gap: 10,
    marginTop: 12,
    borderRadius: 12,
    paddingHorizontal: 12,
    paddingVertical: 10,
  },
  errorText: {
    flex: 1,
    color: "#D64545",
    fontSize: 13,
    lineHeight: 18,
  },
  retryButton: {
    paddingHorizontal: 8,
    paddingVertical: 5,
  },
  retryText: {
    color: "#0A7EA4",
    fontWeight: "700",
  },
  statsRow: {
    minHeight: 78,
    flexDirection: "row",
    alignItems: "center",
    marginTop: 14,
    borderRadius: 14,
    borderWidth: 1,
  },
  statItem: {
    flex: 1,
    alignItems: "center",
    gap: 2,
  },
  statValue: {
    color: "#0A7EA4",
  },
  statLabel: {
    fontSize: 12,
    lineHeight: 17,
    opacity: 0.68,
  },
  statDivider: {
    width: 1,
    height: 34,
  },
  sectionTabs: {
    flexDirection: "row",
    gap: 4,
    marginTop: 20,
    borderRadius: 12,
    padding: 4,
  },
  sectionTab: {
    minHeight: 38,
    flex: 1,
    alignItems: "center",
    justifyContent: "center",
    borderRadius: 9,
    paddingHorizontal: 5,
  },
  sectionTabActive: {
    shadowColor: "#000000",
    shadowOpacity: 0.06,
    shadowRadius: 4,
    shadowOffset: { width: 0, height: 1 },
    elevation: 1,
  },
  sectionTabText: {
    color: "#687076",
    fontSize: 13,
    lineHeight: 18,
    fontWeight: "600",
  },
  sectionTabTextActive: {
    color: "#0A7EA4",
    fontWeight: "800",
  },
  loadingContent: {
    minHeight: 180,
    alignItems: "center",
    justifyContent: "center",
    gap: 10,
  },
  eventList: {
    marginTop: 12,
    gap: 10,
  },
  eventPressable: {
    borderRadius: 12,
  },
  pressed: {
    opacity: 0.72,
  },
  eventCard: {
    borderRadius: 12,
    paddingHorizontal: 14,
    paddingVertical: 13,
    gap: 5,
  },
  eventHeader: {
    flexDirection: "row",
    alignItems: "center",
    gap: 8,
  },
  eventTitle: {
    flex: 1,
  },
  eventMeta: {
    lineHeight: 20,
    opacity: 0.7,
  },
  emptyContent: {
    minHeight: 190,
    alignItems: "center",
    justifyContent: "center",
    gap: 8,
    marginTop: 12,
    borderRadius: 14,
    paddingHorizontal: 24,
  },
  emptyDescription: {
    textAlign: "center",
    lineHeight: 21,
    opacity: 0.68,
  },
  emptyAction: {
    marginTop: 8,
    borderRadius: 9,
    paddingHorizontal: 14,
    paddingVertical: 9,
    backgroundColor: "#0A7EA4",
  },
  emptyActionText: {
    color: "#FFFFFF",
    fontWeight: "700",
  },
  mutedText: {
    color: "#687076",
  },
});
