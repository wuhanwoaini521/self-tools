import * as THREE from "three";
import type { Heightfield } from "./heightfield";
import { LAND_RAMP, SEA_RAMP, rampColor, type PaletteTint } from "./palette";

export interface SlabOptions {
  /** vertical thickness of the museum-slice base below the lowest terrain point */
  thickness: number;
  /** if set, terrain below this level is tinted as sea floor */
  waterLevel?: number | null;
  /** subtle per-type hue tint */
  tint?: PaletteTint;
}

/**
 * Build a full Terrain Slab: a continuous top surface, vertical side walls and a flat
 * bottom plane — like a terrain sample cut out of the earth and mounted in a museum
 * case. Normals are smoothed via `computeVertexNormals` so the surface reads as real,
 * eroded terrain (no faceting).
 */
export function buildSlabGeometry(
  hf: Heightfield,
  opts: SlabOptions,
): THREE.BufferGeometry {
  const { extent, count, resolution, data, min } = hf;
  const cell = (2 * extent) / resolution;
  const base = min - opts.thickness;
  const water = opts.waterLevel;

  const total = count * count * 2; // top + bottom copies
  const positions = new Float32Array(total * 3);
  const colors = new Float32Array(total * 3);
  const uvs = new Float32Array(total * 2);
  const indices: number[] = [];

  const tint = opts.tint ?? [1, 1, 1];

  const putColor = (idx: number, h: number) => {
    const t = (h - hf.min) / (hf.max - hf.min || 1);
    const ocean = water != null && h < water;
    const baseRgb = ocean
      ? rampColor(
          SEA_RAMP,
          1 - Math.min(Math.max((h - hf.min) / (water - hf.min || 1), 0), 1),
        )
      : rampColor(LAND_RAMP, t);
    colors[idx * 3] = baseRgb[0] * tint[0];
    colors[idx * 3 + 1] = baseRgb[1] * tint[1];
    colors[idx * 3 + 2] = baseRgb[2] * tint[2];
  };

  // top face
  for (let iz = 0; iz < count; iz += 1) {
    const z = -extent + iz * cell;
    for (let ix = 0; ix < count; ix += 1) {
      const p = iz * count + ix;
      const x = -extent + ix * cell;
      const h = data[p];
      positions[p * 3] = x;
      positions[p * 3 + 1] = h;
      positions[p * 3 + 2] = z;
      putColor(p, h);
      uvs[p * 2] = ix / resolution;
      uvs[p * 2 + 1] = iz / resolution;
    }
  }

  // bottom face (flat, slightly darker)
  const off = count * count;
  for (let iz = 0; iz < count; iz += 1) {
    const z = -extent + iz * cell;
    for (let ix = 0; ix < count; ix += 1) {
      const q = off + iz * count + ix;
      const x = -extent + ix * cell;
      positions[q * 3] = x;
      positions[q * 3 + 1] = base;
      positions[q * 3 + 2] = z;
      const shade = 0.72;
      colors[q * 3] = 0.66 * shade * tint[0];
      colors[q * 3 + 1] = 0.62 * shade * tint[1];
      colors[q * 3 + 2] = 0.56 * shade * tint[2];
      uvs[q * 2] = ix / resolution;
      uvs[q * 2 + 1] = iz / resolution;
    }
  }

  // top triangles (normal +y), bottom triangles (normal -y)
  for (let iz = 0; iz < resolution; iz += 1) {
    for (let ix = 0; ix < resolution; ix += 1) {
      const a = iz * count + ix;
      const b = a + 1;
      const c = b + count;
      const d = a + count;
      indices.push(a, d, b, b, d, c);
      indices.push(off + a, off + b, off + d, off + b, off + c, off + d);
    }
  }

  // side walls (one quad per boundary cell)
  const wall = (t0: number, t1: number) => {
    const b0 = off + t0;
    const b1 = off + t1;
    indices.push(t0, t1, b1, t0, b1, b0);
  };
  // front (z = -extent) and back (z = +extent)
  for (let ix = 0; ix < resolution; ix += 1) {
    wall(0 * count + ix, 0 * count + ix + 1);
    wall(resolution * count + ix + 1, resolution * count + ix);
  }
  // left (x = -extent) and right (x = +extent)
  for (let iz = 0; iz < resolution; iz += 1) {
    wall(iz * count + 0, (iz + 1) * count + 0);
    wall((iz + 1) * count + resolution, iz * count + resolution);
  }

  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.BufferAttribute(positions, 3));
  geometry.setAttribute("color", new THREE.BufferAttribute(colors, 3));
  geometry.setAttribute("uv", new THREE.BufferAttribute(uvs, 2));
  geometry.setIndex(indices);
  geometry.computeVertexNormals();
  return geometry;
}

/**
 * Build just the top surface grid (single-sided) — used as the base geometry for the
 * contour overlay so contour lines track the exact terrain.
 */
export function buildTopSurfaceGeometry(hf: Heightfield): THREE.BufferGeometry {
  const { extent, count, resolution, data } = hf;
  const cell = (2 * extent) / resolution;
  const positions = new Float32Array(count * count * 3);
  const indices: number[] = [];
  for (let iz = 0; iz < count; iz += 1) {
    const z = -extent + iz * cell;
    for (let ix = 0; ix < count; ix += 1) {
      const p = iz * count + ix;
      positions[p * 3] = -extent + ix * cell;
      positions[p * 3 + 1] = data[p];
      positions[p * 3 + 2] = z;
    }
  }
  for (let iz = 0; iz < resolution; iz += 1) {
    for (let ix = 0; ix < resolution; ix += 1) {
      const a = iz * count + ix;
      const b = a + 1;
      const c = b + count;
      const d = a + count;
      indices.push(a, d, b, b, d, c);
    }
  }
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.BufferAttribute(positions, 3));
  geometry.setIndex(indices);
  geometry.computeVertexNormals();
  return geometry;
}
