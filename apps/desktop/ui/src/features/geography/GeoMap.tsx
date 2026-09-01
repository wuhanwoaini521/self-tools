import { MapPin, Minus, Plus } from "@phosphor-icons/react";
import { useEffect, useRef, useState, type MouseEvent, type PointerEvent } from "react";
import type { GeoEntityType, GeoMapLine, GeoMapPoint, GeoRelationKind } from "../../types";
import { AmapMap } from "./AmapMap";

export type GeoMapLayer = "base" | "administrative" | "terrain" | "river";

interface GeoMapProps {
  points: GeoMapPoint[];
  lines: GeoMapLine[];
  layer: GeoMapLayer;
  amapApiKey?: string | null;
  amapSecurityJsCode?: string | null;
  selectedId: string | null;
  onSelect: (id: string) => void;
  onLayerChange: (layer: GeoMapLayer) => void;
}

const LAYERS: { id: GeoMapLayer; label: string }[] = [
  { id: "base", label: "基础" },
  { id: "administrative", label: "行政" },
  { id: "terrain", label: "地形图" },
  { id: "river", label: "河流" },
];

const layerMatchesRelation: Record<GeoMapLayer, GeoRelationKind[]> = {
  base: [],
  administrative: ["LOCATED_IN", "PART_OF", "BORDERS"],
  terrain: [],
  river: ["FLOWS_THROUGH", "SOURCE_OF", "FLOWS_INTO", "CROSSES"],
};

const MIN_ZOOM = 1;
const MAX_ZOOM = 4;
const ZOOM_STEP = 0.5;

interface ViewPoint {
  x: number;
  y: number;
}

interface DragState extends ViewPoint {
  pointerId: number;
  startX: number;
  startY: number;
  moved: boolean;
}

function clampZoom(value: number) {
  return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, Number(value.toFixed(2))));
}

function clampPan(value: ViewPoint, zoom: number) {
  const maxX = (zoom - 1) * 500;
  const maxY = (zoom - 1) * 210;
  return {
    x: Math.min(maxX, Math.max(-maxX, value.x)),
    y: Math.min(maxY, Math.max(-maxY, value.y)),
  };
}

function project(latitude: number, longitude: number) {
  return { x: ((longitude + 180) / 360) * 1000, y: ((90 - latitude) / 180) * 420 };
}

function pointTone(type: GeoEntityType) {
  if (type === "river") return "river";
  if (["plateau", "basin", "mountain", "mountain_range", "plain"].includes(type)) return "terrain";
  if (["country", "province", "region", "city"].includes(type)) return "place";
  return "neutral";
}

function labelForRelation(kind: GeoRelationKind) {
  const labels: Partial<Record<GeoRelationKind, string>> = {
    LOCATED_IN: "位于", PART_OF: "属于", FLOWS_THROUGH: "流经", SOURCE_OF: "发源于", FLOWS_INTO: "汇入",
    CONNECTED_TO: "连接", SURROUNDS: "环绕", AFFECTS: "影响", NEAR: "邻近",
  };
  return labels[kind] ?? "关系";
}

export function GeoMap({ points, lines, layer, amapApiKey, amapSecurityJsCode, selectedId, onSelect, onLayerChange }: GeoMapProps) {
  const [zoom, setZoom] = useState(MIN_ZOOM);
  const [pan, setPan] = useState<ViewPoint>({ x: 0, y: 0 });
  const viewportRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<DragState | null>(null);
  const suppressClickRef = useRef(false);
  const visibleLines = layer === "base" ? [] : lines.filter((line) => layerMatchesRelation[layer].includes(line.kind));
  const pointById = new Map(points.map((point) => [point.entity_id, point]));
  const cleanAmapApiKey = amapApiKey?.trim() ?? "";
  const cleanAmapSecurityJsCode = amapSecurityJsCode?.trim() ?? "";
  const hasAmapJsConfig = Boolean(cleanAmapApiKey && cleanAmapSecurityJsCode);
  const zoomTo = (nextZoom: number, focus: ViewPoint) => {
    setZoom((currentZoom) => {
      const next = clampZoom(nextZoom);
      setPan((currentPan) => clampPan({
        x: currentPan.x + (currentZoom - next) * (focus.x - 500),
        y: currentPan.y + (currentZoom - next) * (focus.y - 210),
      }, next));
      return next;
    });
  };
  useEffect(() => {
    if (hasAmapJsConfig) return;
    const viewport = viewportRef.current;
    if (!viewport) return;

    const onWheel = (event: globalThis.WheelEvent) => {
      const bounds = viewport.getBoundingClientRect();
      const focus = {
        x: ((event.clientX - bounds.left) / bounds.width) * 1000,
        y: ((event.clientY - bounds.top) / bounds.height) * 420,
      };

      // 地图缩放时不要让滚轮事件继续触发外层页面滚动。
      event.preventDefault();
      setZoom((currentZoom) => {
        const next = clampZoom(currentZoom + (event.deltaY < 0 ? ZOOM_STEP : -ZOOM_STEP));
        setPan((currentPan) => clampPan({
          x: currentPan.x + (currentZoom - next) * (focus.x - 500),
          y: currentPan.y + (currentZoom - next) * (focus.y - 210),
        }, next));
        return next;
      });
    };

    viewport.addEventListener("wheel", onWheel, { passive: false });
    return () => viewport.removeEventListener("wheel", onWheel);
  }, [hasAmapJsConfig]);

  const startMapDrag = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0 || (event.target instanceof Element && event.target.closest("button"))) return;
    dragRef.current = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      x: pan.x,
      y: pan.y,
      moved: false,
    };
    event.currentTarget.setPointerCapture(event.pointerId);
  };

  const moveMap = (event: PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    const viewport = viewportRef.current;
    if (!drag || drag.pointerId !== event.pointerId || !viewport) return;

    const deltaX = event.clientX - drag.startX;
    const deltaY = event.clientY - drag.startY;
    if (Math.abs(deltaX) > 3 || Math.abs(deltaY) > 3) drag.moved = true;
    if (!drag.moved) return;

    const bounds = viewport.getBoundingClientRect();
    setPan(clampPan({
      x: drag.x + (deltaX / bounds.width) * 1000,
      y: drag.y + (deltaY / bounds.height) * 420,
    }, zoom));
  };

  const finishMapDrag = (event: PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    suppressClickRef.current = drag.moved;
    dragRef.current = null;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
  };

  const suppressDraggedClick = (event: MouseEvent<HTMLDivElement>) => {
    if (!suppressClickRef.current) return;
    event.preventDefault();
    event.stopPropagation();
    suppressClickRef.current = false;
  };

  return <section className="geo-map-card" aria-label="地理探索画布">
    <header className="geo-map-head">
      <div><span className="geo-eyebrow">EXPLORATION CANVAS</span><h2>把世界换一片观察镜片</h2></div>
      <div className="geo-map-controls" aria-label="地图图层">
        {LAYERS.map((item) => <button type="button" key={item.id} className={layer === item.id ? "selected" : ""} onClick={() => onLayerChange(item.id)}>{item.label}</button>)}
      </div>
    </header>
    <div
      className="geo-map-viewport"
      ref={viewportRef}
      onPointerDown={hasAmapJsConfig ? undefined : startMapDrag}
      onPointerMove={hasAmapJsConfig ? undefined : moveMap}
      onPointerUp={hasAmapJsConfig ? undefined : finishMapDrag}
      onPointerCancel={hasAmapJsConfig ? undefined : finishMapDrag}
      onClickCapture={hasAmapJsConfig ? undefined : suppressDraggedClick}
    >
      {hasAmapJsConfig ? <AmapMap apiKey={cleanAmapApiKey} securityJsCode={cleanAmapSecurityJsCode} points={points} lines={lines} layer={layer} onSelect={onSelect} /> : <svg viewBox="0 0 1000 420" role="img" aria-label="离线地理实体关系图">
        <defs>
          <linearGradient id="geo-ocean" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stopColor="var(--geo-ocean-a)" /><stop offset="1" stopColor="var(--geo-ocean-b)" /></linearGradient>
          <pattern id="geo-grid" width="50" height="42" patternUnits="userSpaceOnUse"><path d="M 50 0 L 0 0 0 42" fill="none" stroke="var(--geo-grid)" strokeWidth="0.7" /></pattern>
        </defs>
        <g transform={`translate(${500 + pan.x} ${210 + pan.y}) scale(${zoom}) translate(-500 -210)`}>
          <rect className="geo-map-ocean" width="1000" height="420" rx="14" fill="url(#geo-ocean)" />
          <rect className="geo-map-grid" width="1000" height="420" rx="14" fill="url(#geo-grid)" opacity="0.7" />
          <path className="geo-continent-shape" d="M124 104 C166 68 234 76 272 104 C302 126 292 159 330 181 C300 205 252 192 226 212 C190 232 162 201 142 170 C126 146 99 133 124 104Z" />
          <path className="geo-continent-shape" d="M414 104 C470 74 548 79 592 112 C628 141 656 141 694 164 C726 184 710 221 674 235 C638 248 632 287 604 307 C571 329 537 297 525 271 C510 244 477 235 464 206 C450 175 394 151 414 104Z" />
          <path className="geo-continent-shape" d="M756 271 C797 252 859 264 886 300 C900 324 873 351 831 354 C790 356 753 331 756 271Z" />
          {layer === "river" ? <g className="geo-river-overlay" aria-label="河流水系示意">
            <path className="geo-river-main" d="M 738 143 C 760 150, 773 165, 789 162 C 808 158, 817 145, 837 146" />
            <path className="geo-river-tributary" d="M 788 162 C 798 174, 806 181, 816 188" />
            <text className="geo-layer-label river" x="797" y="151">长江水系</text>
          </g> : null}
          <text x="22" y="398" className="geo-map-note">SCHEMATIC OFFLINE VIEW · WGS84 POINTS · NOT A LEGAL BOUNDARY</text>
          {visibleLines.map((line) => {
          const from = project(line.from.latitude, line.from.longitude);
          const to = project(line.to.latitude, line.to.longitude);
          return <g key={`${line.from_id}-${line.to_id}-${line.kind}`} className={`geo-relation-line ${line.kind === "FLOWS_THROUGH" || line.kind === "SOURCE_OF" || line.kind === "FLOWS_INTO" ? "river" : "terrain"}`}>
            <line x1={from.x} y1={from.y} x2={to.x} y2={to.y} />
            <title>{`${pointById.get(line.from_id)?.name ?? line.from_id} ${labelForRelation(line.kind)} ${pointById.get(line.to_id)?.name ?? line.to_id}`}</title>
          </g>;
          })}
          {points.map((point) => {
          const position = project(point.coordinate.latitude, point.coordinate.longitude);
          const selected = selectedId === point.entity_id;
          const dimmed = layer === "river" && point.entity_type !== "river" && !selected;
          const tone = pointTone(point.entity_type);
          return <g key={point.entity_id} className={`geo-map-point ${tone}${selected ? " selected" : ""}${dimmed ? " dimmed" : ""}`} role="button" tabIndex={0} aria-label={`打开${point.name}`} onClick={() => onSelect(point.entity_id)} onKeyDown={(event) => { if (event.key === "Enter" || event.key === " ") onSelect(point.entity_id); }}>
            {selected ? <circle cx={position.x} cy={position.y} r="13" className="geo-point-ring" /> : null}
            {tone === "terrain" ? <path className="geo-point-mark" d={`M ${position.x} ${position.y - 8} L ${position.x + 8} ${position.y + 6} L ${position.x - 8} ${position.y + 6} Z`} /> : <circle cx={position.x} cy={position.y} r={point.entity_type === "city" ? 5 : 7} />}
            <text x={position.x + 10} y={position.y - 9}>{point.name}</text>
          </g>;
          })}
        </g>
      </svg>}
      {!hasAmapJsConfig && (cleanAmapApiKey || cleanAmapSecurityJsCode) ? <div className="geo-map-config-warning" role="status">请同时填写 Geography 的 JS API Key 和 securityJsCode，当前显示离线地图。</div> : null}
      {!hasAmapJsConfig ? <div className="geo-map-zoom" aria-label="地图缩放控制"><button type="button" title="放大地图" aria-label="放大地图" disabled={zoom >= MAX_ZOOM} onClick={() => zoomTo(zoom + ZOOM_STEP, { x: 500, y: 210 })}><Plus size={15} /></button><button type="button" className="geo-map-zoom-value" title="复位地图缩放" aria-label="复位地图缩放" onClick={() => { setZoom(MIN_ZOOM); setPan({ x: 0, y: 0 }); }}>{zoom.toFixed(1)}×</button><button type="button" title="缩小地图" aria-label="缩小地图" disabled={zoom <= MIN_ZOOM} onClick={() => zoomTo(zoom - ZOOM_STEP, { x: 500, y: 210 })}><Minus size={15} /></button></div> : null}
      <div className="geo-map-legend"><span><i className="place" />地点</span><span><i className="terrain" />地形地貌</span><span><i className="river" />河流水系</span></div>
    </div>
    <footer className="geo-map-foot"><span><MapPin size={14} /> 点击实体继续探索</span><span>{hasAmapJsConfig ? "高德动态地图" : "离线示意底图"} · {points.length} 个本地实体 · {visibleLines.length} 条关系</span></footer>
  </section>;
}
