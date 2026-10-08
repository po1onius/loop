import { useFocusEffect, useIsFocused } from "@react-navigation/native";
import { useCallback, useEffect, useSyncExternalStore } from "react";
import { AppState } from "react-native";

import { getUserLocationState, refreshUserLocation, subscribeUserLocation } from "@/lib/user-location";

export function useUserLocation(enabled = true) {
  const state = useSyncExternalStore(subscribeUserLocation, getUserLocationState, getUserLocationState);
  const focused = useIsFocused();

  useFocusEffect(useCallback(() => {
    if (!enabled) return;
    void refreshUserLocation();
    const subscription = AppState.addEventListener("change", (next) => {
      if (next === "active") void refreshUserLocation();
    });
    return () => subscription.remove();
  }, [enabled]));

  useEffect(() => {
    if (enabled && focused && state.status === "stale" && AppState.currentState === "active") {
      void refreshUserLocation();
    }
  }, [enabled, focused, state.status]);

  return state;
}
