import { useEffect, useMemo } from "react";
import { buildSlabGeometry } from "./geometry";
import type { Heightfield } from "./heightfield";
import type { TerrainPreset } from "./presets";

/** The continuous Terrain Slab: top surface + side walls + flat base (museum slice). */
export function TerrainSlab({
  heightfield,
  preset,
}: {
  heightfield: Heightfield;
  preset: TerrainPreset;
}) {
  const geometry = useMemo(
    () =>
      buildSlabGeometry(heightfield, {
        thickness: preset.thickness,
        waterLevel: preset.waterLevel,
        tint: preset.tint,
      }),
    [heightfield, preset],
  );
  useEffect(() => () => geometry.dispose(), [geometry]);
  return (
    <mesh geometry={geometry} receiveShadow castShadow>
      <meshStandardMaterial vertexColors roughness={0.96} metalness={0} />
    </mesh>
  );
}

export function WaterPlane({ preset }: { preset: TerrainPreset }) {
  if (preset.waterLevel == null) return null;
  return (
    <mesh
      rotation-x={-Math.PI / 2}
      position={[0, preset.waterLevel + 0.002, 0]}
    >
      <planeGeometry args={[8, 8]} />
      <meshStandardMaterial
        color="#7e989e"
        transparent
        opacity={0.5}
        roughness={0.35}
        metalness={0}
        depthWrite={false}
      />
    </mesh>
  );
}
