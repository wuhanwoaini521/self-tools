import {
  ArrowsClockwise,
  MapTrifold,
  Mountains,
  Repeat,
} from "@phosphor-icons/react";
import type { RasterDEMSourceSpecification } from "@maplibre/maplibre-gl-style-spec";
import * as maplibregl from "maplibre-gl";
import { useEffect, useMemo, useRef, useState } from "react";
import "maplibre-gl/dist/maplibre-gl.css";
import type { RealTerrainRegion } from "./realTerrainData";

const OSM_TILES = "https://tile.openstreetmap.org/{z}/{x}/{y}.png";
/**
 * 真实地形不能只依赖单一的在线瓦片服务。任一服务在特定网络、CDN 节点或地区
 * 不可达时，所有地貌条目都会同时变成空白。这里保留 Mapterhorn 作为首选，并用
 * AWS Open Data 的全球 Terrarium DEM 作无密钥后备；两者都是 MapLibre 可直接读取
 * 的 Terrarium 编码。
 */
interface TerrainProvider {
  id: string;
  label: string;
  attribution: string;
  source: RasterDEMSourceSpecification;
}

const TERRAIN_PROVIDERS: TerrainProvider[] = [
  {
    id: "mapterhorn",
    label: "Mapterhorn DEM",
    attribution:
      'Terrain: <a href="https://mapterhorn.com/attribution">Mapterhorn</a>',
    source: {
      type: "raster-dem",
      url: "https://tiles.mapterhorn.com/tilejson.json",
      tileSize: 512,
      encoding: "terrarium",
    },
  },
  {
    id: "aws-terrain",
    label: "AWS Open Data DEM",
    attribution:
      'Terrain: <a href="https://registry.opendata.aws/terrain-tiles/">AWS Open Data</a>',
    source: {
      type: "raster-dem",
      tiles: [
        "https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{z}/{x}/{y}.png",
      ],
      tileSize: 256,
      maxzoom: 15,
      encoding: "terrarium",
    },
  },
];
/** 若地图在这段时间内仍未完成加载，判定为加载失败（而非永久的转圈）。
 *  首次加载需拉取 DEM TileJSON + 瓦片，慢网络下给足余量。 */
const LOAD_TIMEOUT_MS = 25000;

interface RealTerrainViewerProps {
  region: RealTerrainRegion;
}

type ViewStatus = "loading" | "ready" | "error";

interface DiagState {
  webglProbe: boolean | null;
  loadFired: boolean;
  tileFail: number;
  fatal: string | null;
  timeoutFired: boolean;
  webglLost: boolean;
  canvasSize: string | null;
  tilesLoaded: boolean | null;
  painted: boolean | null;
}

const INITIAL_DIAG: DiagState = {
  webglProbe: null,
  loadFired: false,
  tileFail: 0,
  fatal: null,
  timeoutFired: false,
  webglLost: false,
  canvasSize: null,
  tilesLoaded: null,
  painted: null,
};

/**
 * 「真实地形」观测器：用 MapLibre + Mapterhorn DEM 展示某条地貌知识对应的真实地理位置。
 *
 * 地图实例在挂载/重试时创建一次；切换知识条目时通过 effect 平滑迁移到新的观测区域并更新标注。
 * 错误处理刻意区分「致命错误」与「单张瓦片失败」：MapLibre 会对单个瓦片 404 / 瞬时网络
 * 抖动 / 个别瓦片解码失败发出 error 事件，这些都不该让整张地图判死——只有无 sourceId 的
 * 样式/初始化级错误，或超时仍未加载，才进入带「重试」的错误态。
 */
export function RealTerrainViewer({ region }: RealTerrainViewerProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const mapRef = useRef<maplibregl.Map | null>(null);
  const [status, setStatus] = useState<ViewStatus>("loading");
  const [retryToken, setRetryToken] = useState(0);
  const [providerIndex, setProviderIndex] = useState(0);
  const [terrain3d, setTerrain3d] = useState(true);
  const [showGuide, setShowGuide] = useState(true);
  const [webglLost, setWebglLost] = useState(false);
  const [tileNotice, setTileNotice] = useState<string | null>(null);
  const [diag, setDiag] = useState<DiagState>(INITIAL_DIAG);
  const terrainProvider = TERRAIN_PROVIDERS[providerIndex];

  // Create (or recreate on retry) the map.
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    let cancelled = false;
    setStatus("loading");
    setWebglLost(false);
    setTileNotice(null);
    setDiag(INITIAL_DIAG);

    // WebGL 是否可用是 MapLibre 渲染的必要前提；提前探测，避免创建后静默黑屏。
    const probe = document.createElement("canvas");
    const probeGl =
      probe.getContext("webgl") || probe.getContext("experimental-webgl");
    if (!probeGl) {
      setDiag((value) => ({ ...value, webglProbe: false }));
      setWebglLost(true);
      setStatus("error");
      return undefined;
    }
    setDiag((value) => ({ ...value, webglProbe: true }));

    try {
      const map = new maplibregl.Map({
        container,
        center: region.center,
        zoom: region.zoom,
        pitch: region.pitch,
        bearing: region.bearing,
        maxPitch: 85,
        renderWorldCopies: true,
        attributionControl: false,
        style: {
          version: 8,
          sources: {
            "rt-basemap": {
              type: "raster",
              tiles: [OSM_TILES],
              tileSize: 256,
              maxzoom: 19,
              attribution: "© OpenStreetMap contributors",
            },
            // terrain 和 hillshade 使用独立 source，避免它们争抢同一组 DEM 瓦片缓存。
            "rt-terrain": { ...terrainProvider.source, attribution: terrainProvider.attribution },
            "rt-hillshade": { ...terrainProvider.source, attribution: terrainProvider.attribution },
          },
          layers: [
            {
              id: "rt-basemap",
              type: "raster",
              source: "rt-basemap",
              paint: { "raster-opacity": 0.4 },
            },
            {
              id: "rt-hillshade",
              type: "hillshade",
              source: "rt-hillshade",
              layout: { visibility: "visible" },
              paint: {
                "hillshade-exaggeration": 1.0,
                "hillshade-shadow-color": "#3a4234",
                "hillshade-highlight-color": "#fff3cc",
                "hillshade-accent-color": "#6f5e42",
                "hillshade-illumination-direction": 315,
                "hillshade-illumination-anchor": "map",
              },
            },
          ],
          sky: {},
        },
      });
      mapRef.current = map;
      // MapLibre 在 map.remove() 时会主动调用 WEBGL_lose_context 来释放 GPU。
      // 因此不能直接监听 canvas：组件切换、StrictMode 的开发期重挂载都会被误判为
      // "WebGL 丢失"。用 MapLibre 事件并跳过已取消的实例，且在浏览器自动恢复时
      // 清除错误态，让它自行重建样式和画布。
      const handleContextLost = () => {
        if (cancelled) return;
        setWebglLost(true);
        setDiag((value) => ({ ...value, webglLost: true }));
      };
      const handleContextRestored = () => {
        if (cancelled) return;
        setWebglLost(false);
        setDiag((value) => ({ ...value, webglLost: false }));
      };
      map.on("webglcontextlost", handleContextLost);
      map.on("webglcontextrestored", handleContextRestored);
      // 不直接读取 WebGL 像素判断画面：MapLibre 可能使用 WebGL2，而从同一
      // canvas 再请求 WebGL1 会返回 null，造成“未绘制”的假阳性。render 事件
      // 是 MapLibre 对“至少已完成一帧”的权威信号。
      let hasRendered = false;
      const handleMapRender = () => {
        hasRendered = true;
      };
      map.on("render", handleMapRender);
      map.addControl(
        new maplibregl.NavigationControl({
          showCompass: true,
          visualizePitch: true,
        }),
        "top-right",
      );
      map.addControl(
        new maplibregl.AttributionControl({ compact: true }),
        "bottom-right",
      );

      let loaded = false;
      let tileFailureCount = 0;
      let terrainProviderFailed = false;
      const timeoutId = window.setTimeout(() => {
        if (!cancelled && !loaded) {
          setDiag((value) => ({ ...value, timeoutFired: true }));
          setStatus("error");
        }
      }, LOAD_TIMEOUT_MS);

      map.on("error", (event) => {
        if (cancelled) return;
        // SAFETY: MapLibre error events carry optional sourceId/tile on
        // per-source/tile failures; the assertion only reads those optional
        // fields (plus the always-present error), never writes, so this
        // shape is safe regardless of actual event subclass.
        const errEvent = event as unknown as {
          error?: unknown;
          sourceId?: string;
          tile?: unknown;
        };
        if (!errEvent.error) return;

        // 单张/某个源的瓦片失败：常见且可恢复（404 / 网络抖动 / 解码失败）。
        // DEM 两个 source 都连续失败时切到后备服务；不要因 OSM 底图的一次失败
        // 误切换高程服务。
        if (errEvent.sourceId || errEvent.tile) {
          const isTerrainFailure =
            errEvent.sourceId === "rt-terrain" ||
            errEvent.sourceId === "rt-hillshade";
          if (isTerrainFailure) {
            tileFailureCount += 1;
            setDiag((value) => ({ ...value, tileFail: value.tileFail + 1 }));
            if (tileFailureCount >= 3 && !terrainProviderFailed) {
              terrainProviderFailed = true;
              if (providerIndex < TERRAIN_PROVIDERS.length - 1) {
                window.clearTimeout(timeoutId);
                setProviderIndex((value) => value + 1);
                return;
              }
              setTileNotice(
                "高程瓦片未能加载（可能网络受限），当前仅显示可用底图。",
              );
            }
          }
          return;
        }

        // 致命错误：无 sourceId/tile 的样式/初始化/WebGL 上下文问题。
        if (loaded) return;
        const fatalMessage =
          (errEvent.error as { message?: string })?.message ??
          String(errEvent.error);
        setDiag((value) => ({ ...value, fatal: fatalMessage }));
        console.error("[real-terrain] fatal map error:", fatalMessage);
        window.clearTimeout(timeoutId);
        setStatus("error");
      });

      map.once("load", () => {
        if (cancelled) return;
        loaded = true;
        window.clearTimeout(timeoutId);
        // 先把状态置为 ready，再做可选的 3D 地形/标注等增强设置；
        // 这些设置即便在某些渲染器上抛错，也不该卡在 loading 或回退成失败。
        try {
          // 显式 resize：个别 WebView 的 ResizeObserver 不触发，画布会保持 0 尺寸。
          map.resize();
          const canvas = map.getCanvas();
          setDiag((value) => ({
            ...value,
            loadFired: true,
            canvasSize: `${canvas.width}\u00d7${canvas.height}`,
          }));
        } catch (resizeError) {
          setDiag((value) => ({ ...value, loadFired: true }));
          console.error("[real-terrain] resize failed:", resizeError);
        }
        setStatus("ready");
        try {
          map.setTerrain({ source: "rt-terrain", exaggeration: 1.8 });
          updateRegionMarker(map, region);
        } catch (setupError) {
          console.error(
            "[real-terrain] post-load setup failed (map still usable):",
            setupError,
          );
        }

        // 将 MapLibre 的绘制状态记录到诊断信息中；无法确认时保留“未知”，而不
        // 用跨 WebGL 版本的像素采样误报“画面未绘制”。
        window.setTimeout(() => {
          if (cancelled || !loaded) return;
          try {
            const tilesLoaded = map.areTilesLoaded();
            setDiag((value) => ({
              ...value,
              tilesLoaded,
              painted: hasRendered ? true : null,
            }));
          } catch (diagnosticError) {
            console.error("[real-terrain] render diagnostic failed:", diagnosticError);
          }
        }, 3000);
      });

      // 窗口尺寸变化时同步地图画布（同样覆盖 ResizeObserver 失效场景）。
      const handleWindowResize = () => {
        try {
          map.resize();
        } catch {
          /* noop */
        }
      };
      window.addEventListener("resize", handleWindowResize);

      return () => {
        cancelled = true;
        window.clearTimeout(timeoutId);
        window.removeEventListener("resize", handleWindowResize);
        map.off("webglcontextlost", handleContextLost);
        map.off("webglcontextrestored", handleContextRestored);
        map.off("render", handleMapRender);
        map.remove();
        mapRef.current = null;
      };
    } catch (error) {
      // 地图初始化阶段同步抛错（如 WebGL 上下文创建失败）时，绝不让它逃出 effect
      // 去卸载整个 React 树——降级到可重试的错误态即可。
      const message = error instanceof Error ? error.message : String(error);
      setDiag((value) => ({ ...value, fatal: message }));
      console.error("[real-terrain] map initialization failed:", error);
      setStatus("error");
      return undefined;
    }
    // 地图按需重建（挂载 + 用户点「重试」）。eslint 依赖警告可忽略。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [retryToken, providerIndex]);

  // Sync to a (possibly new) region while keeping the same map instance.
  useEffect(() => {
    const map = mapRef.current;
    if (!map || status !== "ready") return;
    updateRegionMarker(map, region);
    if (region.pitch > 0) {
      map.setTerrain({ source: "rt-terrain", exaggeration: 1.8 });
      map.easeTo({
        center: region.center,
        zoom: region.zoom,
        pitch: region.pitch,
        bearing: region.bearing,
        duration: 720,
      });
    } else {
      map.easeTo({ center: region.center, zoom: region.zoom, duration: 720 });
    }
  }, [region, status]);

  useEffect(() => {
    const map = mapRef.current;
    if (!map || status !== "ready") return;
    map.setTerrain(
      terrain3d && region.pitch > 0
        ? { source: "rt-terrain", exaggeration: 1.8 }
        : null,
    );
    if (terrain3d && map.getPitch() < 1)
      map.easeTo({ pitch: Math.max(region.pitch, 50), duration: 420 });
    if (!terrain3d && map.getPitch() > 1)
      map.easeTo({ pitch: 0, bearing: 0, duration: 320 });
  }, [terrain3d, region, status]);

  const handleRetry = () => {
    // 每次手动重试都从首选服务开始；自动切换仍会在 DEM 连续失败时发生。
    setProviderIndex(0);
    setRetryToken((value) => value + 1);
  };

  const regionMemo = useMemo(() => region, [region]);

  // 把诊断信息镜像到全局，方便在控制台直接输入 window.__realTerrainDiag 查看。
  useEffect(() => {
    // SAFETY: `__realTerrainDiag` 是本组件私有的诊断槽，库与其它模块不会读写它。
    (window as unknown as { __realTerrainDiag?: DiagState }).__realTerrainDiag =
      diag;
  }, [diag]);

  return (
    <section
      className="realterrain-viewer"
      aria-label={`${regionMemo.place} 真实地形观测`}
    >
      <header className="realterrain-viewer-head">
        <div>
          <span>REAL TERRAIN · MAPLIBRE DEM</span>
          <strong>{regionMemo.place} · 真实地形观测</strong>
        </div>
        <div className="realterrain-viewer-actions">
          <button
            type="button"
            className={terrain3d ? "selected" : ""}
            aria-label={terrain3d ? "切换为平面地图" : "切换为 3D 地形"}
            title={terrain3d ? "3D 地形" : "平面地图"}
            onClick={() => setTerrain3d((value) => !value)}
          >
            <Mountains size={14} />
            3D 地形
          </button>
          <button
            type="button"
            className={showGuide ? "selected" : ""}
            aria-label={showGuide ? "隐藏观察引导" : "显示观察引导"}
            title={showGuide ? "观察引导" : "观察引导"}
            onClick={() => setShowGuide((value) => !value)}
          >
            <MapTrifold size={14} />
            观察引导
          </button>
          <button
            type="button"
            aria-label="重置地图视角"
            title="重置视角"
            onClick={() => {
              const map = mapRef.current;
              if (!map) return;
              map.easeTo({
                center: region.center,
                zoom: region.zoom,
                pitch: region.pitch,
                bearing: region.bearing,
                duration: 520,
              });
            }}
          >
            <ArrowsClockwise size={15} />
          </button>
        </div>
      </header>
      <div className="realterrain-map-wrap">
        <div ref={containerRef} className="realterrain-map" />
        {status === "loading" ? (
          <div className="realterrain-loading">
            <Mountains size={16} />
            <span>正在加载 MapLibre 真实地形…</span>
          </div>
        ) : null}
        {status === "error" ? (
          <div className="realterrain-error" role="alert">
            {webglLost ? (
              <>
                <span>当前环境无法启用 WebGL，真实地形地图无法渲染。</span>
                <p>
                  请开启浏览器的硬件加速 / 更新显卡驱动 / 使用支持 WebGL
                  的浏览器（Chrome、Edge、Firefox）后重试。概念地形仍可正常使用。
                </p>
              </>
            ) : (
              <>
                <span>在线地形暂时无法加载。</span>
                <p>
                  真实地形依赖在线高程瓦片，请检查网络连接后重试；概念地形仍可正常使用。
                </p>
              </>
            )}
            <button type="button" onClick={handleRetry}>
              <Repeat size={14} />
              重试
            </button>
          </div>
        ) : null}
        {showGuide && status === "ready" && !webglLost ? (
          <aside className="realterrain-guide">
            <span className="realterrain-guide-label">
              在这片真实地貌里看什么
            </span>
            <strong>{regionMemo.focus}</strong>
            <p>{regionMemo.observation}</p>
            <small>{regionMemo.cue}</small>
          </aside>
        ) : null}
        {status === "ready" && webglLost ? (
          <div className="realterrain-error" role="alert">
            <span>地图已加载，但 WebGL 渲染上下文丢失，画面无法绘制。</span>
            <p>
              请开启浏览器硬件加速 / 更新显卡驱动 / 关闭占用 GPU
              的大型程序后重试；概念地形仍可正常使用。
            </p>
            <button type="button" onClick={handleRetry}>
              <Repeat size={14} />
              重试
            </button>
          </div>
        ) : null}
        {status === "ready" ? (
          <div className="realterrain-hint">
            滚轮缩放 · 拖动旋转 · {terrainProvider.label} 真实高程
          </div>
        ) : null}
        {tileNotice && status === "ready" ? (
          <div className="realterrain-tile-notice" role="status">
            {tileNotice}
          </div>
        ) : null}
        {providerIndex > 0 && status === "ready" ? (
          <div className="realterrain-provider-notice" role="status">
            已自动切换至 {terrainProvider.label}。
          </div>
        ) : null}
        <div className="realterrain-diag" aria-hidden="true">
          <span
            className={
              diag.webglProbe ? "ok" : diag.webglProbe === null ? "pend" : "bad"
            }
          >
            WebGL {diag.webglProbe === null ? "…" : diag.webglProbe ? "✓" : "✗"}
          </span>
          <span className={diag.loadFired ? "ok" : "pend"}>
            Load {diag.loadFired ? "✓" : "✗"}
          </span>
          <span
            className={diag.webglLost ? "bad" : "ok"}
            title={diag.webglLost ? "渲染上下文已丢失" : undefined}
          >
            {diag.webglLost ? "GL 丢失" : "绘制正常"}
          </span>
          <span>瓦片失败 {diag.tileFail}</span>
          {diag.canvasSize ? <span>画布 {diag.canvasSize}</span> : null}
          {diag.tilesLoaded === null ? null : (
            <span className={diag.tilesLoaded ? "ok" : "bad"}>
              瓦片就绪 {diag.tilesLoaded ? "✓" : "✗"}
            </span>
          )}
          {diag.painted === null ? null : (
            <span
              className={
                diag.painted ? "ok" : diag.painted === false ? "bad" : "pend"
              }
            >
              绘制 {diag.painted === null ? "?" : diag.painted ? "✓" : "✗"}
            </span>
          )}
          {diag.timeoutFired ? <span className="bad">超时</span> : null}
          {diag.fatal ? (
            <span className="bad" title={diag.fatal}>
              {diag.fatal}
            </span>
          ) : null}
        </div>
      </div>
    </section>
  );
}

function updateRegionMarker(map: maplibregl.Map, region: RealTerrainRegion) {
  const [lng, lat] = region.center;
  // Build the marker element with DOM APIs (no innerHTML) so the place label
  // is always treated as textContent — no XSS surface.
  const el = document.createElement("div");
  el.className = "realterrain-marker";
  const pin = document.createElement("span");
  pin.className = "realterrain-marker-pin";
  const dot = document.createElement("span");
  dot.className = "realterrain-marker-dot";
  const label = document.createElement("span");
  label.className = "realterrain-marker-label";
  label.textContent = region.place;
  el.append(pin, dot, label);
  const marker = new maplibregl.Marker({ element: el, anchor: "bottom" })
    .setLngLat([lng, lat])
    .addTo(map);
  // Keep only one marker on the map.
  // SAFETY: `__rtMarker` is our private slot on Map, never touched by library
  // or other modules, so reading/writing it as this shape is guaranteed safe.
  const slot = map as unknown as { __rtMarker?: maplibregl.Marker };
  try {
    slot.__rtMarker?.remove();
  } catch {
    /* noop */
  }
  // SAFETY: same invariant as above — we own the slot and always store a
  // fully-added Marker instance here.
  (map as unknown as { __rtMarker: maplibregl.Marker }).__rtMarker = marker;
}

export default RealTerrainViewer;
