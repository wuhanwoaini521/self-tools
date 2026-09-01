import { useEffect, useRef, useState } from "react";
import type { GeoMapLine, GeoMapPoint, GeoRelationKind } from "../../types";

interface AmapMapProps {
  apiKey: string;
  securityJsCode: string;
  points: GeoMapPoint[];
  lines: GeoMapLine[];
  layer: "base" | "administrative" | "terrain" | "river";
  onSelect: (id: string) => void;
}

interface AmapOverlay {
  on: (event: string, handler: () => void) => void;
}

interface AmapImageLayer extends AmapOverlay {}

interface AmapMapInstance {
  add: (overlays: AmapOverlay | AmapOverlay[]) => void;
  remove: (overlays: AmapOverlay | AmapOverlay[]) => void;
  clearMap: () => void;
  destroy: () => void;
}

interface AmapNamespace {
  Map: new (container: HTMLDivElement, options: Record<string, unknown>) => AmapMapInstance;
  Marker: new (options: Record<string, unknown>) => AmapOverlay;
  Polyline: new (options: Record<string, unknown>) => AmapOverlay;
  ImageLayer: new (options: Record<string, unknown>) => AmapImageLayer;
}

declare global {
  interface Window {
    AMap?: AmapNamespace;
    _AMapSecurityConfig?: { securityJsCode?: string };
  }
}

const AMAP_SCRIPT_ID = "devtoolbox-amap-js-api";

export function loadAmap(apiKey: string, securityJsCode: string) {
  if (!apiKey.trim()) return Promise.reject(new Error("请先填写 Geography 的高德 JS API Key"));
  if (!securityJsCode.trim()) return Promise.reject(new Error("请先填写 Geography 的 securityJsCode"));
  if (window.AMap) return Promise.resolve(window.AMap);
  return new Promise<AmapNamespace>((resolve, reject) => {
    const callbackName = `__devtoolbox_amap_ready_${Date.now()}`;
    const globalWindow = window as unknown as Window & Record<string, unknown>;
    globalWindow._AMapSecurityConfig = { securityJsCode };
    globalWindow[callbackName] = () => {
      delete globalWindow[callbackName];
      if (window.AMap) resolve(window.AMap);
      else reject(new Error("高德 JS API 已加载，但未找到 AMap 对象"));
    };
    const script = document.createElement("script");
    script.id = AMAP_SCRIPT_ID;
    script.async = true;
    script.src = `https://webapi.amap.com/maps?v=2.0&key=${encodeURIComponent(apiKey)}&callback=${callbackName}`;
    script.onerror = () => {
      delete globalWindow[callbackName];
      reject(new Error("高德 JS API 加载失败，请检查 Key、Security Code 或网络连接"));
    };
    document.head.appendChild(script);
  });
}

function relationLinesForLayer(lines: GeoMapLine[], layer: AmapMapProps["layer"]) {
  const matches: Record<Exclude<AmapMapProps["layer"], "base">, GeoRelationKind[]> = {
    administrative: ["LOCATED_IN", "PART_OF", "BORDERS"],
    terrain: [],
    river: ["FLOWS_THROUGH", "SOURCE_OF", "FLOWS_INTO", "CROSSES"],
  };
  return layer === "base" ? [] : lines.filter((line) => matches[layer].includes(line.kind));
}

export function AmapMap({ apiKey, securityJsCode, points, lines, layer, onSelect }: AmapMapProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const mapRef = useRef<AmapMapInstance | null>(null);
  const amapRef = useRef<AmapNamespace | null>(null);
  const overlaysRef = useRef<AmapOverlay[]>([]);
  const terrainLayerRef = useRef<AmapImageLayer | null>(null);
  const [mapReady, setMapReady] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void loadAmap(apiKey, securityJsCode)
      .then((amap) => {
        if (cancelled || !containerRef.current) return;
        amapRef.current = amap;
        mapRef.current = new amap.Map(containerRef.current, {
          center: [105, 32],
          zoom: 3,
          viewMode: "2D",
          resizeEnable: true,
          dragEnable: true,
          zoomEnable: true,
          scrollWheel: true,
          doubleClickZoom: true,
          keyboardEnable: true,
        });
        setMapReady(true);
        setLoading(false);
        setError(null);
      })
      .catch((reason: unknown) => {
        if (!cancelled) {
          setLoading(false);
          setError(reason instanceof Error ? reason.message : "高德 JS API 加载失败");
        }
      });
    return () => {
      cancelled = true;
      mapRef.current?.destroy();
      mapRef.current = null;
      amapRef.current = null;
      overlaysRef.current = [];
      terrainLayerRef.current = null;
      setMapReady(false);
    };
  }, [apiKey, securityJsCode]);

  useEffect(() => {
    const map = mapRef.current;
    const amap = amapRef.current;
    if (!map || !amap || !mapReady) return;

    if (terrainLayerRef.current) {
      map.remove(terrainLayerRef.current);
      terrainLayerRef.current = null;
    }
    if (layer !== "terrain") return;

    // 内置的 Natural Earth 高程晕渲图经过 Web Mercator 重投影，随缩放和平移对齐。
    // 高德只承担地图交互，不再使用手绘地貌多边形或不稳定的第三方地形瓦片。
    const terrainLayer = new amap.ImageLayer({
      url: new URL("geography/natural-earth-50m-hypso-mercator.jpg", window.location.href).href,
      bounds: [-180, -85.05112878, 180, 85.05112878],
      zooms: [2, 6],
      opacity: 1,
      zIndex: 20,
    });
    terrainLayerRef.current = terrainLayer;
    map.add(terrainLayer);
    return () => {
      if (terrainLayerRef.current === terrainLayer) {
        map.remove(terrainLayer);
        terrainLayerRef.current = null;
      }
    };
  }, [layer, mapReady]);

  useEffect(() => {
    const map = mapRef.current;
    const amap = amapRef.current;
    if (!map || !amap || !mapReady) return;
    if (overlaysRef.current.length) map.remove(overlaysRef.current);

    const overlays: AmapOverlay[] = points.map((point) => {
      const marker = new amap.Marker({
        position: [point.coordinate.longitude, point.coordinate.latitude],
        title: point.name,
        label: { content: point.name, direction: "right" },
      });
      marker.on("click", () => onSelect(point.entity_id));
      return marker;
    });
    const visibleLines = relationLinesForLayer(lines, layer);
    visibleLines.forEach((line) => {
      overlays.push(new amap.Polyline({
        path: [[line.from.longitude, line.from.latitude], [line.to.longitude, line.to.latitude]],
        strokeColor: layer === "river" ? "#54c0e8" : layer === "terrain" ? "#d0ad67" : "#7fb3d5",
        strokeWeight: layer === "river" ? 5 : 3,
        strokeOpacity: 0.82,
        strokeStyle: layer === "terrain" ? "dashed" : "solid",
      }));
    });
    overlaysRef.current = overlays;
    if (overlays.length) map.add(overlays);
    return () => {
      if (overlaysRef.current === overlays) {
        map.remove(overlays);
        overlaysRef.current = [];
      }
    };
  }, [layer, lines, mapReady, onSelect, points]);

  return <div className="geo-amap-wrap">
    <div ref={containerRef} className="geo-amap-map" role="img" aria-label="高德动态地理地图" />
    {error ? <div className="geo-amap-error" role="alert">{error}</div> : loading ? <div className="geo-amap-hint">正在加载高德动态地图…</div> : <div className="geo-amap-hint">{layer === "terrain" ? "自然地形图 · 等高线 · 山体晕渲 · 水系" : "滚轮缩放 · 按住左键拖动地图"}</div>}
    {layer === "terrain" && !loading && !error ? <div className="geo-terrain-attribution">地形晕渲：Natural Earth 1:50m · 主要高程数据源：NASA SRTM Plus</div> : null}
  </div>;
}
