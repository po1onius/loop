import {
  DarkTheme,
  DefaultTheme,
  ThemeProvider,
  type Theme,
} from "@react-navigation/native";
import { Stack } from "expo-router";
import { StatusBar } from "expo-status-bar";
import * as SystemUI from "expo-system-ui";
import { useEffect, useMemo } from "react";
import "react-native-reanimated";

import { Colors } from "@/constants/theme";
import { useColorScheme } from "@/hooks/use-color-scheme";

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

  return (
    <ThemeProvider value={appTheme}>
      <Stack
        screenOptions={{ contentStyle: { backgroundColor: screenBackground } }}
      >
        <Stack.Screen name="login" options={{ headerShown: false }} />
        <Stack.Screen name="register" options={{ headerShown: false }} />
        <Stack.Screen name="create-event" options={{ headerShown: false }} />
        <Stack.Screen name="event/[eventId]" options={{ headerShown: false }} />
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
