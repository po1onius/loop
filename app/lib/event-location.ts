import type { EventResp, UpdateEventDraftRequest } from "@/lib/dto";

export type MapLocation = {
  name: string;
  address: string;
  lat: number;
  lng: number;
  provider: "amap";
  coordinateSystem: "GCJ-02";
  poiId: string | null;
};

export function isMapLocation(value: unknown): value is MapLocation {
  if (!value || typeof value !== "object") return false;
  const p = value as Record<string, unknown>;
  return typeof p["name"] === "string" && !!p["name"].trim() && [...p["name"]].length <= 80
    && typeof p["address"] === "string" && !!p["address"].trim() && [...p["address"]].length <= 200
    && typeof p["lat"] === "number" && Number.isFinite(p["lat"]) && Math.abs(p["lat"]) <= 90
    && typeof p["lng"] === "number" && Number.isFinite(p["lng"]) && Math.abs(p["lng"]) <= 180
    && p["provider"] === "amap" && p["coordinateSystem"] === "GCJ-02"
    && (p["poiId"] === null || (typeof p["poiId"] === "string" && p["poiId"].length <= 80));
}

export function locationFromEvent(event: EventResp): MapLocation | null {
  const location = {
    name: event.location_name, address: event.location_address,
    lat: event.location_latitude, lng: event.location_longitude,
    provider: event.location_provider, coordinateSystem: event.location_coordinate_system,
    poiId: event.location_poi_id,
  };
  return isMapLocation(location) ? location : null;
}

type LocationFields = Pick<UpdateEventDraftRequest,
  "location_name" | "location_address" | "location_latitude" | "location_longitude"
  | "location_provider" | "location_coordinate_system" | "location_poi_id" | "location_note">;

export function eventLocationFields(location: MapLocation | null, note: string): LocationFields {
  return {
    location_name: location?.name ?? null,
    location_address: location?.address ?? null,
    location_latitude: location?.lat ?? null,
    location_longitude: location?.lng ?? null,
    location_provider: location?.provider ?? null,
    location_coordinate_system: location?.coordinateSystem ?? null,
    location_poi_id: location?.poiId ?? null,
    location_note: location ? note.trim() || null : null,
  };
}

export function mapNavigationUrl(location: MapLocation): string {
  // AMap navigation URI uses GCJ-02 coordinates, longitude first.
  return `https://uri.amap.com/navigation?to=${location.lng},${location.lat},${encodeURIComponent(location.name)}&mode=car&src=loop&callnative=1`;
}
