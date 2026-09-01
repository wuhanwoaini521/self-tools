import { lazy, Suspense, useEffect, useMemo, useRef, useState } from "react";
import type { GeoEntity, GeoEntityType, GeoMapPoint, GeoRelationView } from "../../types";
import { loadAmap, loadAmapPlugin, type AmapDistrict, type AmapMapInstance, type AmapNamespace, type AmapOverlay } from "./AmapMap";

const MapLibreMap = lazy(() => import("./MapLibreMap").then((module) => ({ default: module.MapLibreMap })));

interface AmapRegionMapProps {
  item: GeoEntity;
  relations?: GeoRelationView[];
  apiKey?: string | null;
  securityJsCode?: string | null;
  onSelect?: (id: string) => void;
}

const REGION_TYPES: GeoEntityType[] = ["country", "province", "region", "city"];

function normalizeDistrictName(name: string) {
  return name.replace(/(特别行政区|自治区|自治州|地区|盟|省|市)$/u, "").trim();
}

function districtMatches(itemName: string, districtName: string | undefined) {
  if (!districtName) return false;
  const item = normalizeDistrictName(itemName);
  const district = normalizeDistrictName(districtName);
  return item === district || item.includes(district) || district.includes(item);
}

function initialZoom(type: GeoEntityType) {
  if (type === "country") return 4;
  if (type === "province" || type === "region") return 6;
  return 9;
}

function terrainZoom(type: GeoEntityType) {
  if (type === "country") return 4.4;
  if (type === "province" || type === "region") return 5.7;
  if (type === "city") return 9;
  return 5.2;
}

function terrainPoints(item: GeoEntity, relations: GeoRelationView[]): GeoMapPoint[] {
  const entities = [item, ...relations.map((relation) => relation.entity)];
  return entities
    .filter((entity): entity is GeoEntity & { coordinates: NonNullable<GeoEntity["coordinates"]> } => Boolean(entity.coordinates))
    .filter((entity, index, all) => all.findIndex((candidate) => candidate.id === entity.id) === index)
    .slice(0, 9)
    .map((entity) => ({ entity_id: entity.id, name: entity.name, entity_type: entity.entity_type, coordinate: entity.coordinates }));
}

function queryDistrict(amap: AmapNamespace, item: GeoEntity) {
  return loadAmapPlugin(amap, ["AMap.DistrictSearch"]).then(() => {
    if (!amap.DistrictSearch) throw new Error("高德行政区查询插件加载失败");
    return new Promise<AmapDistrict>((resolve, reject) => {
      const search = new amap.DistrictSearch!({ subdistrict: 0, extensions: "all", showbiz: false });
      search.search(item.name, (status, result) => {
        if (status !== "complete" || !result.districtList?.length) {
          reject(new Error(`未找到“${item.name}”的行政区域边界`));
          return;
        }
        resolve(result.districtList.find((district) => districtMatches(item.name, district.name)) ?? result.districtList[0]);
      });
    });
  });
}

function addMarker(amap: AmapNamespace, item: GeoEntity, isPrimary: boolean, onSelect?: (id: string) => void) {
  if (!item.coordinates) return null;
  const marker = new amap.Marker({
    position: [item.coordinates.longitude, item.coordinates.latitude],
    title: item.name,
    label: { content: isPrimary ? item.name : `${item.name} · 关联`, direction: "right" },
  });
  if (onSelect) marker.on("click", () => onSelect(item.id));
  return marker;
}

export function AmapRegionMap({ item, relations = [], apiKey, securityJsCode, onSelect }: AmapRegionMapProps) {
  const shellRef = useRef<HTMLElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const mapRef = useRef<AmapMapInstance | null>(null);
  const amapRef = useRef<AmapNamespace | null>(null);
  const overlaysRef = useRef<AmapOverlay[]>([]);
  const [mapReady, setMapReady] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [mapView, setMapView] = useState<"terrain" | "administrative">("terrain");
  const hasConfig = Boolean(apiKey?.trim() && securityJsCode?.trim());
  const canShowRegion = REGION_TYPES.includes(item.entity_type);
  const canShowMap = Boolean(item.coordinates) || canShowRegion;
  const [isVisible, setIsVisible] = useState(!hasConfig);
  const mapPoints = useMemo(() => terrainPoints(item, relations), [item, relations]);

  useEffect(() => {
    setMapView("terrain");
  }, [item.id]);

  useEffect(() => {
    if (!hasConfig || !canShowMap || isVisible) return;
    const shell = shellRef.current;
    if (!shell || !("IntersectionObserver" in window)) {
      setIsVisible(true);
      return;
    }
    const observer = new IntersectionObserver(([entry]) => {
      if (entry.isIntersecting) {
        setIsVisible(true);
        observer.disconnect();
      }
    }, { rootMargin: "240px" });
    observer.observe(shell);
    return () => observer.disconnect();
  }, [canShowMap, hasConfig, isVisible]);

  useEffect(() => {
    if (!hasConfig || !canShowMap || !isVisible || mapView !== "administrative") return;
    let cancelled = false;
    setLoading(true);
    setError(null);
    void loadAmap(apiKey!.trim(), securityJsCode!.trim())
      .then((amap) => {
        if (cancelled || !containerRef.current) return;
        amapRef.current = amap;
        mapRef.current = new amap.Map(containerRef.current, {
          center: item.coordinates ? [item.coordinates.longitude, item.coordinates.latitude] : [116.397428, 39.90923],
          zoom: initialZoom(item.entity_type),
          viewMode: "2D",
          resizeEnable: true,
          dragEnable: true,
          zoomEnable: true,
          scrollWheel: true,
          doubleClickZoom: true,
          keyboardEnable: true,
        });
        setMapReady(true);
      })
      .catch((reason: unknown) => {
        if (!cancelled) {
          setLoading(false);
          setError(reason instanceof Error ? reason.message : "高德动态地图加载失败");
        }
      });
    return () => {
      cancelled = true;
      mapRef.current?.destroy();
      mapRef.current = null;
      amapRef.current = null;
      overlaysRef.current = [];
      setMapReady(false);
    };
  }, [apiKey, canShowMap, hasConfig, isVisible, item.coordinates, item.entity_type, item.name, mapView, securityJsCode]);

  useEffect(() => {
    const map = mapRef.current;
    const amap = amapRef.current;
    if (!map || !amap || !mapReady) return;
    let cancelled = false;
    setLoading(true);
    setError(null);
    if (overlaysRef.current.length) {
      map.remove(overlaysRef.current);
      overlaysRef.current = [];
    }

    const primaryMarker = addMarker(amap, item, true, onSelect);
    const relatedMarkers = relations
      .map((relation) => relation.entity)
      .filter((entity, index, entities) => entity.id !== item.id && entities.findIndex((candidate) => candidate.id === entity.id) === index)
      .slice(0, 8)
      .map((entity) => addMarker(amap, entity, false, onSelect))
      .filter((marker): marker is AmapOverlay => marker !== null);
    const markers = primaryMarker ? [primaryMarker, ...relatedMarkers] : relatedMarkers;

    const renderOverlays = (boundaryOverlays: AmapOverlay[] = []) => {
      if (cancelled) return;
      const overlays = [...boundaryOverlays, ...markers];
      overlaysRef.current = overlays;
      if (overlays.length) map.add(overlays);
      if (boundaryOverlays.length) map.setFitView(boundaryOverlays, false, [36, 36, 36, 36], item.entity_type === "city" ? 12 : undefined);
      else if (markers.length) map.setFitView(markers, false, [48, 48, 48, 48], 12);
      setLoading(false);
    };

    if (!canShowRegion) {
      renderOverlays();
      return () => { cancelled = true; };
    }

    void queryDistrict(amap, item)
      .then((district) => {
        const boundaries = (district.boundaries ?? []).filter((path) => path.length > 2);
        if (!boundaries.length) throw new Error(`“${item.name}”没有可展示的行政区域边界`);
        const polygons = boundaries.map((path) => new amap.Polygon({
          path,
          strokeColor: "#18a999",
          strokeWeight: 3,
          strokeOpacity: 0.95,
          fillColor: "#2bc4b0",
          fillOpacity: 0.16,
          cursor: "pointer",
        }));
        renderOverlays(polygons);
      })
      .catch((reason: unknown) => {
        if (!cancelled) {
          setLoading(false);
          setError(reason instanceof Error ? reason.message : "行政区域边界加载失败");
          if (markers.length) renderOverlays();
        }
      });
    return () => { cancelled = true; };
  }, [canShowRegion, item, mapReady, onSelect, relations]);

  if (!canShowMap) return null;
  const mapTitle = canShowRegion ? `${item.name}地图` : `${item.name}三维地形地图`;
  return <section ref={shellRef} className="geo-region-map-card" aria-label={mapTitle}>
    <header className="geo-region-map-head">
      <div><span className="geo-eyebrow">{mapView === "administrative" ? "REGIONAL MAP · 行政区域" : "TERRAIN MAP · 三维地形"}</span><h3>{mapTitle}</h3></div>
      <div className="geo-detail-map-tabs" role="tablist" aria-label="详情地图类型">
        <button type="button" role="tab" aria-selected={mapView === "terrain"} className={mapView === "terrain" ? "selected" : ""} onClick={() => setMapView("terrain")}>3D 地形</button>
        {canShowRegion ? <button type="button" role="tab" aria-selected={mapView === "administrative"} className={mapView === "administrative" ? "selected" : ""} onClick={() => setMapView("administrative")}>行政区域</button> : null}
      </div>
    </header>
    {mapView === "terrain" ? <div className="geo-region-map-wrap geo-detail-terrain-wrap">
      <Suspense fallback={<div className="geo-region-map-empty">正在加载 3D 地形…</div>}>
        <MapLibreMap key={item.id} points={mapPoints} lines={[]} layer="terrain" onSelect={onSelect ?? (() => undefined)} onError={() => undefined} center={item.coordinates ? [item.coordinates.longitude, item.coordinates.latitude] : [105, 32]} zoom={terrainZoom(item.entity_type)} />
      </Suspense>
    </div> : !hasConfig ? <div className="geo-region-map-empty">填写 Geography 的高德 JS API Key 和 `securityJsCode` 后，可显示真实行政边界。</div> : !isVisible ? <div className="geo-region-map-empty">滚动到这里后加载行政区域，减少详情页首次打开时的等待。</div> : <div className="geo-region-map-wrap">
      <div ref={containerRef} className="geo-region-map" role="img" aria-label={`${item.name}行政区域地图`} />
      {loading ? <div className="geo-region-map-status">正在加载{item.name}行政边界…</div> : null}
      {error ? <div className="geo-region-map-status error" role="alert">{error}</div> : null}
      {!loading && !error ? <div className="geo-region-map-attribution">边界由高德行政区划查询返回 · 仅用于地理探索</div> : null}
    </div>}
  </section>;
}
