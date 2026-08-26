import {
  DarkTheme,
  DefaultTheme,
  ThemeProvider,
  type Theme,
} from "@react-navigation/native";
import { router, Stack } from "expo-router";
import { StatusBar } from "expo-status-bar";
import * as SystemUI from "expo-system-ui";
import { useEffect, useMemo } from "react";
import { AppState } from "react-native";
import "react-native-reanimated";

import { Colors } from "@/constants/theme";
import { useColorScheme } from "@/hooks/use-color-scheme";
import { hasRefreshSession, restoreAuthSession } from "@/lib/auth-api";
import {
  hasAccessToken,
  refreshAccessTokenIfNeeded,
  subscribeAuthenticationExpired,
} from "@/lib/api-client";

export const unstable_settings = {
  anchor: "(tabs)",
};

export default function RootLayout() {
  const colorScheme = useColorScheme();
  const colorSchemeName = colorScheme === "dark" ? "dark" : "light";
  const screenBackground = Colors[colorSchemeName].background;
  const appTheme = useMemo<Theme>(() => {
    const baseTheme = colorSchemeName === "dark" ? DarkTheme : DefaultTheme;
    return {
      ...baseTheme,
      colors: {
        ...baseTheme.colors,
        background: screenBackground,
        card: screenBackground,
      },
    };
  }, [colorSchemeName, screenBackground]);

  useEffect(() => {
    // 同步原生 root 背景，避免 SafeArea、键盘圆角和页面转场露出导航默认底色。
    void SystemUI.setBackgroundColorAsync(screenBackground).catch((error) => {
      console.warn("[layout] system background update failed", error);
    });
  }, [screenBackground]);

  useEffect(() => {
    let active = true;
    const unsubscribe = subscribeAuthenticationExpired((reason) => {
      console.info("[layout] redirecting to login after session expiration", {
        reason,
      });
      router.replace({
        pathname: "/login",
        params: { reason: "expired" },
      } as never);
    });
    const appStateSubscription = AppState.addEventListener(
      "change",
      (nextState) => {
        if (
          nextState === "active" &&
          (hasAccessToken() || hasRefreshSession())
        ) {
          // 移动系统会暂停后台 JS 定时器；恢复前台后如已进入刷新窗口，立即轮换
          // token。没有本地 access token 的冷启动由 restoreAuthSession 负责。
          const hadAccessToken = hasAccessToken();
          void refreshAccessTokenIfNeeded("app_foreground").then((result) => {
            if (!hadAccessToken && result === "ready") {
              console.info("[layout] restored session after returning active");
              router.replace("/(tabs)" as never);
            }
          });
        }
      },
    );

    void restoreAuthSession().then((restored) => {
      if (!active || !restored) {
        return;
      }
      console.info("[layout] restored session; entering app");
      router.replace("/(tabs)" as never);
    });
    return () => {
      active = false;
      unsubscribe();
      appStateSubscription.remove();
    };
  }, []);

  return (
    <ThemeProvider value={appTheme}>
      <Stack
        screenOptions={{ contentStyle: { backgroundColor: screenBackground } }}
      >
        <Stack.Screen name="login" options={{ headerShown: false }} />
        <Stack.Screen name="register" options={{ headerShown: false }} />
        <Stack.Screen name="create-event" options={{ headerShown: false }} />
        <Stack.Screen name="search-events" options={{ headerShown: false }} />
        <Stack.Screen name="create-post" options={{ headerShown: false }} />
        <Stack.Screen name="event/[eventId]" options={{ headerShown: false }} />
        <Stack.Screen
          name="community/post/[postId]"
          options={{ headerShown: false }}
        />
        <Stack.Screen
          name="conversation/[conversationId]"
          options={{ headerShown: false }}
        />
        <Stack.Screen
          name="event/[eventId]/join-requests"
          options={{ headerShown: false }}
        />
        <Stack.Screen name="(tabs)" options={{ headerShown: false }} />
        <Stack.Screen
          name="modal"
          options={{ presentation: "modal", title: "Modal" }}
        />
      </Stack>
      <StatusBar style="auto" />
    </ThemeProvider>
  );
}
