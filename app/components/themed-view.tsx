import { View, type ViewProps } from "react-native";

import { useThemeColor } from "@/hooks/use-theme-color";

export type ThemedViewProps = ViewProps & {
  lightColor?: string;
  darkColor?: string;
};

export function ThemedView({
  style,
  lightColor,
  darkColor,
  ...otherProps
}: ThemedViewProps) {
  const themeColors: { light?: string; dark?: string } = {};
  if (lightColor !== undefined) {
    themeColors.light = lightColor;
  }
  if (darkColor !== undefined) {
    themeColors.dark = darkColor;
  }
  const backgroundColor = useThemeColor(themeColors, "background");

  return <View style={[{ backgroundColor }, style]} {...otherProps} />;
}
