import * as Crypto from "expo-crypto";
import { useCallback, useState } from "react";
import { Modal, Platform, Pressable, StyleSheet, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import MapPickerFrame from "@/components/map-picker-frame";
import { ThemedText } from "@/components/themed-text";
import { getApiBaseUrl } from "@/lib/api-client";
import { isMapLocation, type MapLocation } from "@/lib/event-location";

export function EventLocationPicker({ location, onSelect, onClose }: {
  location: MapLocation | null;
  onSelect: (location: MapLocation) => void;
  onClose: () => void;
}) {
  const [session] = useState(() => {
    const channel = Crypto.randomUUID();
    const baseUrl = getApiBaseUrl();
    const initial = { channel, location, parentOrigin: Platform.OS === "web" ? window.location.origin : null };
    return { channel, uri: baseUrl ? `${baseUrl}/maps/picker#${encodeURIComponent(JSON.stringify(initial))}` : null };
  });
  const receive = useCallback((value: unknown) => {
    if (!value || typeof value !== "object") return;
    const message = value as Record<string, unknown>;
    if (message["channel"] !== session.channel) return;
    if (message["type"] === "selected" && isMapLocation(message["location"])) {
      console.info("[map-picker] location accepted", { hasPoi: Boolean(message["location"].poiId) });
      onSelect(message["location"]);
    } else if (message["type"] === "log" && typeof message["event"] === "string") {
      console.info("[map-picker] page event", { event: message["event"], extra: message["extra"] });
    }
  }, [session.channel, onSelect]);
  return <Modal visible animationType="slide" onRequestClose={onClose}>
    <SafeAreaView style={styles.container}>
      <View style={styles.header}>
        <ThemedText type="defaultSemiBold" style={styles.title}>选择活动地点</ThemedText>
        <Pressable accessibilityRole="button" onPress={onClose} hitSlop={12}>
          <ThemedText style={styles.cancel}>取消</ThemedText>
        </Pressable>
      </View>
      {session.uri ? <MapPickerFrame uri={session.uri} onMessage={receive} /> : <ThemedText style={styles.title}>服务地址尚未配置，暂时无法打开地图。</ThemedText>}
    </SafeAreaView>
  </Modal>;
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: "#fff" },
  header: { flexDirection: "row", alignItems: "center", justifyContent: "space-between", padding: 16 },
  title: { color: "#172033" },
  cancel: { color: "#176bff" },
});
