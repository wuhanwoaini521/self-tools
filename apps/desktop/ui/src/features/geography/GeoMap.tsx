import { MapPin } from "@phosphor-icons/react";
import { lazy, memo, Suspense, useMemo, useState } from "react";
import type { GeoMapLine, GeoMapPoint, GeoRelationKind } from "../../types";

const MapLibreMap = lazy(() => import("./MapLibreMap").then((module) => ({ default: module.MapLibreMap })));

export type GeoMapLayer = "base" | "administrative" | "terrain" | "river";

interface GeoMapProps {
  points: GeoMapPoint[];
  lines: GeoMapLine[];
  layer: GeoMapLayer;
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

export const GeoMap = memo(function GeoMap({ points, lines, layer, onSelect, onLayerChange }: GeoMapProps) {
  const [mapLibreError, setMapLibreError] = useState(false);
  const visibleLines = useMemo(() => layer === "base" ? [] : lines.filter((line) => layerMatchesRelation[layer].includes(line.kind)), [layer, lines]);

  return <section className="geo-map-card" aria-label="地理探索画布">
    <header className="geo-map-head">
      <div><span className="geo-eyebrow">EXPLORATION CANVAS</span><h2>把世界换一片观察镜片</h2></div>
      <div className="geo-map-controls" aria-label="地图图层">
        {LAYERS.map((item) => <button type="button" key={item.id} className={layer === item.id ? "selected" : ""} onClick={() => onLayerChange(item.id)}>{item.label}</button>)}
      </div>
    </header>
    <div className="geo-map-viewport">
      {mapLibreError ? <div className="geo-map-error-state" role="alert">在线地图暂时无法加载，请检查网络连接后重新进入 Geography 页面。</div> : <Suspense fallback={<div className="geo-maplibre-loading">正在载入 MapLibre 地图模块…</div>}><MapLibreMap points={points} lines={lines} layer={layer} onSelect={onSelect} onError={() => setMapLibreError(true)} /></Suspense>}
      <div className="geo-map-legend"><span><i className="place" />地点</span><span><i className="terrain" />地形地貌</span><span><i className="river" />河流水系</span></div>
    </div>
    <footer className="geo-map-foot"><span><MapPin size={14} /> 点击实体继续探索</span><span>{mapLibreError ? "地图未连接" : "MapLibre · Mapterhorn DEM"} · {points.length} 个本地实体 · {visibleLines.length} 条关系</span></footer>
  </section>;
});
