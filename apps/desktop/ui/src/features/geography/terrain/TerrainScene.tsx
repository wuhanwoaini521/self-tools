import { OrbitControls } from "@react-three/drei/core/OrbitControls";
import { useThree } from "@react-three/fiber";
import { useEffect, useRef, type RefObject } from "react";
import type { OrbitControls as OrbitControlsImpl } from "three-stdlib";
import type { LandformHotspot, LandformViewMode } from "../landform3dData";
import type { Heightfield } from "./heightfield";
import type { TerrainPreset } from "./presets";
import { ContourOverlay } from "./ContourOverlay";
import { SectionOverlay } from "./SectionOverlay";
import { TerrainLabels } from "./TerrainLabels";
import { TerrainSlab, WaterPlane } from "./TerrainSlab";

function CameraRig({
      preset,
      resetToken,
      controlsRef,
}: {
      preset: TerrainPreset;
      resetToken: number;
      controlsRef: RefObject<OrbitControlsImpl | null>;
}) {
      const { camera } = useThree();
      useEffect(() => {
            camera.position.set(...preset.camera.position);
            controlsRef.current?.target.set(...preset.camera.target);
            controlsRef.current?.update();
      }, [camera, controlsRef, preset, resetToken]);
      return null;
}

function TerrainLighting() {
      return (
            <>
                  <hemisphereLight args={["#fdf2e0", "#8a7f6b", 0.85]} />
                  <directionalLight
                        castShadow
                        position={[5, 8, 6]}
                        intensity={1.9}
                        shadow-mapSize-width={2048}
                        shadow-mapSize-height={2048}
                        shadow-camera-left={-9}
                        shadow-camera-right={9}
                        shadow-camera-top={9}
                        shadow-camera-bottom={-9}
                        shadow-camera-near={1}
                        shadow-camera-far={30}
                        shadow-radius={6}
                        shadow-bias={-0.0004}
                  />
                  <directionalLight
                        position={[-5, 3, -4]}
                        intensity={0.4}
                        color="#cfc4b0"
                  />
            </>
      );
}

export interface TerrainSceneProps {
      heightfield: Heightfield;
      preset: TerrainPreset;
      /** hotspot items to surface as labels (anchored onto the terrain) */
      hotspots: LandformHotspot[];
      mode: LandformViewMode;
      labelsVisible: boolean;
      resetToken: number;
      onSelectHotspot: (item: LandformHotspot) => void;
      /** 渲染途中 WebGL 上下文被系统回收时回调（用于提示而不是静默黑屏）。 */
      onContextLost?: () => void;
}

export function TerrainScene({
      heightfield,
      preset,
      hotspots,
      mode,
      labelsVisible,
      resetToken,
      onSelectHotspot,
      onContextLost,
}: TerrainSceneProps) {
      const controlsRef = useRef<OrbitControlsImpl | null>(null);
      const showContour = mode === "contour";
      const showSection = mode === "section";
      const onContextLostRef = useRef(onContextLost);
      onContextLostRef.current = onContextLost;

      // 监听 WebGL 上下文丢失：GPU 压力/驱动/WebView 限制会使上下文被回收，
      // 若不处理，画布会静默黑屏。丢失时通知上层做优雅降级（提示 + 重建）。
      const { gl } = useThree();
      useEffect(() => {
            const canvas = gl.domElement;
            const handleLost = (event: Event) => {
                  event.preventDefault();
                  onContextLostRef.current?.();
            };
            canvas.addEventListener("webglcontextlost", handleLost, false);
            return () =>
                  canvas.removeEventListener(
                        "webglcontextlost",
                        handleLost,
                        false,
                  );
      }, [gl]);

      return (
            <>
                  <color attach="background" args={["#e9e3d5"]} />
                  <CameraRig
                        preset={preset}
                        resetToken={resetToken}
                        controlsRef={controlsRef}
                  />
                  <TerrainLighting />
                  <TerrainSlab heightfield={heightfield} preset={preset} />
                  <WaterPlane preset={preset} />
                  {showContour ? (
                        <ContourOverlay
                              heightfield={heightfield}
                              preset={preset}
                        />
                  ) : null}
                  {showSection ? (
                        <SectionOverlay
                              heightfield={heightfield}
                              preset={preset}
                        />
                  ) : null}
                  {labelsVisible ? (
                        <TerrainLabels
                              heightfield={heightfield}
                              preset={preset}
                              items={hotspots}
                              onSelect={onSelectHotspot}
                        />
                  ) : null}
                  <OrbitControls
                        ref={controlsRef}
                        makeDefault
                        enablePan={false}
                        enableZoom
                        enableRotate
                        minDistance={3.6}
                        maxDistance={16}
                        maxPolarAngle={Math.PI / 2 - 0.05}
                        minPolarAngle={0.25}
                  />
            </>
      );
}
