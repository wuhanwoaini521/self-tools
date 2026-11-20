/**
 * Per-landform terrain generators.
 *
 * Each generator returns a `HeightFn` with a *distinct structural signature* — not a
 * one-shape-fit-all noise. The phase-1 hero landforms (mountain, plateau, basin, plain,
 * river, island-arc) get dedicated geometry; the remaining viewer types reuse the same
 * continuous-heightfield machinery so every scene stays scientific and slab-like.
 */

import { type HeightFn, fbm, ridged, rot, smoothstep } from "./heightfield";

// ---------------------------------------------------------------------------
// shared helpers
// ---------------------------------------------------------------------------

/** a meandering river path from (-3.9, ..) to (3.9, ..), water flows +x */
function riverPath2D(): [number, number][] {
  const pts: [number, number][] = [];
  for (let i = 0; i <= 48; i += 1) {
    const t = i / 48;
    const x = -3.9 + 7.8 * t;
    const z =
      0.62 * Math.sin(6.2832 * 0.9 * t + 0.7) +
      0.34 * Math.sin(6.2832 * 2.3 * t - 1.1) +
      0.22 * Math.sin(6.2832 * 5.3 * t + 2.0) +
      0.3 * Math.sin(6.2832 * 0.32 * t - 0.4);
    pts.push([x, z]);
  }
  return pts;
}

interface PathSample {
  x: number;
  z: number;
  /** distance from the river start in world units */
  d: number;
  /** total path length */
  total: number;
}

function buildPathSamples(pts: [number, number][], sub = 6): PathSample[] {
  const samples: PathSample[] = [];
  let acc = 0;
  for (let i = 0; i < pts.length - 1; i += 1) {
    const [ax, az] = pts[i];
    const [bx, bz] = pts[i + 1];
    const seg = Math.hypot(bx - ax, bz - az);
    for (let s = 0; s < sub; s += 1) {
      const t = s / sub;
      samples.push({
        x: ax + (bx - ax) * t,
        z: az + (bz - az) * t,
        d: acc + seg * t,
        total: 0, // filled below
      });
    }
    acc += seg;
  }
  // last point
  const [lx, lz] = pts[pts.length - 1];
  samples.push({ x: lx, z: lz, d: acc, total: 0 });
  const total = acc;
  for (const s of samples) s.total = total;
  return samples;
}

const RIVER_PATH = riverPath2D();
const RIVER_SAMPLES = buildPathSamples(RIVER_PATH);

/** nearest path distance and downstream distance for a world point */
function nearestPath(
  x: number,
  z: number,
  samples: PathSample[] = RIVER_SAMPLES,
): { dist: number; along: number } {
  let best = Infinity;
  let along = 0;
  for (let i = 0; i < samples.length; i += 1) {
    const s = samples[i];
    const dx = x - s.x;
    const dz = z - s.z;
    const dd = dx * dx + dz * dz;
    if (dd < best) {
      best = dd;
      along = s.d;
    }
  }
  return { dist: Math.sqrt(best), along };
}

/** channel excavation: a soft gaussian trough centered on the river line */
function channelIncise(
  x: number,
  z: number,
  samples: PathSample[] = RIVER_SAMPLES,
  halfWidth = 0.5,
  maxDepth = 0.55,
): number {
  const { dist, along } = nearestPath(x, z, samples);
  const total = samples[samples.length - 1].d;
  const alongT = total > 0 ? along / total : 0;
  // width widens downstream, depth deepens downstream
  const w = halfWidth * (0.75 + 0.65 * alongT);
  const depth = maxDepth * (0.55 + 0.6 * alongT);
  return Math.exp(-(dist * dist) / (2 * w * w)) * depth;
}

// ---------------------------------------------------------------------------
// generators
// ---------------------------------------------------------------------------

function bump(x: number, z: number, cx: number, cz: number, rx: number, rz: number, h: number): number {
  const u = (x - cx) / rx;
  const v = (z - cz) / rz;
  return h * Math.exp(-(u * u + v * v));
}

/** 有限半径的馒头丘：出了半径就是 0，丘与丘之间不会糊成一座大山 */
function cosineMound(
  x: number,
  z: number,
  cx: number,
  cz: number,
  radius: number,
  height: number,
): number {
  const d = Math.hypot(x - cx, z - cz);
  if (d >= radius) return 0;
  return height * 0.5 * (1 + Math.cos(Math.PI * (d / radius)));
}

/** 山地与山脉 — 一条弯曲主脊、三座峰、一条山谷 */
export function generateMountainTerrain(seed: number): HeightFn {
  return (x, z) => {
    const ridgeZ = -0.15 + 0.55 * Math.sin(x * 0.42);
    const along = Math.exp(-((z - ridgeZ) ** 2) / 1.35);
    const peaks =
      bump(x, z, -1.55, ridgeZ - 0.05, 1.15, 0.85, 1.55) +
      bump(x, z, 0.15, ridgeZ + 0.12, 1.35, 0.92, 2.05) +
      bump(x, z, 1.85, ridgeZ - 0.18, 1.05, 0.78, 1.25);
    const spur = bump(x, z, -0.2, 1.85, 1.6, 0.7, 0.85);
    const valley = bump(x, z, 0.4, 1.45, 2.4, 0.62, 0.72);
    const grain = 0.045 * fbm(x * 0.55, z * 0.55, seed, 2);
    return Math.max(0.14, 0.28 + along * 1.55 + peaks + spur - valley + grain);
  };
}

/** 丘陵 — 正面一座示范丘，右侧邻丘，中间浅谷；侧视能看清圆顶和缓坡 */
export function generateHillTerrain(_seed: number): HeightFn {
  return (x, z) => {
    const floor = 0.2;
    const demo = cosineMound(x, z, 0, 0, 1.72, 1.08);
    const neighbor = cosineMound(x, z, 3.05, 0.18, 1.42, 0.78);
    const far = cosineMound(x, z, -2.55, 1.85, 1.15, 0.52);
    return floor + Math.max(demo, neighbor, far);
  };
}

/** 高原 — 举起来的大桌子，边缘陡，一条河谷切开桌面 */
export function generatePlateauTerrain(_seed: number): HeightFn {
  return (x, z) => {
    const nx = x / 3.15;
    const nz = z / 2.55;
    const r = Math.hypot(nx, nz);
    const table = smoothstep(1.08, 0.82, r);
    const skirt = 0.22 + 1.85 * table;
    const gorge = Math.exp(-((z + 0.15 * Math.sin(x * 0.7)) ** 2) / 0.18) * table * 0.85;
    return Math.max(0.16, skirt - gorge);
  };
}

/** 盆地 — 四周高、中间低，西南有一个出水口 */
export function generateBasinTerrain(_seed: number): HeightFn {
  const outletAngle = -2.15;
  return (x, z) => {
    const r = Math.hypot(x * 0.92, z);
    const ang = Math.atan2(z, x);
    const ringR = 2.55;
    const ringDist = Math.abs(r - ringR);
    const ring = Math.max(0, 1 - ringDist / 0.95) ** 1.6;
    const outlet = Math.exp(-(((ang - outletAngle) / 0.42) ** 2));
    const floor = 0.22 + 0.08 * smoothstep(2.2, 0.4, r);
    return Math.max(0.16, floor + ring * (1 - 0.92 * outlet) * 1.85);
  };
}

/** 平原 — very low undulation + gentle slope + shallow floodplain river */
export function generatePlainTerrain(seed: number): HeightFn {
  return (x, z) => {
    // gentle regional tilt, then low-amplitude undulation
    const slope = 0.22 + 0.06 * (x / 6 + z / 10);
    const relief = fbm(x * 0.62, z * 0.62, seed, 4) * 0.14;
    const floodplain = fbm(x * 0.9, z * 0.9, seed + 15, 3) * 0.05;
    // land stays clearly above the water level except in the carved channel
    const land = Math.max(0.2, slope + relief + floodplain);
    const channel = channelIncise(x, z, RIVER_SAMPLES, 0.34, 0.26);
    return Math.max(0.03, land - channel);
  };
}

/** 河流 — winding valley between flanks, incised channel + floodplain + alluvial fan */
export function generateRiverTerrain(seed: number): HeightFn {
  return (x, z) => {
    const { dist, along } = nearestPath(x, z);
    const total = 38;
    const alongT = along / total;
    const valleyW = 1.15;
    // flanks rise away from the river with ridge texture
    const flank = smoothstep(valleyW * 0.6, valleyW * 3.2, dist) ** 1.3;
    const flankNoise =
      1.0 +
      0.5 * fbm(x * 0.9, z * 0.9, seed + 3, 4) +
      0.7 * ridged(x * 0.5, z * 0.55, seed + 41, 3);
    const flankH = flank * flankNoise * 1.7;
    // valley floor: floodplain slightly above channel
    const floor = 0.42 + 0.16 * fbm(x * 1.1, z * 1.1, seed + 9, 3);
    // alluvial fan where the river exits the upland (downstream end)
    const fanH =
      smoothstep(0.9, 1.0, alongT) *
      Math.exp(-(dist * dist) / (2 * 1.4 * 1.4)) *
      0.6;
    // channel carve
    const channel = channelIncise(x, z, RIVER_SAMPLES, 0.4, 0.42);
    return Math.max(0.1, flankH + floor - channel + fanH);
  };
}

/** 岛弧 — ocean floor + trench + curved volcanic arc (all built into the terrain) */
export function generateIslandArcTerrain(seed: number): HeightFn {
  // arc center and angle range; arc radius ~ 2.6, centered slightly off-left
  const arcCx = -0.6;
  const arcCz = -0.4;
  const arcR = 2.7;
  const a0 = -1.4;
  const a1 = 1.9;
  const samples: { x: number; z: number; s: number }[] = [];
  const n = 40;
  for (let i = 0; i <= n; i += 1) {
    const t = i / n;
    const a = a0 + (a1 - a0) * t;
    samples.push({
      x: arcCx + arcR * Math.cos(a),
      z: arcCz + arcR * Math.sin(a),
      s: t,
    });
  }
  const nearestArc = (x: number, z: number) => {
    let bd = Infinity;
    let bt = 0;
    for (const s of samples) {
      const dx = x - s.x;
      const dz = z - s.z;
      const dd = dx * dx + dz * dz;
      if (dd < bd) {
        bd = dd;
        bt = s.s;
      }
    }
    return { dist: Math.sqrt(bd), t: bt };
  };
  return (x, z) => {
    // deep ocean floor
    let h = -1.28 + fbm(x * 0.42, z * 0.42, seed, 3) * 0.3;
    // trench: deep linear trough along the arc, seaward (outer) side
    const { dist: arcDist, t } = nearestArc(x, z);
    // side of the arc: positive => inner (landward) side index via signed offset
    const px = arcCx + arcR * Math.cos(a0 + (a1 - a0) * t);
    const pz = arcCz + arcR * Math.sin(a0 + (a1 - a0) * t);
    const sdx = x - px;
    const sdz = z - pz;
    const sign =
      sdx * Math.cos(a0 + (a1 - a0) * t) + sdz * Math.sin(a0 + (a1 - a0) * t) >=
      0
        ? 1
        : -1;
    // trench on the outer (negative sign) side, a band parallel to the arc
    const trenchBand = smoothstep(0.5, 1.6, arcDist);
    h -= trenchBand * (sign < 0 ? 1 : 0.35) * 0.7;
    const arcProfile = Math.exp(-(arcDist * arcDist) / (2 * 0.95 * 0.95));
    // individual cones along the arc from 1-D ridged/noise peaks
    const peaks =
      (1 - Math.abs(2 * fbm(t * 3.2, 0.5, seed + 71, 4) - 1)) ** 2.4 * 1.6 +
      0.7 * (1 - Math.abs(2 * fbm(t * 7.1, 0.5, seed + 79, 3) - 1)) ** 2.6;
    const cone = Math.max(0, 1 - arcDist / 1.05) ** 1.35;
    h += arcProfile * peaks * cone * 1.55;
    // back-arc shelf rises a little toward the mainland side
    h +=
      smoothstep(1.3, 2.6, arcDist) *
      (sign > 0 ? 0.45 : 0) *
      smoothstep(a0, a1, a0 + (a1 - a0) * t) *
      0;
    // shallow ocean noise
    h += 0.05 * fbm(x * 1.4, z * 1.4, seed + 89, 3);
    return h;
  };
}

/** 火山锥 — 光滑圆锥、顶部火山口、一侧熔岩裙 */
export function generateVolcanicTerrain(_seed: number): HeightFn {
  return (x, z) => {
    const r = Math.hypot(x, z);
    const ang = Math.atan2(z, x);
    const apron = Math.max(0, 1 - r / 3.6) ** 1.8 * 0.85;
    const cone = Math.max(0, 1 - r / 1.85) ** 1.05 * 3.05;
    const crater = r < 0.38 ? (0.38 - r) * 1.85 : 0;
    const lava =
      Math.exp(-(((ang + 1.65) / 0.7) ** 2)) *
      smoothstep(1.2, 3.0, r) *
      0.55;
    return Math.max(0.08, apron + cone - crater + lava);
  };
}

/** 冰川 — U-shaped valley between two ridged walls + cirque head + moraine */
export function generateGlacialTerrain(seed: number): HeightFn {
  return (x, z) => {
    const wallNoise =
      1.6 +
      0.5 * fbm(x * 0.5, z * 0.5, seed, 3) +
      0.5 * ridged(x * 0.4, z * 0.7, seed + 11, 3);
    const wallPos = wallNoise * (1 - (0.12 * Math.abs(x)) / 4);
    const dz = Math.abs(z) - wallPos * 0.82;
    // U cross-section: flat floor, steep walls
    const uShape = smoothstep(0.15, 1.0, dz);
    let h = uShape * wallNoise * 1.35 + (1 - uShape) * 0.4;
    // cirque bowl carved at the head (x negative)
    const cirque = Math.exp(
      -(((x + 3.55) * (x + 3.55)) / 0.85 + (z * z) / 2.6),
    );
    h -= cirque * 1.15;
    // moraine ridge across the mouth (x positive)
    const moraine =
      Math.exp(-(((x - 2.9) * (x - 2.9)) / 0.16)) *
      Math.exp(-((z * z) / 7)) *
      0.85;
    h += moraine;
    // floor micro-texture
    h += 0.04 * fbm(x * 1.6, z * 1.6, seed + 23, 3) * (1 - uShape);
    return Math.max(0.05, h);
  };
}

/** 岩溶 — 几座塔状峰林 + 两个落水洞，像桂林示意 */
export function generateKarstTerrain(_seed: number): HeightFn {
  const towers: [number, number, number, number][] = [
    [-2.1, -0.8, 0.55, 2.35],
    [-0.7, 0.15, 0.62, 2.85],
    [0.85, -1.05, 0.5, 2.15],
    [1.95, 0.35, 0.58, 2.55],
    [0.15, 1.65, 0.48, 1.85],
    [2.45, 1.55, 0.42, 1.55],
    [-1.55, 1.35, 0.4, 1.45],
  ];
  return (x, z) => {
    let h = 0.22;
    for (const [cx, cz, r, hh] of towers) {
      const d = Math.hypot(x - cx, z - cz);
      h += Math.max(0, 1 - d / r) ** 1.35 * hh;
    }
    h -= bump(x, z, 0.35, 0.45, 0.7, 0.7, 0.55);
    h -= bump(x, z, 1.15, 1.05, 0.55, 0.55, 0.4);
    return Math.max(0.1, h);
  };
}

/** 沙漠 — 迎风缓、背风陡的新月形沙丘 */
export function generateDesertTerrain(_seed: number): HeightFn {
  const dunes: [number, number, number][] = [
    [-1.6, -0.2, 1.15],
    [0.35, 0.55, 1.35],
    [2.0, -0.85, 0.95],
    [-0.4, 1.85, 0.82],
  ];
  return (x, z) => {
    let h = 0.16;
    for (const [cx, cz, amp] of dunes) {
      const dx = x - cx;
      const dz = z - cz;
      const along = Math.exp(-(dz * dz) / 1.15);
      const windward = dx < 0 ? Math.exp(-(dx * dx) / 1.8) : Math.exp(-(dx * dx) / 0.38);
      h += along * windward * amp;
    }
    return Math.max(0.08, h);
  };
}

/** 海岸 — irregular shoreline, sea cliff, beach, shallow sea floor */
export function generateCoastalTerrain(seed: number): HeightFn {
  return (x, z) => {
    const shore = 0.55 + 0.35 * fbm(z * 0.42, x * 0.42, seed, 3);
    if (x > shore) {
      const up = x - shore;
      // sandy beach flat just above the water
      const beach = smoothstep(0.05, 0.4, up) * 0.1;
      // sea cliff just above the beach
      const cliff = smoothstep(0.4, 0.85, up) * 1.35;
      // rising hinterland
      const inland = Math.min(up / 3.4, 1) * 0.85;
      const relief = 0.06 * fbm(x * 1.3, z * 1.3, seed + 3, 3);
      return Math.max(0.06, 0.05 + beach + cliff + inland + relief);
    }
    // shallow continental shelf then deepening floor
    const down = (shore - x) / 3.6;
    const floor =
      -0.12 -
      Math.min(down, 1.25) * 1.0 +
      0.07 * fbm(x * 1.1, z * 1.1, seed + 9, 3);
    return floor;
  };
}

/** 构造 — 中间地垒抬升，两侧地堑下陷，断层崖清楚 */
export function generateStructuralTerrain(_seed: number): HeightFn {
  return (x, z) => {
    const [u] = rot(x, z, 0.18);
    const horst = smoothstep(-0.95, -0.72, u) * (1 - smoothstep(0.82, 1.05, u));
    const westLow = smoothstep(-1.95, -1.7, u) * (1 - smoothstep(-0.95, -0.72, u));
    const eastLow = smoothstep(0.82, 1.05, u) * (1 - smoothstep(2.05, 2.3, u));
    return Math.max(0.16, 0.7 + horst * 1.45 - westLow * 0.38 - eastLow * 0.32 + 0.03 * Math.sin(z * 0.55));
  };
}

/** 重力 — steep headwall + concave scar + slid mass tongue + toe lobe */
export function generateMassMovementTerrain(seed: number): HeightFn {
  return (x, z) => {
    // regional slope descending toward +x
    const slope = 2.5 - 1.6 * smoothstep(-3.6, 3.2, x);
    const relief = fbm(x * 0.8, z * 0.8, seed, 3) * 0.12;
    // headwall: a marked step near x = -0.9
    const headwall = smoothstep(-0.15, 0.5, -(x + 0.85)) * 1.25 * 0; // reserved
    void headwall;
    let h = slope + relief;
    // concave scar / detachment depression behind the slide
    const scar = Math.exp(-(((x + 0.55) * (x + 0.55)) / 0.5 + (z * z) / 3.2));
    h -= scar * 0.7;
    // slid mass: bulged tongue flowing from the scarp downslope
    const mass = Math.exp(-(((x - 0.3) * (x - 0.3)) / 0.62 + (z * z) / 1.15));
    h += mass * 0.72;
    // toe lobe at the front
    const toe = Math.exp(-(((x - 1.7) * (x - 1.7)) / 0.5 + (z * z) / 2.0));
    h += toe * 0.5;
    // lateral ridges outlining the mass
    const lateral = Math.exp(
      -(((x - 0.45) * (x - 0.45)) / 0.7 + ((z - 1.15) * (z - 1.15)) / 0.06),
    );
    h += lateral * 0.6;
    const lateral2 = Math.exp(
      -(((x - 0.45) * (x - 0.45)) / 0.7 + ((z + 1.15) * (z + 1.15)) / 0.06),
    );
    h += lateral2 * 0.6;
    return Math.max(0.05, h);
  };
}

const GENERATORS: Record<string, (seed: number) => HeightFn> = {
  mountains: generateMountainTerrain,
  hills: generateHillTerrain,
  plateau: generatePlateauTerrain,
  basin: generateBasinTerrain,
  plain: generatePlainTerrain,
  river: generateRiverTerrain,
  "island-arc": generateIslandArcTerrain,
  volcanic: generateVolcanicTerrain,
  glacial: generateGlacialTerrain,
  karst: generateKarstTerrain,
  desert: generateDesertTerrain,
  coastal: generateCoastalTerrain,
  structural: generateStructuralTerrain,
  "mass-movement": generateMassMovementTerrain,
};

/** seed-aware, memoizable look-up used by the viewer */
export function getGenerator(type: string): (seed: number) => HeightFn {
  return GENERATORS[type] ?? generateMountainTerrain;
}

// silence unused-import lints for helpers referenced across branches
export const __abi = {
  buildPathSamples,
  riverPath2D,
  nearestPath,
  channelIncise,
};
