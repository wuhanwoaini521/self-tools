import { Html } from "@react-three/drei/web/Html";
import { Line } from "@react-three/drei/core/Line";
import { useMemo } from "react";
import * as THREE from "three";
import type { LandformHotspot } from "../landform3dData";
import { heightAt, type Heightfield } from "./heightfield";
import type { TerrainPreset } from "./presets";

const PIN_COLORS: Record<string, string> = {
  crest: "#c45c26",
  slope: "#1f6fbf",
  vale: "#1e7a48",
  peak: "#c45c26",
  ridge: "#8a5a32",
  valley: "#1e7a48",
  floor: "#1e7a48",
};

const DEFAULT_OFFSET: [number, number, number] = [0.08, 0.85, 0.08];

function pinColor(id: string): string {
  return PIN_COLORS[id] ?? "#bd713c";
}

function Leader({
  from,
  to,
  color,
}: {
  from: [number, number, number];
  to: [number, number, number];
  color: string;
}) {
  const geom = useMemo(() => {
    const a = new THREE.Vector3(...from);
    const b = new THREE.Vector3(...to);
    const dir = b.clone().sub(a);
    const length = dir.length();
    const mid = a.clone().add(b).multiplyScalar(0.5);
    const quat = new THREE.Quaternion().setFromUnitVectors(
      new THREE.Vector3(0, 1, 0),
      dir.clone().normalize(),
    );
    return { mid: mid.toArray() as [number, number, number], quat, length };
  }, [from, to]);

  if (geom.length < 0.08) return null;
  return (
    <mesh position={geom.mid} quaternion={geom.quat}>
      <cylinderGeometry args={[0.02, 0.02, geom.length, 8]} />
      <meshBasicMaterial color={color} />
    </mesh>
  );
}

function HillTeachingMarks({
  heightfield,
  preset,
}: {
  heightfield: Heightfield;
  preset: TerrainPreset;
}) {
  const crest = preset.labelAnchors.crest;
  const vale = preset.labelAnchors.vale;
  const slope = preset.labelAnchors.slope;
  const marks = useMemo(() => {
    if (!crest || !vale || !slope) return null;
    const crestY = heightAt(heightfield, crest[0], crest[1]);
    const slopeLine: [number, number, number][] = [];
    for (let i = 0; i <= 10; i += 1) {
      const t = 0.12 + (0.72 * i) / 10;
      const x = crest[0] + (vale[0] - crest[0]) * t;
      const z = crest[1] + (vale[1] - crest[1]) * t;
      slopeLine.push([x, heightAt(heightfield, x, z) + 0.05, z]);
    }
    return {
      crest: [crest[0], crestY + 0.03, crest[1]] as [number, number, number],
      slopeLine,
    };
  }, [heightfield, crest, vale, slope]);

  if (!marks) return null;
  return (
    <>
      <mesh position={marks.crest} rotation={[-Math.PI / 2, 0, 0]}>
        <ringGeometry args={[0.16, 0.28, 28]} />
        <meshBasicMaterial
          color="#c45c26"
          transparent
          opacity={0.9}
          side={THREE.DoubleSide}
        />
      </mesh>
      <Line
        points={marks.slopeLine}
        color="#1f6fbf"
        lineWidth={3}
        dashed={false}
      />
    </>
  );
}

/**
 * 教学标注：钉子钉在真实地表，再拉出引线，文字离开模型，避免三张标签糊在同一点。
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
    return items.map((item, index) => {
      const anchor = preset.labelAnchors[item.id] ?? [
        item.position[0],
        item.position[2],
      ];
      let y = heightAt(heightfield, anchor[0], anchor[1]);
      if (water != null && y < water) y = water + 0.06;
      const offset = preset.labelOffsets?.[item.id] ?? DEFAULT_OFFSET;
      const pin: [number, number, number] = [anchor[0], y + 0.03, anchor[1]];
      const card: [number, number, number] = [
        pin[0] + offset[0],
        pin[1] + offset[1],
        pin[2] + offset[2],
      ];
      return {
        item,
        index: index + 1,
        color: pinColor(item.id),
        pin,
        card,
      };
    });
  }, [heightfield, preset, items]);

  return (
    <>
      {preset.type === "hills" ? (
        <HillTeachingMarks heightfield={heightfield} preset={preset} />
      ) : null}
      {anchored.map(({ item, index, color, pin, card }) => (
        <group key={item.id}>
          <mesh position={pin}>
            <sphereGeometry args={[0.07, 14, 14]} />
            <meshBasicMaterial color={color} />
          </mesh>
          <Leader from={pin} to={card} color={color} />
          <mesh position={card}>
            <sphereGeometry args={[0.035, 10, 10]} />
            <meshBasicMaterial color={color} />
          </mesh>
          <Html
            position={card}
            center
            transform={false}
            occlude={false}
            zIndexRange={[30, 0]}
            style={{ pointerEvents: "auto" }}
          >
            <button
              type="button"
              className={"landform-hotspot landform-hotspot--" + item.id}
              onClick={() => onSelect(item)}
            >
              <b>{String(index)}</b>
              <span>{item.label}</span>
            </button>
          </Html>
        </group>
      ))}
    </>
  );
}
