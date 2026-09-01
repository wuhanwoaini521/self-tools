import { Canvas } from "@react-three/fiber";
import {
  ArrowsClockwise,
  ChartLineUp,
  EyeSlash,
  MapTrifold,
  SelectionBackground,
  Tag,
} from "@phosphor-icons/react";
import { useEffect, useMemo, useState } from "react";
import type {
  LandformHotspot,
  LandformViewerConfig,
  LandformViewMode,
} from "./landform3dData";
import { getGenerator } from "./terrain/generators";
import { makeHeightfield, sampleLine } from "./terrain/heightfield";
import { getPreset } from "./terrain/presets";
import { TerrainScene } from "./terrain/TerrainScene";
import { ElevationProfile } from "./terrain/ElevationProfile";

interface Landform3DViewerProps {
  title: string;
  config: LandformViewerConfig;
}

const MODES: {
  id: LandformViewMode;
  label: string;
  icon: typeof MapTrifold;
}[] = [
  { id: "terrain", label: "地形", icon: MapTrifold },
  { id: "contour", label: "等高线", icon: ChartLineUp },
  { id: "section", label: "剖面", icon: SelectionBackground },
];

const DRAG_HINTS: Record<LandformViewMode, string> = {
  terrain: "拖动旋转 · 滚轮缩放",
  contour: "等高线叠加在地形上 · 拖动观察",
  section: "A─B 剖面 · 下方对应高程曲线",
};

function StaticFallback({ title, image }: { title: string; image?: string }) {
  return (
    <div
      className="landform-static-fallback"
      role="img"
      aria-label={`${title}静态地貌示意图`}
    >
      {image ? (
        <img src={image} alt="" />
      ) : (
        <svg viewBox="0 0 640 260" aria-hidden="true">
          <path d="M0 198 L85 158 L160 185 L236 75 L318 145 L405 108 L492 178 L560 132 L640 184 V260 H0Z" />
          <path d="M0 204 C130 182 182 208 294 180 S476 214 640 173" />
        </svg>
      )}
      <span>当前设备无法启用 3D 观察，以下为静态地貌示意。</span>
    </div>
  );
}

export function Landform3DViewer({ title, config }: Landform3DViewerProps) {
  const preset = useMemo(() => getPreset(config.modelType), [config.modelType]);
  const hotspots: LandformHotspot[] = config.hotspots;

  const [mode, setMode] = useState<LandformViewMode>("terrain");
  const [labelsVisible, setLabelsVisible] = useState(false);
  const [resetToken, setResetToken] = useState(0);
  const [webglAvailable, setWebglAvailable] = useState<boolean | null>(null);
  const [selectedHotspot, setSelectedHotspot] =
    useState<LandformHotspot | null>(null);
  const [compact, setCompact] = useState(false);

  useEffect(() => {
    const canvas = document.createElement("canvas");
    setWebglAvailable(
      Boolean(
        canvas.getContext("webgl") || canvas.getContext("experimental-webgl"),
      ),
    );
  }, []);

  useEffect(() => {
    const mq = window.matchMedia("(max-width: 720px)");
    const update = () => setCompact(mq.matches);
    update();
    mq.addEventListener("change", update);
    return () => mq.removeEventListener("change", update);
  }, []);

  // switching landform: back to observation mode, clear selection, reset camera
  useEffect(() => {
    setMode("terrain");
    setLabelsVisible(false);
    setSelectedHotspot(null);
    setResetToken((value) => value + 1);
  }, [config.modelType]);

  const resolution = compact
    ? preset.resolutionMobile
    : preset.resolutionDesktop;
  const heightfield = useMemo(() => {
    const generator = getGenerator(preset.type);
    return makeHeightfield(generator(preset.seed), resolution, preset.extent);
  }, [preset.type, preset.seed, resolution, preset.extent]);

  const profile = useMemo(() => {
    if (mode !== "section") return null;
    const { a, b } = preset.section;
    const heights = sampleLine(heightfield, a[0], a[1], b[0], b[1], 180);
    let min = Infinity;
    let max = -Infinity;
    for (const h of heights) {
      if (h < min) min = h;
      if (h > max) max = h;
    }
    const length = Math.hypot(b[0] - a[0], b[1] - a[1]);
    return { heights, min, max, length, aLabel: "A", bLabel: "B" };
  }, [mode, heightfield, preset]);

  if (webglAvailable === false) {
    return (
      <section className="landform-viewer landform-viewer-fallback">
        <StaticFallback title={title} image={config.fallbackImage} />
      </section>
    );
  }

  return (
    <section className="landform-viewer" aria-label={`${title} 3D 教学模型`}>
      <header className="landform-viewer-head">
        <div>
          <span>SCIENTIFIC TERRAIN</span>
          <strong>{title} · 连续地形剖面模型</strong>
        </div>
        <div className="landform-viewer-actions">
          <div role="group" aria-label="地貌视图模式">
            {MODES.map(({ id, label, icon: Icon }) => (
              <button
                key={id}
                type="button"
                className={mode === id ? "selected" : ""}
                aria-label={`切换到${label}视图`}
                onClick={() => setMode(id)}
              >
                <Icon size={14} />
                {label}
              </button>
            ))}
          </div>
          <button
            type="button"
            className={labelsVisible ? "selected" : ""}
            aria-label={labelsVisible ? "关闭地表标注" : "显示地表标注"}
            title={labelsVisible ? "关闭标注" : "显示标注"}
            onClick={() => setLabelsVisible((value) => !value)}
          >
            <Tag size={14} />
            标注
          </button>
          <button
            type="button"
            aria-label="重置模型视角"
            title="重置视角"
            onClick={() => setResetToken((value) => value + 1)}
          >
            <ArrowsClockwise size={15} />
          </button>
        </div>
      </header>
      <div className="landform-canvas-wrap">
        {webglAvailable === null ? (
          <div className="landform-loading">正在生成连续地形…</div>
        ) : (
          <Canvas
            shadows
            dpr={[1, 1.5]}
            camera={{ fov: 35, position: preset.camera.position }}
          >
            <TerrainScene
              heightfield={heightfield}
              preset={preset}
              hotspots={hotspots}
              mode={mode}
              labelsVisible={labelsVisible}
              resetToken={resetToken}
              onSelectHotspot={setSelectedHotspot}
            />
          </Canvas>
        )}
        <div className="landform-drag-hint">{DRAG_HINTS[mode]}</div>
        {selectedHotspot ? (
          <aside className="landform-hotspot-detail">
            <button
              type="button"
              onClick={() => setSelectedHotspot(null)}
              aria-label="关闭标注说明"
            >
              <EyeSlash size={13} />
            </button>
            <strong>{selectedHotspot.label}</strong>
            <p>{selectedHotspot.description}</p>
          </aside>
        ) : null}
      </div>
      {mode === "section" && profile ? (
        <ElevationProfile data={profile} />
      ) : null}
    </section>
  );
}
