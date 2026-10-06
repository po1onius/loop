/* global AMap */
(() => {
  "use strict";
  const el = (id) => document.getElementById(id);
  let initial = {};
  try {
    const value = JSON.parse(decodeURIComponent(location.hash.slice(1)) || "{}");
    if (value && typeof value === "object") initial = value;
  }
  catch { /* A direct browser visit starts without an existing selection. */ }
  const channel = typeof initial.channel === "string" ? initial.channel : "";
  const parentOrigin = (() => {
    try { return new URL(initial.parentOrigin).origin; } catch { return null; }
  })();
  let map;
  let geocoder;
  let places;
  let selected = null;
  let revision = 0;
  let movingTo = null;
  let debounce;
  let sdkTimer;
  const status = (text) => { el("status").textContent = text; };
  function send(type, data) {
    const message = { type, channel, ...data };
    if (window.ReactNativeWebView) window.ReactNativeWebView.postMessage(JSON.stringify(message));
    else if (parentOrigin && window.parent !== window) window.parent.postMessage(message, parentOrigin);
  }
  function log(event, extra = {}) {
    console.info("[map-picker]", event, extra);
    send("log", { event, extra });
  }
  function failure(stage, text, result) {
    status(text);
    const info = typeof result === "string" ? result : result?.info;
    const code = typeof info === "string" && /^[A-Za-z0-9_ -]{1,80}$/.test(info) ? info : null;
    log("failed", { stage, code });
  }
  function coordinates(value) {
    const lng = value?.getLng ? value.getLng() : value?.lng;
    const lat = value?.getLat ? value.getLat() : value?.lat;
    return Number.isFinite(lng) && Number.isFinite(lat) && Math.abs(lng) <= 180 && Math.abs(lat) <= 90
      ? { lng, lat } : null;
  }
  const same = (a, b) => a && b && Math.abs(a.lng - b.lng) < 0.000001 && Math.abs(a.lat - b.lat) < 0.000001;
  const text = (value) => typeof value === "string" ? value.trim() : "";
  function validPlace(value) {
    return value && coordinates(value) && text(value.name) && text(value.address)
      && [...value.name].length <= 80 && [...value.address].length <= 200
      && (value.poiId === null || (typeof value.poiId === "string" && value.poiId.length <= 80))
      && value.provider === "amap" && value.coordinateSystem === "GCJ-02";
  }
  function clearSelection() {
    selected = null;
    el("confirm").disabled = true;
    el("name").textContent = "请选择活动地点";
    el("address").textContent = "搜索地点，或拖动地图选择位置";
  }
  function choose(place, recenter = true) {
    if (!validPlace(place)) { failure("invalid_place", "地点信息不完整，请选择其他地点"); return; }
    revision += 1;
    clearTimeout(debounce);
    selected = place;
    el("name").textContent = place.name;
    el("address").textContent = place.address;
    el("confirm").disabled = false;
    if (recenter && !same(place, coordinates(map.getCenter()))) {
      movingTo = { lng: place.lng, lat: place.lat };
      map.setCenter([place.lng, place.lat], true);
    }
    status("已选择地点，可以确认或继续调整");
    log("selected", { hasPoi: Boolean(place.poiId) });
  }
  function poiPlace(poi) {
    const point = coordinates(poi.location);
    if (!point) return null;
    const address = [poi.pname, poi.cityname, poi.adname, poi.address].map(text).filter(Boolean).join("");
    return { ...point, name: text(poi.name), address, poiId: text(poi.id) || null, provider: "amap", coordinateSystem: "GCJ-02" };
  }
  function showResults(pois, prefix = "") {
    el("results").replaceChildren();
    for (const poi of pois) {
      const place = poiPlace(poi);
      if (!place) continue;
      if (prefix && !place.address.startsWith(prefix)) place.address = prefix + place.address;
      if (!validPlace(place)) continue;
      const button = document.createElement("button");
      const name = document.createElement("span");
      const address = document.createElement("small");
      name.textContent = place.name;
      address.textContent = place.address;
      button.append(name, address);
      button.addEventListener("click", () => choose(place));
      el("results").append(button);
    }
  }
  function resolveCenter() {
    const point = coordinates(map.getCenter());
    if (!point) return;
    const request = ++revision;
    clearSelection();
    el("results").replaceChildren();
    status("正在查询该位置和附近地点…");
    geocoder.getAddress([point.lng, point.lat], (resultStatus, result) => {
      if (request !== revision) return;
      const address = text(result?.regeocode?.formattedAddress);
      if (resultStatus !== "complete" || !address) {
        failure("reverse_geocode", "未能获取该位置地址，请重新选点或搜索地点", result); return;
      }
      choose({ ...point, name: "地图选点", address, poiId: null, provider: "amap", coordinateSystem: "GCJ-02" }, false);
      const part = result.regeocode.addressComponent;
      const prefix = part ? [...new Set([part.province, part.city, part.district].map(text).filter(Boolean))].join("") : "";
      showResults(result.regeocode.pois || [], prefix);
      log("reverse_geocode.completed", { count: result.regeocode.pois?.length || 0 });
    });
  }
  function search() {
    if (!places) return;
    clearTimeout(debounce);
    const keyword = el("query").value.trim();
    if (!keyword) { resolveCenter(); return; }
    const request = ++revision;
    clearSelection();
    el("results").replaceChildren();
    places.setCity(el("city").value.trim() || "全国");
    status("正在搜索地点…");
    places.search(keyword, (resultStatus, result) => {
      if (request !== revision) return;
      const pois = result?.poiList?.pois || [];
      if (resultStatus === "no_data" || (resultStatus === "complete" && !pois.length)) {
        status("没有找到地点，请调整关键词或城市"); return;
      }
      if (resultStatus !== "complete") { failure("search", "搜索失败，请重试", result); return; }
      showResults(pois);
      status("点击搜索结果选择地点");
      log("search.completed", { count: pois.length });
    });
  }
  el("search").addEventListener("submit", (event) => { event.preventDefault(); search(); });
  for (const id of ["query", "city"]) el(id).addEventListener("input", () => {
    revision += 1;
    clearSelection();
    el("results").replaceChildren();
    clearTimeout(debounce);
    debounce = setTimeout(search, 400);
  });
  el("confirm").addEventListener("click", () => {
    if (validPlace(selected)) { log("confirmed", { hasPoi: Boolean(selected.poiId) }); send("selected", { location: selected }); }
  });
  function locate() {
    if (!navigator.geolocation || !window.isSecureContext) {
      failure("geolocation_unavailable", "当前环境无法定位，请搜索地点或拖动地图"); return;
    }
    const request = ++revision;
    clearSelection();
    el("results").replaceChildren();
    clearTimeout(debounce);
    status("正在获取当前位置…");
    log("geolocation.requested");
    navigator.geolocation.getCurrentPosition((position) => {
      if (request !== revision) return;
      AMap.convertFrom([position.coords.longitude, position.coords.latitude], "gps", (resultStatus, result) => {
        if (request !== revision) return;
        const point = coordinates(result?.locations?.[0]);
        if (resultStatus !== "complete" || !point) { failure("coordinate_conversion", "位置转换失败，请重试或搜索地点", result); return; }
        // GPS is WGS-84; only converted GCJ-02 coordinates enter the map and event payload.
        if (same(point, coordinates(map.getCenter()))) resolveCenter();
        else map.setCenter([point.lng, point.lat], true);
        log("geolocation.completed");
      });
    }, (error) => {
      if (request !== revision) return;
      failure("geolocation", error.code === 1 ? "未获得定位权限，仍可搜索地点或拖动地图" : "定位失败，请重试或搜索地点");
    }, { enableHighAccuracy: true, timeout: 10000, maximumAge: 60000 });
  }
  window.loopMapLocate = locate;
  window.loopMapLocationDenied = () => failure("geolocation_permission", "未获得精确定位权限，仍可搜索地点或拖动地图");
  el("locate").addEventListener("click", () => {
    if (!window.isSecureContext) { failure("geolocation_unavailable", "当前环境无法定位，请搜索地点或拖动地图"); return; }
    if (window.ReactNativeWebView) send("request_location", {});
    else locate();
  });
  window._AMapSecurityConfig = { serviceHost: new URL("./_AMapService", location.href).href };
  const script = document.createElement("script");
  script.src = "https://webapi.amap.com/maps?v=2.0&key=" + encodeURIComponent(document.body.dataset.amapKey)
    + "&plugin=AMap.PlaceSearch,AMap.Geocoder";
  script.onerror = () => { clearTimeout(sdkTimer); failure("sdk_load", "地图加载失败，请关闭后重试"); };
  script.onload = () => {
    clearTimeout(sdkTimer);
    try {
      const existing = validPlace(initial.location) ? initial.location : null;
      map = new AMap.Map("map", { zoom: existing ? 16 : 11, center: existing ? [existing.lng, existing.lat] : [116.397428, 39.90923] });
      geocoder = new AMap.Geocoder({ extensions: "all", radius: 1000 });
      places = new AMap.PlaceSearch({ pageSize: 20, extensions: "all" });
      map.on("dragstart", () => { movingTo = null; revision += 1; clearSelection(); clearTimeout(debounce); });
      map.on("mapmove", () => {
        if (movingTo) return;
        if (selected && same(selected, coordinates(map.getCenter()))) return;
        revision += 1;
        clearSelection();
        clearTimeout(debounce);
      });
      map.on("moveend", () => {
        if (movingTo && same(movingTo, coordinates(map.getCenter()))) { movingTo = null; return; }
        movingTo = null;
        if (selected && same(selected, coordinates(map.getCenter()))) return;
        clearTimeout(debounce);
        debounce = setTimeout(resolveCenter, 250);
      });
      el("search-button").disabled = false;
      el("locate").disabled = false;
      if (existing) choose(existing, false);
      else status("请输入城市和地点，或拖动地图选点");
      log("ready", { restored: Boolean(existing) });
      send("ready", {});
    } catch { failure("initialization", "地图初始化失败，请关闭后重试"); }
  };
  sdkTimer = setTimeout(() => failure("sdk_timeout", "地图加载超时，请检查网络后重试"), 20000);
  document.head.append(script);
  window.addEventListener("pagehide", () => { revision += 1; clearTimeout(debounce); clearTimeout(sdkTimer); map?.destroy(); });
})();
