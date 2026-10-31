import { useEffect, useRef, useState } from "react";
import * as maplibregl from "maplibre-gl";
import type { GeoMapLine, GeoMapPoint } from "../../types";
import "maplibre-gl/dist/maplibre-gl.css";

interface MapLibreMapProps {
  points: GeoMapPoint[];
  lines: GeoMapLine[];
  layer: GeoMapLayer;
  onSelect: (id: string) => void;
  onError: () => void;
  center?: [number, number];
  zoom?: number;
  compact?: boolean;
  selectedId?: string | null;
}

type GeoMapLayer = "base" | "administrative" | "terrain" | "river";

const TERRAIN_TILEJSON = "https://tiles.mapterhorn.com/tilejson.json";
const OSM_TILES = "https://tile.openstreetmap.org/{z}/{x}/{y}.png";

function relationKinds(layer: GeoMapLayer) {
  if (layer === "administrative") return ["LOCATED_IN", "PART_OF", "BORDERS"];
  if (layer === "river") return ["FLOWS_THROUGH", "SOURCE_OF", "FLOWS_INTO", "CROSSES"];
  return [];
}

function relationFeatures(lines: GeoMapLine[], layer: GeoMapLayer): GeoJSON.FeatureCollection<GeoJSON.LineString> {
  const kinds = relationKinds(layer);
  return {
    type: "FeatureCollection",
    features: lines
      .filter((line) => kinds.includes(line.kind))
      .map((line) => ({
        type: "Feature",
        properties: { id: `${line.from_id}-${line.to_id}-${line.kind}`, kind: line.kind },
        geometry: { type: "LineString", coordinates: [[line.from.longitude, line.from.latitude], [line.to.longitude, line.to.latitude]] },
      })),
  };
}

function webgl2Available(): boolean {
  try {
    const canvas = document.createElement("canvas");
    const context = canvas.getContext("webgl2");
    const available = Boolean(context);
    context?.getExtension("WEBGL_lose_context")?.loseContext();
    return available;
  } catch {
    return false;
  }
}

function pointFeatures(points: GeoMapPoint[]): GeoJSON.FeatureCollection<GeoJSON.Point> {
  return {
    type: "FeatureCollection",
    features: points.map((point) => ({
      type: "Feature",
      properties: { id: point.entity_id, name: point.name, entity_type: point.entity_type },
      geometry: { type: "Point", coordinates: [point.coordinate.longitude, point.coordinate.latitude] },
    })),
  };
}

function selectionFilter(selectedId: string | null): maplibregl.FilterSpecification {
  return ["==", ["get", "id"], selectedId ?? ""] as unknown as maplibregl.FilterSpecification;
}

function updateMapData(map: maplibregl.Map, points: GeoMapPoint[], lines: GeoMapLine[], layer: GeoMapLayer) {  const pointSource = map.getSource("geo-points") as maplibregl.GeoJSONSource | undefined;
  const relationSource = map.getSource("geo-relations") as maplibregl.GeoJSONSource | undefined;
  pointSource?.setData(pointFeatures(points));
  relationSource?.setData(relationFeatures(lines, layer));
  if (map.getLayer("geo-relations")) {
    map.setLayoutProperty("geo-relations", "visibility", layer === "base" ? "none" : "visible");
    map.setPaintProperty("geo-relations", "line-color", layer === "river" ? "#54c0e8" : "#d0ad67");
    map.setPaintProperty("geo-relations", "line-dasharray", layer === "river" ? [1, 0] : [2, 1]);
  }
  if (map.getLayer("geo-hillshade")) {
    map.setLayoutProperty("geo-hillshade", "visibility", layer === "terrain" ? "visible" : "none");
  }
  if (map.getLayer("geo-basemap")) {
    map.setPaintProperty("geo-basemap", "raster-opacity", layer === "terrain" ? 0.5 : 0.9);
  }
  map.setTerrain(layer === "terrain" ? { source: "geo-terrain", exaggeration: 1.8 } : null);
  if (layer === "terrain") {
    map.easeTo({ pitch: 58, bearing: -8, duration: 420 });
  } else if (map.getPitch() > 1) {
    map.easeTo({ pitch: 0, bearing: 0, duration: 260 });
  }
}

export function MapLibreMap({ points, lines, layer, onSelect, onError, center = [91, 33], zoom = 3.7, compact = false, selectedId = null }: MapLibreMapProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const mapRef = useRef<maplibregl.Map | null>(null);
  const onSelectRef = useRef(onSelect);
  const selectedIdRef = useRef(selectedId);
  const [ready, setReady] = useState(false);
  const [mapError, setMapError] = useState(false);
  const mapFailedRef = useRef(false);
  onSelectRef.current = onSelect;
  selectedIdRef.current = selectedId;

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    let cancelled = false;
    if (!webgl2Available()) {
      mapFailedRef.current = true;
      setMapError(true);
      onError();
      return () => {
        cancelled = true;
      };
    }
    let map: maplibregl.Map;
    try {
      map = new maplibregl.Map({
        container,
        center,
        zoom,
        pitch: 52,
        bearing: -8,
        maxPitch: 85,
        renderWorldCopies: false,
        attributionControl: false,
        style: {
          version: 8,
          sources: {
            "geo-basemap": { type: "raster", tiles: [OSM_TILES], tileSize: 256, maxzoom: 19, attribution: "© OpenStreetMap contributors" },
            "geo-terrain": { type: "raster-dem", url: TERRAIN_TILEJSON, tileSize: 512, encoding: "terrarium", attribution: "Terrain: <a href=\"https://mapterhorn.com/attribution\">Mapterhorn</a>" },
            "geo-hillshade-source": { type: "raster-dem", url: TERRAIN_TILEJSON, tileSize: 512, encoding: "terrarium", attribution: "Terrain: <a href=\"https://mapterhorn.com/attribution\">Mapterhorn</a>" },
          },
          layers: [
            { id: "geo-basemap", type: "raster", source: "geo-basemap", paint: { "raster-opacity": 0.62 } },
            { id: "geo-hillshade", type: "hillshade", source: "geo-hillshade-source", layout: { visibility: "visible" }, paint: { "hillshade-exaggeration": 0.95, "hillshade-shadow-color": "#3f4738", "hillshade-highlight-color": "#fff0c4", "hillshade-accent-color": "#766345", "hillshade-illumination-direction": 315, "hillshade-illumination-anchor": "map" } },
          ],
          sky: {},
        },
      });
    } catch {
      mapFailedRef.current = true;
      setMapError(true);
      onError();
      return () => {
        cancelled = true;
      };
    }
    mapRef.current = map;
    map.addControl(new maplibregl.NavigationControl({ showCompass: true, visualizePitch: true }), "top-right");
    map.addControl(new maplibregl.AttributionControl({ compact: true }), "bottom-right");
    let styleLoaded = false;
    map.on("error", (event) => {
      if (!cancelled && !styleLoaded && event.error) {
        mapFailedRef.current = true;
        setMapError(true);
        onError();
      }
    });
    map.once("load", () => {
      if (cancelled || mapFailedRef.current) return;
      styleLoaded = true;
      map.addSource("geo-points", { type: "geojson", data: pointFeatures(points) });
      map.addSource("geo-relations", { type: "geojson", data: relationFeatures(lines, layer) });
      map.addLayer({ id: "geo-relations", type: "line", source: "geo-relations", layout: { visibility: layer === "base" ? "none" : "visible" }, paint: { "line-color": layer === "river" ? "#54c0e8" : "#d0ad67", "line-width": layer === "river" ? 3.5 : 2.5, "line-opacity": 0.85, "line-dasharray": layer === "river" ? [1, 0] : [2, 1] } });
      map.addLayer({ id: "geo-points", type: "circle", source: "geo-points", paint: { "circle-radius": ["match", ["get", "entity_type"], "city", 5, 7], "circle-color": ["match", ["get", "entity_type"], "river", "#54c0e8", "mountain", "#d0ad67", "mountain_range", "#d0ad67", "plateau", "#d0ad67", "basin", "#8fbf83", "#18a999"], "circle-stroke-color": "#ffffff", "circle-stroke-width": 1.5 } });
      map.addLayer({
        id: "geo-point-selected",
        type: "circle",
        source: "geo-points",
        filter: selectionFilter(selectedIdRef.current),
        paint: {
          "circle-radius": ["match", ["get", "entity_type"], "city", 9, 11],
          "circle-color": "#5566e6",
          "circle-opacity": 0.18,
          "circle-stroke-color": "#5566e6",
          "circle-stroke-width": 2.4,
          "circle-stroke-opacity": 0.95,
        },
      });
      map.addLayer({ id: "geo-point-labels", type: "symbol", source: "geo-points", layout: { "text-field": ["get", "name"], "text-size": 11, "text-offset": [0.8, -0.8], "text-anchor": "left" }, paint: { "text-color": "#17383d", "text-halo-color": "#ffffff", "text-halo-width": 1.5 } });
      map.on("click", "geo-points", (event) => {
        const feature = event.features?.[0];
        const id = feature?.properties?.id;
        if (typeof id === "string") onSelectRef.current(id);
      });
      map.on("mouseenter", "geo-points", () => { map.getCanvas().style.cursor = "pointer"; });
      map.on("mouseleave", "geo-points", () => { map.getCanvas().style.cursor = ""; });
      updateMapData(map, points, lines, layer);
      setReady(true);
    });
    return () => {
      cancelled = true;
      try {
        map.remove();
      } catch {
        // MapLibre may have only partially initialized when WebGL is unavailable.
      }
      mapRef.current = null;
    };
  // MapLibre must be created once; data and callbacks are synchronized below.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!ready || !mapRef.current) return;
    updateMapData(mapRef.current, points, lines, layer);
  }, [layer, lines, points, ready]);

  useEffect(() => {
    if (!ready || !mapRef.current) return;
    const map = mapRef.current;
    if (map.getLayer("geo-point-selected")) {
      map.setFilter("geo-point-selected", selectionFilter(selectedId));
    }
  }, [ready, selectedId]);

  useEffect(() => {
    if (!ready || !mapRef.current) return;
    mapRef.current.easeTo({ center, zoom, duration: 650 });
  }, [center[0], center[1], ready, zoom]);

  return (
    <div className={`geo-maplibre-wrap${compact ? " compact" : ""}`}>
      <div
        ref={containerRef}
        className="geo-maplibre-map"
        role="img"
        aria-label="MapLibre 真实三维地形地图"
        aria-hidden={mapError || undefined}
        style={mapError ? { visibility: "hidden" } : undefined}
      />
      {mapError ? (
        <div className="geo-maplibre-hint" role="status">
          地图暂不可用；地点资料仍可查看。
        </div>
      ) : ready ? (
        <div className="geo-maplibre-hint">
          MapLibre GL JS · <a href="https://mapterhorn.com/attribution" target="_blank" rel="noreferrer">Mapterhorn DEM 地形</a> · 滚轮缩放 · 拖动旋转
        </div>
      ) : (
        <div className="geo-maplibre-hint">正在加载 MapLibre 地图与 Mapterhorn 地形…</div>
      )}
    </div>
  );
}
