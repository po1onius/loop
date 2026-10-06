import { useRef, useState } from "react";
import { ActivityIndicator, PermissionsAndroid, Platform, StyleSheet, View } from "react-native";
import WebView from "react-native-webview";
import { ThemedText } from "@/components/themed-text";

export type MapPickerFrameProps = { uri: string; onMessage: (data: unknown) => void };

export default function MapPickerFrame({ uri, onMessage }: MapPickerFrameProps) {
  const [error, setError] = useState<string | null>(null);
  const webView = useRef<WebView>(null);
  const requestingLocation = useRef(false);
  const page = new URL(uri);
  async function requestLocation() {
    if (requestingLocation.current) return;
    requestingLocation.current = true;
    try {
      if (Platform.OS === "android") {
        // Android 12+ requires coarse and fine permissions in the same request.
        const grants = await PermissionsAndroid.requestMultiple([
          PermissionsAndroid.PERMISSIONS.ACCESS_COARSE_LOCATION,
          PermissionsAndroid.PERMISSIONS.ACCESS_FINE_LOCATION,
        ]);
        if (grants[PermissionsAndroid.PERMISSIONS.ACCESS_FINE_LOCATION] !== PermissionsAndroid.RESULTS.GRANTED) {
          console.info("[map-picker] precise location permission declined");
          webView.current?.injectJavaScript("window.loopMapLocationDenied?.(); true;");
          return;
        }
      }
      webView.current?.injectJavaScript("window.loopMapLocate?.(); true;");
    } catch {
      console.warn("[map-picker] location permission request failed");
      webView.current?.injectJavaScript("window.loopMapLocationDenied?.(); true;");
    } finally { requestingLocation.current = false; }
  }
  if (error) return <View style={styles.error}><ThemedText style={{ color: "#172033" }}>{error}</ThemedText></View>;
  return <WebView
    ref={webView}
    style={styles.frame}
    source={{ uri }}
    originWhitelist={[page.origin]}
    javaScriptEnabled
    geolocationEnabled
    setSupportMultipleWindows={false}
    mixedContentMode="never"
    startInLoadingState
    renderLoading={() => <ActivityIndicator style={styles.loading} />}
    onShouldStartLoadWithRequest={(request) => {
      try {
        const target = new URL(request.url);
        return target.origin === page.origin && target.pathname === page.pathname;
      } catch { return false; }
    }}
    onMessage={(event) => {
      try {
        const message: unknown = JSON.parse(event.nativeEvent.data);
        if (message && typeof message === "object" && "type" in message && message.type === "request_location") {
          void requestLocation();
        } else onMessage(message);
      }
      catch { console.warn("[map-picker] ignored invalid bridge message"); }
    }}
    onError={() => { console.warn("[map-picker] page load failed"); setError("地图加载失败，请关闭后重试。"); }}
    onHttpError={(event) => {
      console.warn("[map-picker] page returned HTTP error", { status: event.nativeEvent.statusCode });
      setError(event.nativeEvent.statusCode === 503 ? "地图服务尚未配置，请稍后重试。" : "地图加载失败，请关闭后重试。");
    }}
  />;
}

const styles = StyleSheet.create({
  frame: { flex: 1, backgroundColor: "#fff" },
  error: { flex: 1, alignItems: "center", justifyContent: "center", padding: 24 },
  loading: { position: "absolute", top: "50%", left: "50%" },
});
