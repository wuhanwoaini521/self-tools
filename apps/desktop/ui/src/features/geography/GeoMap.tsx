import { MapPin } from "@phosphor-icons/react";
import { lazy, memo, Suspense, useEffect, useMemo, useRef, useState } from "react";
import type { GeoMapLine, GeoMapPoint, GeoRelationKind } from "../../types";

const MapLibreMap = lazy(() => import("./MapLibreMap").then((module) => ({ default: module.MapLibreMap })));

export type GeoMapLayer = "base" | "administrative" | "terrain" | "river";

interface GeoMapProps {
  points: GeoMapPoint[];
  lines: GeoMapLine[];
  layer: GeoMapLayer;
  active: boolean;
  onSelect: (id: string) => void;
  onLayerChange: (layer: GeoMapLayer) => void;
}

const LAYERS: { id: GeoMapLayer; label: string }[] = [
  { id: "base", label: "基础" },
  { id: "administrative", label: "行政" },
  { id: "terrain", label: "3D 地形" },
  { id: "river", label: "河流" },
];

const layerMatchesRelation: Record<GeoMapLayer, GeoRelationKind[]> = {
  base: [],
  administrative: ["LOCATED_IN", "PART_OF", "BORDERS"],
  terrain: [],
  river: ["FLOWS_THROUGH", "SOURCE_OF", "FLOWS_INTO", "CROSSES"],
};

interface MapViewpoint {
  entityId: string | null;
  center: [number, number];
}

function randomViewpoint(points: GeoMapPoint[], previousEntityId: string | null): MapViewpoint {
  const candidates = points.filter(
    (point) =>
      Number.isFinite(point.coordinate.longitude) &&
      Number.isFinite(point.coordinate.latitude),
  );
  const alternatives = candidates.filter((point) => point.entity_id !== previousEntityId);
  const choices = alternatives.length > 0 ? alternatives : candidates;
  const point = choices[Math.floor(Math.random() * choices.length)];

  return point
    ? {
        entityId: point.entity_id,
        center: [point.coordinate.longitude, point.coordinate.latitude],
      }
    : { entityId: null, center: [105, 32] };
}

export const GeoMap = memo(function GeoMap({ points, lines, layer, active, onSelect, onLayerChange }: GeoMapProps) {
  const [mapLibreError, setMapLibreError] = useState(false);
  const [viewpoint, setViewpoint] = useState(() => randomViewpoint(points, null));
  const wasActiveRef = useRef(active);
  const visibleLines = useMemo(() => layer === "base" ? [] : lines.filter((line) => layerMatchesRelation[layer].includes(line.kind)), [layer, lines]);

  // 地理页再次打开时，换一个首页实体作为观察起点，避免总是停在固定区域。
  useEffect(() => {
    const currentPointExists = points.some(
      (point) => point.entity_id === viewpoint.entityId,
    );
    if (active && (!wasActiveRef.current || !currentPointExists)) {
      setViewpoint((current) => randomViewpoint(points, current.entityId));
    }
    wasActiveRef.current = active;
  }, [active, points, viewpoint.entityId]);

  return <section className="geo-map-card" aria-label="地理探索画布">
    <header className="geo-map-head">
      <div><span className="geo-eyebrow">EXPLORATION CANVAS</span><h2>把世界换一片观察镜片</h2></div>
      <div className="geo-map-controls" aria-label="地图图层">
        {LAYERS.map((item) => <button type="button" key={item.id} className={layer === item.id ? "selected" : ""} onClick={() => onLayerChange(item.id)}>{item.label}</button>)}
      </div>
    </header>
    <div className="geo-map-viewport">
      {mapLibreError ? <div className="geo-map-error-state" role="alert">在线地图暂时无法加载，请检查网络连接后重新进入 Geography 页面。</div> : <Suspense fallback={<div className="geo-maplibre-loading">正在载入 MapLibre 地图模块…</div>}><MapLibreMap points={points} lines={lines} layer={layer} onSelect={onSelect} onError={() => setMapLibreError(true)} center={viewpoint.center} /></Suspense>}
      <div className="geo-map-legend"><span><i className="place" />地点</span><span><i className="terrain" />地形地貌</span><span><i className="river" />河流水系</span></div>
    </div>
    <footer className="geo-map-foot"><span><MapPin size={14} /> 点击实体继续探索</span><span>{mapLibreError ? "地图未连接" : "MapLibre · Mapterhorn DEM"} · {points.length} 个本地实体 · {visibleLines.length} 条关系</span></footer>
  </section>;
});
