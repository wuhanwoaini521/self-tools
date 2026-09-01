import { Html } from "@react-three/drei/web/Html";
import { useMemo } from "react";
import type { LandformHotspot } from "../landform3dData";
import { heightAt, type Heightfield } from "./heightfield";
import type { TerrainPreset } from "./presets";

/**
 * Map-style labels: a small dot + short rule + text, anchored onto the actual terrain
 * surface (never floating in the air).
 */
export function TerrainLabels({
  heightfield,
  preset,
  items,
  onSelect,
}: {
  heightfield: Heightfield;
  preset: TerrainPreset;
  items: LandformHotspot[];
  onSelect: (item: LandformHotspot) => void;
}) {
  const anchored = useMemo(() => {
    const water = preset.waterLevel;
    return items.map((item) => {
      const anchor = preset.labelAnchors[item.id] ?? [
        item.position[0],
        item.position[2],
      ];
      let y = heightAt(heightfield, anchor[0], anchor[1]);
      if (water != null && y < water) y = water + 0.06;
      return { item, x: anchor[0], y: y + 0.02, z: anchor[1] };
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [heightfield, preset, items]);

  return (
    <>
      {anchored.map(({ item, x, y, z }) => (
        <group key={item.id} position={[x, y, z]}>
          <mesh>
            <sphereGeometry args={[0.05, 12, 12]} />
            <meshBasicMaterial color="#bd713c" />
          </mesh>
          <Html distanceFactor={10} position={[0.11, 0.05, 0]} center>
            <button
              type="button"
              className="landform-hotspot"
              onClick={() => onSelect(item)}
            >
              <i />
              {item.label}
            </button>
          </Html>
        </group>
      ))}
    </>
  );
}
