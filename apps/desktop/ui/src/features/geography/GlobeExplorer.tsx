import { OrbitControls } from "@react-three/drei/core/OrbitControls";
import { Stars } from "@react-three/drei/core/Stars";
import { Html } from "@react-three/drei/web/Html";
import { Canvas, useFrame, useThree } from "@react-three/fiber";
import {
  ArrowsClockwise,
  Pause,
  Play,
} from "@phosphor-icons/react";
import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import * as THREE from "three";
import type { OrbitControls as OrbitControlsImpl } from "three-stdlib";
/** NASA Blue Marble 4K（公有领域），用于提高陆地纹理清晰度。 */
import earthDayUrl from "./assets/earth-day.jpg";
import earthColorUrl from "./assets/earth-color.webp";
import {
  type LandformGlobeMarker,
  landformCategories,
} from "./globeLandformMarkers";
import {
  GLOBE_RADIUS,
  isFacingCamera,
  latLngToVector3,
} from "./globeMath";

const CATEGORY_COLORS: Record<string, string> = {
  "构造与地形骨架": "#e8b86d",
  火山地貌: "#e07050",
  流水地貌: "#5aa7e6",
  冰川地貌: "#d7eef8",
  岩溶地貌: "#7bc47f",
  "风成与干旱地貌": "#d4a574",
  "海岸与海洋地貌": "#4aa3d8",
  "重力与坡地地貌": "#c48978",
};

interface GlobeExplorerProps {
  active: boolean;
  markers: LandformGlobeMarker[];
  selectedId?: string | null;
  onSelect: (id: string) => void;
  onOpenLandform: (id: string) => void;
}

function prefersReducedMotion(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

function prepareEarthTexture(texture: THREE.Texture, anisotropy: number) {
  texture.colorSpace = THREE.SRGBColorSpace;
  texture.wrapS = THREE.ClampToEdgeWrapping;
  texture.wrapT = THREE.ClampToEdgeWrapping;
  texture.minFilter = THREE.LinearMipmapLinearFilter;
  texture.magFilter = THREE.LinearFilter;
  texture.anisotropy = anisotropy;
  texture.generateMipmaps = true;
  texture.needsUpdate = true;
}

function loadEquirectTexture(url: string, anisotropy: number): Promise<THREE.Texture> {
  return new Promise((resolve, reject) => {
    const loader = new THREE.TextureLoader();
    loader.load(
      url,
      (texture) => {
        prepareEarthTexture(texture, anisotropy);
        resolve(texture);
      },
      undefined,
      () => reject(new Error(`earth-texture-failed:${url}`)),
    );
  });
}

function EarthMesh() {
  const { gl } = useThree();
  const meshRef = useRef<THREE.Mesh>(null);
  const [colorMap, setColorMap] = useState<THREE.Texture | null>(null);

  useEffect(() => {
    let cancelled = false;
    let uploaded: THREE.Texture | null = null;
    void (async () => {
      const anisotropy = gl.capabilities.getMaxAnisotropy();
      for (const source of [earthDayUrl, earthColorUrl]) {
        try {
          const texture = await loadEquirectTexture(source, anisotropy);
          if (cancelled) {
            texture.dispose();
            return;
          }
          gl.initTexture(texture);
          uploaded = texture;
          setColorMap(texture);
          return;
        } catch {
          /* 4K JPEG 优先，低清 WebP 回退 */
        }
      }
    })();
    return () => {
      cancelled = true;
      uploaded?.dispose();
    };
  }, [gl]);

  useEffect(() => {
    const material = meshRef.current?.material;
    if (!colorMap || !material || Array.isArray(material)) return;
    const lambert = material as THREE.MeshLambertMaterial;
    lambert.map = colorMap;
    lambert.color.set("#ffffff");
    lambert.needsUpdate = true;
  }, [colorMap]);

  return (
    <mesh ref={meshRef}>
      <sphereGeometry args={[GLOBE_RADIUS, 192, 128]} />
      <meshLambertMaterial color={colorMap ? "#ffffff" : "#1b4f78"} />
    </mesh>
  );
}

function Atmosphere() {
  return (
    <mesh scale={1.045}>
      <sphereGeometry args={[GLOBE_RADIUS, 64, 48]} />
      <meshBasicMaterial
        color="#6db7ff"
        transparent
        opacity={0.07}
        side={THREE.BackSide}
        depthWrite={false}
      />
    </mesh>
  );
}

function MarkerBubble({
  marker,
  selected,
  compact,
  onSelect,
}: {
  marker: LandformGlobeMarker;
  selected: boolean;
  compact: boolean;
  onSelect: (id: string) => void;
}) {
  const position = useMemo(
    () => latLngToVector3(marker.latitude, marker.longitude, GLOBE_RADIUS + 0.04),
    [marker.latitude, marker.longitude],
  );
  const [visible, setVisible] = useState(true);
  const color = CATEGORY_COLORS[marker.category] ?? "#8eb4e8";

  useFrame(({ camera }) => {
    const facing = isFacingCamera(position, camera.position);
    setVisible((current) => (current === facing ? current : facing));
  });

  if (!visible) return null;

  return (
    <group position={position}>
      <mesh>
        <sphereGeometry args={[selected ? 0.035 : 0.024, 12, 12]} />
        <meshBasicMaterial color={color} />
      </mesh>
      <Html
        center
        transform={false}
        zIndexRange={[20, 0]}
        style={{ pointerEvents: "auto" }}
      >
        <button
          type="button"
          className={
            "geo-globe-bubble" +
            (selected ? " is-selected" : "") +
            (compact && !selected && !marker.featured ? " is-compact" : "")
          }
          style={{ "--geo-bubble": color } as CSSProperties}
          onPointerDown={(event) => event.stopPropagation()}
          onClick={(event) => {
            event.stopPropagation();
            onSelect(marker.id);
          }}
        >
          <i />
          <span>
            <strong>{marker.title}</strong>
            {compact && !selected && !marker.featured ? null : (
              <small>{marker.example}</small>
            )}
          </span>
        </button>
      </Html>
    </group>
  );
}

function GlobeScene({
  markers,
  selectedId,
  autoRotate,
  onSelect,
  onDrag,
}: {
  markers: LandformGlobeMarker[];
  selectedId?: string | null;
  autoRotate: boolean;
  onSelect: (id: string) => void;
  onDrag: () => void;
}) {
  const controlsRef = useRef<OrbitControlsImpl | null>(null);
  const compact = markers.length > 18;

  return (
    <>
      <color attach="background" args={["#071018"]} />
      <Stars radius={80} depth={40} count={1200} factor={2.4} saturation={0} fade speed={0.4} />
      <ambientLight intensity={0.78} />
      <hemisphereLight args={["#d7ebff", "#0b1c2c", 0.5]} />
      <directionalLight position={[6, 3.4, 8]} intensity={1.35} color="#fff4e2" />
      <directionalLight position={[-7, 1.2, -4]} intensity={0.42} color="#8eb6dd" />
      <EarthMesh />
      <Atmosphere />
      {markers.map((marker) => (
        <MarkerBubble
          key={marker.id}
          marker={marker}
          selected={selectedId === marker.id}
          compact={compact}
          onSelect={onSelect}
        />
      ))}
      <OrbitControls
        ref={controlsRef}
        makeDefault
        enablePan={false}
        enableZoom
        enableRotate
        autoRotate={autoRotate}
        autoRotateSpeed={0.38}
        minDistance={3.15}
        maxDistance={7.6}
        minPolarAngle={0.22}
        maxPolarAngle={Math.PI - 0.22}
        rotateSpeed={0.55}
        zoomSpeed={0.7}
        onStart={onDrag}
      />
    </>
  );
}

export function GlobeExplorer({
  active,
  markers,
  selectedId,
  onSelect,
  onOpenLandform,
}: GlobeExplorerProps) {
  const [autoRotate, setAutoRotate] = useState(() => !prefersReducedMotion());
  const [category, setCategory] = useState<string | "all">("all");
  const resumeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const categories = useMemo(() => landformCategories(markers), [markers]);
  const visibleMarkers = useMemo(
    () =>
      category === "all"
        ? markers
        : markers.filter((marker) => marker.category === category),
    [markers, category],
  );
  const selected = markers.find((marker) => marker.id === selectedId) ?? null;

  useEffect(() => {
    return () => {
      if (resumeTimer.current) clearTimeout(resumeTimer.current);
    };
  }, []);

  const handleDrag = () => {
    if (prefersReducedMotion()) return;
    setAutoRotate(false);
    if (resumeTimer.current) clearTimeout(resumeTimer.current);
    resumeTimer.current = setTimeout(() => setAutoRotate(true), 2400);
  };

  return (
    <div className="geo-globe-stage">
      <Canvas
        className="geo-globe-canvas"
        frameloop={active ? "always" : "never"}
        dpr={[1, 2]}
        camera={{ position: [0.35, 1.05, 5.35], fov: 38, near: 0.1, far: 200 }}
        gl={{ antialias: true, alpha: false, outputColorSpace: THREE.SRGBColorSpace }}
        onCreated={({ gl }) => {
          gl.outputColorSpace = THREE.SRGBColorSpace;
          gl.setClearColor("#071018");
        }}
      >
        <GlobeScene
          markers={visibleMarkers}
          selectedId={selectedId}
          autoRotate={autoRotate && active}
          onSelect={onSelect}
          onDrag={handleDrag}
        />
      </Canvas>
      <div className="geo-globe-hud">
        <div className="geo-globe-hud-top">
          <p className="geo-globe-hint">拖动旋转地球 · 滚轮缩放 · 点气泡进入地貌</p>
          <button
            type="button"
            className={"geo-globe-rotate" + (autoRotate ? " is-on" : "")}
            aria-pressed={autoRotate}
            onClick={() => {
              if (resumeTimer.current) clearTimeout(resumeTimer.current);
              setAutoRotate((value) => !value);
            }}
          >
            {autoRotate ? <Pause size={14} /> : <Play size={14} />}
            {autoRotate ? "自动旋转中" : "手动旋转"}
          </button>
        </div>
        <div className="geo-globe-filters" role="toolbar" aria-label="地貌类型">
          <button
            type="button"
            className={category === "all" ? "selected" : ""}
            onClick={() => setCategory("all")}
          >
            全部地貌
          </button>
          {categories.map((item) => (
            <button
              type="button"
              key={item}
              className={category === item ? "selected" : ""}
              onClick={() => setCategory(item)}
            >
              {item.replace(/地貌$/, "")}
            </button>
          ))}
        </div>
      </div>
      {selected ? (
        <aside className="geo-globe-card" aria-live="polite">
          <span className="geo-card-type">{selected.category}</span>
          <h3>{selected.title}</h3>
          <p>{selected.subtitle}</p>
          <small>
            {selected.place}
            <br />
            {selected.latitude.toFixed(2)}°, {selected.longitude.toFixed(2)}° · {selected.example}
          </small>
          <button type="button" onClick={() => onOpenLandform(selected.id)}>
            打开地貌接口 <ArrowsClockwise size={14} aria-hidden="true" />
          </button>
        </aside>
      ) : null}
    </div>
  );
}
