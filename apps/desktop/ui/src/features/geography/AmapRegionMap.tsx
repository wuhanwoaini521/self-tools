import { lazy, Suspense, useEffect, useMemo, useState } from "react";
import type { GeoEntity, GeoEntityType, GeoMapLine, GeoMapPoint, GeoRelationView } from "../../types";
import { OfflineCoordinates } from "./GeoMap";
import { ViewerErrorBoundary } from "./ViewerErrorBoundary";
const MapLibreMap = lazy(() => import("./MapLibreMap").then((module) => ({ default: module.MapLibreMap })));

interface AmapRegionMapProps {
  item: GeoEntity;
  relations?: GeoRelationView[];
  onSelect?: (id: string) => void;
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

function relationLines(item: GeoEntity, relations: GeoRelationView[]): GeoMapLine[] {
  const entities = new Map([item, ...relations.map((relation) => relation.entity)].map((entity) => [entity.id, entity]));
  return relations.flatMap(({ relation }) => {
    const from = entities.get(relation.from_id)?.coordinates;
    const to = entities.get(relation.to_id)?.coordinates;
    return from && to
      ? [{ from_id: relation.from_id, to_id: relation.to_id, kind: relation.kind, from, to }]
      : [];
  });
}

export function AmapRegionMap({ item, relations = [], onSelect }: AmapRegionMapProps) {
  const [mapView, setMapView] = useState<"terrain" | "administrative">("terrain");
  const [mapError, setMapError] = useState(false);
  const mapPoints = useMemo(() => terrainPoints(item, relations), [item, relations]);
  const mapLines = useMemo(() => relationLines(item, relations), [item, relations]);

  useEffect(() => {
    setMapView("terrain");
    setMapError(false);
  }, [item.id]);

  const canShowMap = Boolean(item.coordinates || ["country", "province", "region", "city"].includes(item.entity_type));
  if (!canShowMap) return null;

  const mapTitle = `${item.name}地图`;
  return (
    <section className="geo-region-map-card" aria-label={mapTitle}>
      <header className="geo-region-map-head">
        <div>
          <span className="geo-eyebrow">{mapView === "administrative" ? "REGIONAL MAP · 行政区域" : "TERRAIN MAP · 三维地形"}</span>
          <h3>{mapTitle}</h3>
        </div>
        <div className="geo-detail-map-tabs" role="tablist" aria-label="详情地图类型">
          <button type="button" role="tab" aria-selected={mapView === "terrain"} className={mapView === "terrain" ? "selected" : ""} onClick={() => setMapView("terrain")}>3D 地形</button>
          <button type="button" role="tab" aria-selected={mapView === "administrative"} className={mapView === "administrative" ? "selected" : ""} onClick={() => setMapView("administrative")}>行政区域</button>
        </div>
      </header>
      {mapView === "terrain" ? (
        <div className="geo-region-map-wrap geo-detail-terrain-wrap">
          {mapError ? (
            <OfflineCoordinates
              points={mapPoints}
              lines={mapLines}
              onSelect={onSelect ?? (() => undefined)}
              compact
            />
          ) : (
            <ViewerErrorBoundary
              fallback={(
                <OfflineCoordinates
                  points={mapPoints}
                  lines={mapLines}
                  onSelect={onSelect ?? (() => undefined)}
                  compact
                />
              )}
            >
              <Suspense fallback={<div className="geo-region-map-empty">正在加载地形地图…</div>}>
                <MapLibreMap
                  points={mapPoints}
                  lines={mapLines}
                  layer="terrain"
                  onSelect={onSelect ?? (() => undefined)}
                  onError={() => setMapError(true)}
                  center={item.coordinates ? [item.coordinates.longitude, item.coordinates.latitude] : [105, 32]}
                  zoom={terrainZoom(item.entity_type)}
                />
              </Suspense>
            </ViewerErrorBoundary>
          )}
        </div>
      ) : (
        <div className="geo-region-map-empty" role="status">
          动态行政边界暂不可用。为避免将地图凭据发送到浏览器，待服务端地图代理接入后再启用；当前可使用交互式地形图与位置标记。
        </div>
      )}
    </section>
  );
}
