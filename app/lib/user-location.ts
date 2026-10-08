import * as Location from "expo-location";

import { ApiRequestError, requestJson } from "@/lib/api-client";
import type { ConvertCoordinatesRequest, ConvertCoordinatesResp } from "@/lib/dto";

export const LOCATION_MAX_AGE_MS = 2 * 60_000;
const LOCATION_TIMEOUT_MS = 20_000;

export type UserLocation = ConvertCoordinatesResp & {
  coordinateSystem: "GCJ-02";
  capturedAt: number;
};

type LocationStatus = "idle" | "loading" | "ready" | "stale" | "denied" | "error";
export type UserLocationState = {
  status: LocationStatus;
  position: UserLocation | null;
  canAskAgain: boolean;
  message: string;
};

let state: UserLocationState = {
  status: "idle", position: null, canAskAgain: true, message: "开启定位，查看距离",
};
let pending: Promise<void> | null = null;
let expiryTimer: ReturnType<typeof setTimeout> | undefined;
const listeners = new Set<() => void>();

export function subscribeUserLocation(listener: () => void) {
  listeners.add(listener);
  return () => { listeners.delete(listener); };
}

export function getUserLocationState() { return state; }

function update(next: UserLocationState) {
  clearTimeout(expiryTimer);
  state = next;
  if (next.position) {
    expiryTimer = setTimeout(() => {
      update({ status: "stale", position: null, canAskAgain: true, message: "位置已过期，请刷新位置" });
      console.info("[user-location] cached location expired");
    }, Math.max(0, next.position.capturedAt + LOCATION_MAX_AGE_MS - Date.now()));
  }
  for (const listener of listeners) listener();
}

// One request serves every page. Coordinates stay in memory and are sent only to
// the conversion endpoint in a POST body; never include them in logs/errors.
export function refreshUserLocation({ requestPermission = false, force = false } = {}): Promise<void> {
  if (pending) return pending;
  pending = acquireLocation(requestPermission, force).finally(() => { pending = null; });
  return pending;
}

async function acquireLocation(requestPermission: boolean, force: boolean) {
  const started = Date.now();
  let phase = "permission";
  let timer: ReturnType<typeof setTimeout> | undefined;
  const controller = new AbortController();
  try {
    let permission = await Location.getForegroundPermissionsAsync();
    if (!permission.granted && requestPermission && permission.canAskAgain) {
      update({ status: "loading", position: null, canAskAgain: true, message: "等待定位授权…" });
      console.info("[user-location] requesting foreground permission");
      permission = await Location.requestForegroundPermissionsAsync();
    }
    if (!permission.granted) {
      update({
        status: permission.status === "denied" ? "denied" : "idle",
        position: null, canAskAgain: permission.canAskAgain,
        message: permission.status === "denied" ? "未开启定位，暂时无法显示距离" : "开启定位，查看距离",
      });
      console.info("[user-location] permission unavailable", { status: permission.status, canAskAgain: permission.canAskAgain });
      return;
    }
    if (!force && state.position && Date.now() - state.position.capturedAt < LOCATION_MAX_AGE_MS) {
      console.info("[user-location] reusing recent location");
      return;
    }
    update({ status: "loading", position: null, canAskAgain: true, message: "正在获取位置…" });
    phase = "device";
    console.info("[user-location] location requested");
    if (!await Location.hasServicesEnabledAsync()) {
      update({ status: "error", position: null, canAskAgain: true, message: "请开启设备定位服务后重试" });
      console.info("[user-location] device location services disabled");
      return;
    }
    // expo-location forwards these Web Geolocation options to the browser. Its
    // default maximumAge is Infinity, which would otherwise reuse stale fixes.
    const options = {
      accuracy: Location.Accuracy.High, mayShowUserSettingsDialog: requestPermission,
      maximumAge: 0, timeout: LOCATION_TIMEOUT_MS,
    };
    const device = await Promise.race([
      Location.getCurrentPositionAsync(options),
      new Promise<never>((_, reject) => {
        timer = setTimeout(() => reject(new Error("location_timeout")), LOCATION_TIMEOUT_MS);
      }),
    ]);
    clearTimeout(timer);
    timer = undefined;
    // Approximate OS location can be kilometres off; don't present it as a precise distance.
    if (device.coords.accuracy === null || device.coords.accuracy > 200) {
      update({ status: "error", position: null, canAskAgain: true, message: "定位精度不足，请开启精确位置或移至信号较好的位置后重试" });
      console.info("[user-location] insufficient location accuracy");
      return;
    }
    phase = "conversion";
    timer = setTimeout(() => controller.abort(), LOCATION_TIMEOUT_MS);
    const converted = await requestJson<ConvertCoordinatesRequest, ConvertCoordinatesResp>("/maps/coordinates/convert", {
      method: "POST", signal: controller.signal,
      body: { latitude: device.coords.latitude, longitude: device.coords.longitude },
    });
    if (!Number.isFinite(converted.latitude) || !Number.isFinite(converted.longitude)
      || Math.abs(converted.latitude) > 90 || Math.abs(converted.longitude) > 180) {
      throw new Error("invalid_coordinates");
    }
    const capturedAt = device.timestamp;
    if (!Number.isFinite(capturedAt) || Date.now() - capturedAt >= LOCATION_MAX_AGE_MS || capturedAt > Date.now()) {
      throw new Error("stale_location");
    }
    update({
      status: "ready", canAskAgain: true, message: "已显示直线距离",
      position: { ...converted, coordinateSystem: "GCJ-02", capturedAt },
    });
    console.info("[user-location] location ready", { elapsedMs: Date.now() - started });
  } catch (error) {
    const reason = error instanceof ApiRequestError ? "conversion_http_error"
      : controller.signal.aborted ? "conversion_timeout"
        : error instanceof Error && ["location_timeout", "invalid_coordinates", "stale_location"].includes(error.message)
          ? error.message : "location_unavailable";
    console.warn("[user-location] location failed", { phase, reason, elapsedMs: Date.now() - started });
    update({
      status: "error", position: null, canAskAgain: true,
      message: error instanceof ApiRequestError && error.status === 503
        ? "距离服务暂不可用，请稍后重试"
        : phase === "conversion" ? "距离计算失败，请重试" : "定位失败，请检查权限和定位服务后重试",
    });
  } finally {
    clearTimeout(timer);
  }
}
