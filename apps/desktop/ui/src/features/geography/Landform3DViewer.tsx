import { Canvas, useThree } from "@react-three/fiber";
import { Html } from "@react-three/drei/web/Html";
import { Line } from "@react-three/drei/core/Line";
import { OrbitControls } from "@react-three/drei/core/OrbitControls";
import { ArrowClockwise, ArrowsClockwise, Crosshair, Eye, EyeSlash, SelectionBackground } from "@phosphor-icons/react";
import { useEffect, useMemo, useRef, useState, type RefObject } from "react";
import * as THREE from "three";
import type { OrbitControls as OrbitControlsImpl } from "three-stdlib";
import type { LandformHotspot, LandformViewerConfig, LandformViewMode } from "./landform3dData";

interface Landform3DViewerProps {
  title: string;
  config: LandformViewerConfig;
}

const VIEW_OPTIONS: { id: LandformViewMode; label: string; icon: typeof Eye }[] = [
  { id: "model", label: "模型", icon: Eye },
  { id: "section", label: "剖面", icon: SelectionBackground },
  { id: "labels", label: "标注", icon: Crosshair },
];

function terrainHeight(type: LandformViewerConfig["modelType"], x: number, z: number) {
  const r = Math.hypot(x, z);
  if (type === "mountains") return Math.max(0, 2.7 * Math.exp(-((x + 1.7) ** 2 + (z + .2) ** 2) / 2.4) + 2.35 * Math.exp(-((x - 1.25) ** 2 + (z - .35) ** 2) / 2.1) + 1.25 * Math.exp(-((x - .1) ** 2 + (z + 2.1) ** 2) / 2.8) - .55 * Math.exp(-((x - .2) ** 2 + (z - .1) ** 2) / .6));
  if (type === "plateau") return 1.8 / (1 + Math.exp((r - 3.2) * 3.2)) + .09 * Math.sin(x * 3) * Math.cos(z * 2) - .6 * Math.exp(-((x - .2) ** 2) / .16);
  if (type === "basin") return .18 + 2.15 * (1 - Math.exp(-((r / 2.85) ** 4))) + .16 * Math.sin(x * 2.2) * Math.cos(z * 2.8);
  if (type === "plain") return .18 + .08 * Math.sin(x * 2.5) * Math.cos(z * 2.2) + .1 * Math.exp(-((x + 3) ** 2 + (z - 2) ** 2) / 2);
  if (type === "river") return .25 + .9 * Math.exp(-((x + 2.8) ** 2) / 2.5) + .55 * Math.exp(-((x - 2.5) ** 2) / 3) - .38 * Math.exp(-((x - .55 * Math.sin(z * 1.15)) ** 2) / .14);
  if (type === "volcanic") return .18 + 2.65 * Math.exp(-(r ** 2) / 3.1) - .5 * Math.exp(-(r ** 2) / .18);
  if (type === "glacial") return .25 + 2.2 * Math.exp(-((x - 2.65) ** 2) / 1.7) + 2.2 * Math.exp(-((x + 2.65) ** 2) / 1.7) - .18 * Math.exp(-(x ** 2) / 1.5);
  if (type === "karst") return .2 + .22 * Math.sin(x * 2.4) * Math.cos(z * 2.3);
  if (type === "desert") return .15 + .84 * ((Math.sin(x * 1.9 + z * .45) + 1) / 2) ** 2 + .26 * Math.sin(z * 2.1 + x * .2);
  if (type === "coastal") return x < 0 ? .1 + .04 * Math.sin(z * 3) : .15 + .95 / (1 + Math.exp(-(x - 1.7) * 3.5));
  if (type === "structural") return .32 + .9 * Math.abs(x) + .2 * Math.sin(z * 2) - .75 * Math.exp(-(x ** 2) / .42);
  if (type === "mass-movement") return .18 + 2.65 / (1 + Math.exp((x - .1) * 1.35)) + .1 * Math.sin(z * 2.1);
  return .15 + .5 * Math.exp(-((x - 2.4) ** 2 + (z + 1) ** 2) / 2) + .25 * Math.exp(-((x + 2.2) ** 2 + (z - 1.3) ** 2) / 3);
}

function TerrainSurface({ type }: { type: LandformViewerConfig["modelType"] }) {
  const geometry = useMemo(() => {
    const next = new THREE.PlaneGeometry(8, 8, 34, 34);
    const position = next.attributes.position;
    for (let index = 0; index < position.count; index += 1) position.setZ(index, terrainHeight(type, position.getX(index), position.getY(index)));
    position.needsUpdate = true;
    next.computeVertexNormals();
    return next;
  }, [type]);
  useEffect(() => () => geometry.dispose(), [geometry]);
  return <mesh geometry={geometry} rotation-x={-Math.PI / 2} receiveShadow castShadow><meshStandardMaterial color="#aa9468" roughness={.86} metalness={0} flatShading /></mesh>;
}

function RiverPath({ type }: { type: LandformViewerConfig["modelType"] }) {
  if (type !== "plain" && type !== "river") return null;
  const points: [number, number, number][] = Array.from({ length: 28 }, (_, index) => {
    const z = -3.8 + index * .28;
    const x = type === "plain" ? .65 * Math.sin(z * 1.12) : .55 * Math.sin(z * 1.15);
    return [x, terrainHeight(type, x, z) + .05, z];
  });
  return <Line points={points} color="#6f9ea8" lineWidth={3.2} transparent opacity={.92} />;
}

function IslandArc() {
  return <group>
    <mesh position={[1.15, .85, -.45]} castShadow><coneGeometry args={[.78, 2.4, 20]} /><meshStandardMaterial color="#b08159" roughness={.88} /></mesh>
    <mesh position={[2.45, .48, .85]} castShadow><coneGeometry args={[.5, 1.3, 18]} /><meshStandardMaterial color="#b58f64" roughness={.88} /></mesh>
    <mesh position={[-2.8, -.14, .65]} rotation={[-Math.PI / 2, 0, 0]}><torusGeometry args={[.52, .1, 10, 24]} /><meshStandardMaterial color="#628e9a" roughness={.9} /></mesh>
    <Line points={[[-3.6, .08, .7], [-1.4, .02, .2], [.35, -.3, -.2]]} color="#7b6854" lineWidth={2} dashed dashScale={2} />
  </group>;
}

function SceneFeatures({ type }: { type: LandformViewerConfig["modelType"] }) {
  if (type === "island-arc") return <IslandArc />;
  if (type === "volcanic") return <group><mesh position={[0, 1.6, 0]} castShadow><coneGeometry args={[1.5, 2.75, 30]} /><meshStandardMaterial color="#a97654" roughness={.88} /></mesh><mesh position={[0, 3, 0]} rotation-x={-Math.PI / 2}><torusGeometry args={[.42, .12, 10, 24]} /><meshStandardMaterial color="#6f5543" roughness={.92} /></mesh></group>;
  if (type === "glacial") return <mesh position={[0, .43, -.25]} rotation-x={-Math.PI / 2}><planeGeometry args={[1.5, 5.7, 10, 18]} /><meshStandardMaterial color="#bdd6d5" roughness={.78} transparent opacity={.82} /></mesh>;
  if (type === "karst") return <group>{[[-1.45, 1.22, -.3, .56, 2.35], [1.15, .86, .7, .48, 1.7], [2.3, 1.15, -1.25, .42, 2.25]].map(([x, y, z, radius, height]) => <mesh key={`${x}-${z}`} position={[x, y, z]} castShadow><coneGeometry args={[radius, height, 12]} /><meshStandardMaterial color="#a2926d" roughness={.92} /></mesh>)}</group>;
  if (type === "coastal") return <mesh position={[-2.1, .18, 0]} rotation-x={-Math.PI / 2}><planeGeometry args={[3.8, 8]} /><meshStandardMaterial color="#83aeb4" roughness={.58} transparent opacity={.88} /></mesh>;
  if (type === "structural") return <Line points={[[0, .3, -3.8], [0, 1.15, 0], [0, .3, 3.8]]} color="#735844" lineWidth={2.4} dashed dashScale={2} />;
  if (type === "mass-movement") return <mesh position={[.3, 1.15, .1]} rotation-z={-.45} castShadow><coneGeometry args={[1.5, 3.35, 4]} /><meshStandardMaterial color="#a5825b" roughness={.94} /></mesh>;
  return null;
}

function SectionProfile({ type }: { type: LandformViewerConfig["modelType"] }) {
  const profiles: Record<LandformViewerConfig["modelType"], number[]> = {
    mountains: [.2, .55, 2.9, 1.35, .52, 2.55, 1.05, .35], plateau: [.2, .5, 2.15, 2.12, 2.08, 1.95, .65, .25], basin: [2.3, 1.5, .5, .24, .25, .48, 1.6, 2.4], plain: [.25, .26, .27, .25, .28, .3, .28, .28], river: [1.15, 1.1, .85, .34, .32, .75, 1.05, .95], "island-arc": [.25, .16, -.35, -.58, .18, 2.35, 1.25, .45], volcanic: [.2, .35, .85, 2.5, 3.1, 2.45, .75, .25], glacial: [2.4, 1.85, .65, .28, .28, .65, 1.85, 2.4], karst: [.3, 1.75, .35, .18, .25, 1.55, .45, .3], desert: [.25, 1.15, .45, 1.3, .5, 1.05, .42, .26], coastal: [.1, .12, .15, .22, .45, 1.25, 1.45, 1.55], structural: [1.9, 1.5, .42, .25, .25, .42, 1.5, 1.9], "mass-movement": [2.85, 2.25, 1.45, .65, .35, .28, .22, .2],
  };
  const points = profiles[type].map((height, index) => [-3.7 + index * 1.05, height, 0] as [number, number, number]);
  const layers = type === "island-arc" ? ["#d2ad7a", "#b78661", "#8f6d57"] : ["#d4bf91", "#b99868", "#967657"];
  return <group>
    {layers.map((color, index) => <mesh key={color} position={[0, -.35 - index * .42, .05]}><boxGeometry args={[8, .4, .9]} /><meshStandardMaterial color={color} roughness={.94} /></mesh>)}
    <Line points={points} color="#6b513d" lineWidth={3} />
    {type === "island-arc" ? <Line points={[[-3.6, .12, .15], [-1.1, -.45, .15], [.85, .72, .15]]} color="#6c8790" lineWidth={2} dashed dashScale={2} /> : null}
  </group>;
}

function Hotspots({ items, onSelect }: { items: LandformHotspot[]; onSelect: (item: LandformHotspot) => void }) {
  return <>{items.map((item) => <group key={item.id} position={item.position}>
    <mesh><sphereGeometry args={[.075, 16, 16]} /><meshBasicMaterial color="#bd713c" /></mesh>
    <Html distanceFactor={10} position={[.09, .08, 0]}><button type="button" className="landform-hotspot" onClick={() => onSelect(item)}><i />{item.label}</button></Html>
  </group>)}</>;
}

function CameraRig({ position, resetToken, controlsRef }: { position: [number, number, number]; resetToken: number; controlsRef: RefObject<OrbitControlsImpl | null> }) {
  const { camera } = useThree();
  useEffect(() => {
    camera.position.set(...position);
    controlsRef.current?.target.set(0, .9, 0);
    controlsRef.current?.update();
  }, [camera, controlsRef, position, resetToken]);
  return null;
}

function ViewerScene({ config, mode, resetToken, autoRotate, onSelect }: { config: LandformViewerConfig; mode: LandformViewMode; resetToken: number; autoRotate: boolean; onSelect: (item: LandformHotspot) => void }) {
  const controlsRef = useRef<OrbitControlsImpl | null>(null);
  return <>
    <CameraRig position={mode === "section" ? [0, 3, 9] : config.cameraPosition} resetToken={resetToken} controlsRef={controlsRef} />
    <color attach="background" args={["#e7e1d4"]} />
    <ambientLight intensity={1.25} />
    <directionalLight position={[5, 7, 5]} intensity={2.2} castShadow shadow-mapSize-width={1024} shadow-mapSize-height={1024} />
    <directionalLight position={[-4, 2, -2]} intensity={.45} />
    <group position={[0, -.18, 0]}>{mode === "section" ? <SectionProfile type={config.modelType} /> : <><TerrainSurface type={config.modelType} /><RiverPath type={config.modelType} /><SceneFeatures type={config.modelType} />{mode === "labels" ? <Hotspots items={config.hotspots} onSelect={onSelect} /> : null}</>}</group>
    <mesh rotation-x={-Math.PI / 2} position={[0, -.22, 0]} receiveShadow><circleGeometry args={[5.7, 64]} /><shadowMaterial transparent opacity={.18} /></mesh>
    <OrbitControls ref={controlsRef} enablePan={false} minDistance={5.2} maxDistance={13} maxPolarAngle={Math.PI / 2.02} autoRotate={autoRotate && mode !== "section"} autoRotateSpeed={.6} makeDefault />
  </>;
}

function StaticFallback({ title, image }: { title: string; image?: string }) {
  return <div className="landform-static-fallback" role="img" aria-label={`${title}静态地貌示意图`}>{image ? <img src={image} alt="" /> : <svg viewBox="0 0 640 260" aria-hidden="true"><path d="M0 198 L85 158 L160 185 L236 75 L318 145 L405 108 L492 178 L560 132 L640 184 V260 H0Z" /><path d="M0 204 C130 182 182 208 294 180 S476 214 640 173" /></svg>}<span>当前设备无法启用 3D 观察，以下为静态地貌示意。</span></div>;
}

export function Landform3DViewer({ title, config }: Landform3DViewerProps) {
  const [mode, setMode] = useState<LandformViewMode>("model");
  const [autoRotate, setAutoRotate] = useState(false);
  const [resetToken, setResetToken] = useState(0);
  const [webglAvailable, setWebglAvailable] = useState<boolean | null>(null);
  const [selectedHotspot, setSelectedHotspot] = useState<LandformHotspot | null>(null);
  useEffect(() => { const canvas = document.createElement("canvas"); setWebglAvailable(Boolean(canvas.getContext("webgl") || canvas.getContext("experimental-webgl"))); }, []);
  useEffect(() => { setMode("model"); setSelectedHotspot(null); setResetToken((value) => value + 1); }, [config.modelType]);
  if (webglAvailable === false) return <section className="landform-viewer landform-viewer-fallback"><StaticFallback title={title} image={config.fallbackImage} /></section>;
  return <section className="landform-viewer" aria-label={`${title} 3D 教学模型`}>
    <header className="landform-viewer-head"><div><span>3D LANDFORM STUDY</span><strong>{title} · 可交互模型</strong></div><div className="landform-viewer-actions"><div role="group" aria-label="地貌模型视图">{VIEW_OPTIONS.map(({ id, label, icon: Icon }) => <button key={id} type="button" className={mode === id ? "selected" : ""} aria-label={`切换到${label}视图`} onClick={() => setMode(id)}><Icon size={14} />{label}</button>)}</div><button type="button" aria-label="重置模型视角" title="重置视角" onClick={() => setResetToken((value) => value + 1)}><ArrowsClockwise size={15} /></button><button type="button" className={autoRotate ? "selected" : ""} aria-label={autoRotate ? "关闭自动旋转" : "开启自动旋转"} title={autoRotate ? "关闭自动旋转" : "开启自动旋转"} onClick={() => setAutoRotate((value) => !value)}><ArrowClockwise size={15} /></button></div></header>
    <div className="landform-canvas-wrap">
      {webglAvailable === null ? <div className="landform-loading">正在准备地貌模型…</div> : <Canvas shadows dpr={[1, 1.5]} camera={{ fov: 35, position: config.cameraPosition }}><ViewerScene config={config} mode={mode} resetToken={resetToken} autoRotate={autoRotate} onSelect={setSelectedHotspot} /></Canvas>}
      <div className="landform-drag-hint">{mode === "section" ? "教学剖面 · 拖动观察结构" : "拖动旋转 · 滚轮缩放"}</div>
      {selectedHotspot ? <aside className="landform-hotspot-detail"><button type="button" onClick={() => setSelectedHotspot(null)} aria-label="关闭标注说明"><EyeSlash size={13} /></button><strong>{selectedHotspot.label}</strong><p>{selectedHotspot.description}</p></aside> : null}
    </div>
  </section>;
}
