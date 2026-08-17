import { Image } from "expo-image";
import { useEffect, useState } from "react";
import { StyleSheet, View, type StyleProp, type ViewStyle } from "react-native";

import { ThemedText } from "@/components/themed-text";
import { getMediaDownloadUrl } from "@/lib/media-api";

const avatarUrlCache = new Map<string, Promise<string | null>>();

export function UserAvatar({
  username,
  avatarAssetId,
  size = 38,
  style,
}: {
  username: string;
  avatarAssetId: string | null;
  size?: number;
  style?: StyleProp<ViewStyle>;
}) {
  const [uri, setUri] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    setUri(null);
    if (!avatarAssetId) return () => { active = false; };
    let request = avatarUrlCache.get(avatarAssetId);
    if (!request) {
      request = getMediaDownloadUrl(avatarAssetId)
        .then((resp) => resp.download_url)
        .catch((error) => {
          // 失败结果不缓存，下一次渲染仍有机会重试临时的网络或签名错误。
          avatarUrlCache.delete(avatarAssetId);
          console.warn("[user-avatar] avatar URL load failed", {
            avatarAssetId,
            reason: error instanceof Error ? error.message : String(error),
          });
          return null;
        });
      avatarUrlCache.set(avatarAssetId, request);
    }
    void request.then((nextUri) => {
      if (active) setUri(nextUri);
    });
    return () => {
      active = false;
    };
  }, [avatarAssetId]);

  const dimension = { width: size, height: size, borderRadius: size / 2 };
  return (
    <View style={[styles.fallback, dimension, style]}>
      {uri ? (
        <Image source={uri} contentFit="cover" transition={120} style={[styles.image, dimension]} />
      ) : (
        <ThemedText style={[styles.initial, { fontSize: Math.max(11, size * 0.4) }]}>
          {username.trim().slice(0, 1).toUpperCase() || "?"}
        </ThemedText>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  fallback: { alignItems: "center", justifyContent: "center", backgroundColor: "#D9EEF5", overflow: "hidden" },
  image: { position: "absolute" },
  initial: { color: "#08789C", fontWeight: "800" },
});
