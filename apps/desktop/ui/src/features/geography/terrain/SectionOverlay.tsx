import { Html } from "@react-three/drei/web/Html";
import { Line } from "@react-three/drei/core/Line";
import { useMemo } from "react";
import { heightAt, type Heightfield } from "./heightfield";
import type { TerrainPreset } from "./presets";

/**
 * A-B section cut: a draggable-free preset transect line laid on the actual terrain
 * surface with A/B end markers, used to derive the real elevation profile.
 */
export function SectionOverlay({
  heightfield,
  preset,
}: {
  heightfield: Heightfield;
  preset: TerrainPreset;
}) {
  const { a, b } = preset.section;

  const points = useMemo(() => {
    const out: [number, number, number][] = [];
    const steps = 140;
    for (let i = 0; i <= steps; i += 1) {
      const t = i / steps;
      const x = a[0] + (b[0] - a[0]) * t;
      const z = a[1] + (b[1] - a[1]) * t;
      out.push([x, heightAt(heightfield, x, z) + 0.035, z]);
    }
    return out;
  }, [heightfield, a, b]);

  const yA = heightAt(heightfield, a[0], a[1]);
  const yB = heightAt(heightfield, b[0], b[1]);

  return (
    <group>
      <Line
        points={points}
        color="#8a5a2e"
        lineWidth={1.6}
        transparent
        opacity={0.9}
      />
      <mesh position={[a[0], yA + 0.12, a[1]]}>
        <sphereGeometry args={[0.06, 12, 12]} />
        <meshBasicMaterial color="#b05a24" />
      </mesh>
      <mesh position={[b[0], yB + 0.12, b[1]]}>
        <sphereGeometry args={[0.06, 12, 12]} />
        <meshBasicMaterial color="#b05a24" />
      </mesh>
      <Html distanceFactor={10} position={[a[0], yA + 0.32, a[1]]} center>
        <span className="landform-section-mark">A</span>
      </Html>
      <Html distanceFactor={10} position={[b[0], yB + 0.32, b[1]]} center>
        <span className="landform-section-mark">B</span>
      </Html>
    </group>
  );
}
