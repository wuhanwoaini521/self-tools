import { MapPin } from "@phosphor-icons/react";
import { lazy, memo, Suspense, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { GeoMapLine, GeoMapPoint, GeoRelationKind } from "../../types";
import { ViewerErrorBoundary } from "./ViewerErrorBoundary";

const MapLibreMap = lazy(() => import("./MapLibreMap").then((module) => ({ default: module.MapLibreMap })));

export type GeoMapLayer = "base" | "administrative" | "terrain" | "river";

interface GeoMapProps {
  points: GeoMapPoint[];
  lines: GeoMapLine[];
  layer: GeoMapLayer;
  active: boolean;
  onSelect: (id: string) => void;
  onLayerChange: (layer: GeoMapLayer) => void;
  /** 当前在地图上被选中的实体；只影响高亮，不改变选择回调。 */
  selectedId?: string | null;
  /** 覆盖在地图视口上的附属 UI（例如选中实体的浮动信息卡）。 */
  overlay?: ReactNode;
  /**
   * 外部指定的观察中心（例如从探索索引「在地图查看」定位某个地点）。
   * 设置后覆盖随机起点；MapLibreMap 会在 center 变化时平滑移动。
   */
  focus?: { center: [number, number]; zoom?: number } | null;
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

export function OfflineCoordinates({
  points,
  lines,
  onSelect,
  compact = false,
  selectedId = null,
}: {
  points: GeoMapPoint[];
  lines: GeoMapLine[];
  onSelect: (id: string) => void;
  compact?: boolean;
  selectedId?: string | null;
}) {
  const width = 800;
  const height = 340;
  const padding = 38;
  const located = points.filter(
    (point) =>
      Number.isFinite(point.coordinate.longitude) &&
      Number.isFinite(point.coordinate.latitude),
  );
  const longitudes = located.map((point) => point.coordinate.longitude);
  const latitudes = located.map((point) => point.coordinate.latitude);
  const minLongitude = longitudes.length ? Math.min(...longitudes) : 0;
  const maxLongitude = longitudes.length ? Math.max(...longitudes) : 1;
  const minLatitude = latitudes.length ? Math.min(...latitudes) : 0;
  const maxLatitude = latitudes.length ? Math.max(...latitudes) : 1;
  const longitudeSpan = maxLongitude - minLongitude || 1;
  const latitudeSpan = maxLatitude - minLatitude || 1;
  const position = (longitude: number, latitude: number): [number, number] => [
    padding + ((longitude - minLongitude) / longitudeSpan) * (width - padding * 2),
    height - padding - ((latitude - minLatitude) / latitudeSpan) * (height - padding * 2),
  ];
  const pointById = new Map(
    located.map((point) => [point.entity_id, position(point.coordinate.longitude, point.coordinate.latitude)]),
  );

  return (
    <div className={`geo-offline-map${compact ? " compact" : ""}`} role="region" aria-label="离线地理资料">
      <div className="geo-offline-map-heading">
        <div>
          <strong>离线坐标与关系</strong>
          <p>在线底图不可用。这里仅呈现本地 Geography 坐标与关系，不代表导航地图或地形图。</p>
        </div>
        <span>{located.length} 个有坐标的实体 · {lines.length} 条关系</span>
      </div>
      {located.length ? (
        <svg
          className="geo-offline-coordinate-plot"
          viewBox={`0 0 ${width} ${height}`}
          role="img"
          aria-label={`离线经纬度关系图，包含 ${located.length} 个有坐标的地理实体`}
        >
          {[0, 1, 2, 3, 4].map((step) => {
            const x = padding + (step / 4) * (width - padding * 2);
            const y = padding + (step / 4) * (height - padding * 2);
            return (
              <g key={step} className="geo-offline-gridline">
                <line x1={x} y1={padding} x2={x} y2={height - padding} />
                <line x1={padding} y1={y} x2={width - padding} y2={y} />
              </g>
            );
          })}
          {lines.map((line) => {
            const from = pointById.get(line.from_id);
            const to = pointById.get(line.to_id);
            if (!from || !to) return null;
            return (
              <line
                className="geo-offline-relation-line"
                key={`${line.from_id}-${line.to_id}-${line.kind}`}
                x1={from[0]}
                y1={from[1]}
                x2={to[0]}
                y2={to[1]}
              />
            );
          })}
          {located.map((point) => {
            const [x, y] = position(point.coordinate.longitude, point.coordinate.latitude);
            return (
              <g
                className={`geo-offline-point${point.entity_id === selectedId ? " selected" : ""}`}
                key={point.entity_id}
                role="button"
                tabIndex={0}
                aria-label={`打开 ${point.name}，${point.coordinate.latitude.toFixed(2)}° N，${point.coordinate.longitude.toFixed(2)}° E`}
                aria-pressed={point.entity_id === selectedId}
                onClick={() => onSelect(point.entity_id)}
                onKeyDown={(event) => {
                  if (event.key === "Enter" || event.key === " ") {
                    event.preventDefault();
                    onSelect(point.entity_id);
                  }
                }}
              >
                <title>{point.name} · {point.entity_type}</title>
                <circle cx={x} cy={y} r={point.entity_type === "city" ? 7 : 5} />
              </g>
            );
          })}
          <text className="geo-offline-axis-label" x={width / 2} y={height - 8} textAnchor="middle">
            经度 {minLongitude.toFixed(1)}° 至 {maxLongitude.toFixed(1)}°
          </text>
        </svg>
      ) : (
        <p className="geo-offline-map-empty">本地资料中还没有可绘制的坐标；仍可使用实体搜索、区域关系和来源记录。</p>
      )}
      <ul className="geo-offline-entity-list" aria-label="本地地理实体">
        {points.map((point) => (
          <li key={point.entity_id}>
            <button
              type="button"
              className={point.entity_id === selectedId ? "selected" : ""}
              onClick={() => onSelect(point.entity_id)}
            >
              <span>{point.name}</span>
              <small>
                {point.coordinate.latitude.toFixed(2)}° N · {point.coordinate.longitude.toFixed(2)}° E
              </small>
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}

export const GeoMap = memo(function GeoMap({ points, lines, layer, active, onSelect, onLayerChange, selectedId = null, overlay = null, focus = null }: GeoMapProps) {
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
      {mapLibreError ? (
        <OfflineCoordinates points={points} lines={visibleLines} onSelect={onSelect} selectedId={selectedId} />
      ) : (
        <ViewerErrorBoundary
          fallback={
            <OfflineCoordinates points={points} lines={visibleLines} onSelect={onSelect} selectedId={selectedId} />
          }
        >
          <Suspense fallback={<div className="geo-maplibre-loading">正在载入 MapLibre 地图模块…</div>}>
            <MapLibreMap
              points={points}
              lines={lines}
              layer={layer}
              onSelect={onSelect}
              onError={() => setMapLibreError(true)}
              center={focus?.center ?? viewpoint.center}
              zoom={focus?.zoom ?? 3.7}
              selectedId={selectedId}
            />
          </Suspense>
        </ViewerErrorBoundary>
      )}
      <div className="geo-map-legend">
        <span><i className="place" />地点</span>
        <span><i className="terrain" />地形地貌</span>
        <span><i className="river" />河流水系</span>
      </div>
      {overlay}
    </div>
    <footer className="geo-map-foot"><span><MapPin size={14} /> 点击实体查看地点信息卡</span><span>{mapLibreError ? "地图未连接" : "MapLibre · Mapterhorn DEM"} · {points.length} 个本地实体 · {visibleLines.length} 条关系</span></footer>
  </section>;
});
