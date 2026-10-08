import { Alert, Linking, Platform, Pressable, StyleSheet, View } from "react-native";

import { ThemedText } from "@/components/themed-text";
import { refreshUserLocation, type UserLocationState } from "@/lib/user-location";

export function LocationDistanceControl({ state }: { state: UserLocationState }) {
  const loading = state.status === "loading";
  const settings = state.status === "denied" && (!state.canAskAgain || Platform.OS === "web");
  const label = state.status === "ready" ? "刷新位置"
    : settings ? (Platform.OS === "web" ? "重新检查" : "开启权限")
      : state.status === "error" ? "重试" : state.status === "stale" ? "刷新位置" : "开启定位";

  function handlePress() {
    if (settings && Platform.OS !== "web") {
      void Linking.openSettings().catch(() => {
        console.warn("[user-location] could not open system settings");
        Alert.alert("无法打开设置", "请在系统设置中允许 Loop 访问位置信息。");
      });
      return;
    }
    void refreshUserLocation({ requestPermission: true, force: true });
  }

  return <View style={styles.container}>
    <ThemedText style={styles.message} accessibilityLiveRegion="polite">
      {settings && Platform.OS === "web" ? "请在浏览器的网站权限设置中允许位置访问" : state.message}
    </ThemedText>
    {!loading && <Pressable accessibilityRole="button" onPress={handlePress} hitSlop={8}>
      <ThemedText type="link" style={styles.action}>{label}</ThemedText>
    </Pressable>}
  </View>;
}

const styles = StyleSheet.create({
  container: { flexDirection: "row", alignItems: "center", gap: 12, paddingVertical: 8 },
  message: { flex: 1, fontSize: 13, lineHeight: 20 },
  action: { fontSize: 13 },
});
