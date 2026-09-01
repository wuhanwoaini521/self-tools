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

/** 山地与山脉 — continuous ridge systems + valley incision + multi-scale erosion */
export function generateMountainTerrain(seed: number): HeightFn {
  return (x, z) => {
    // primary chain system (elongated along u)
    const [u, v] = rot(x, z, 0.58);
    const c1 = ridged(u * 0.4, v * 2.2, seed, 5) ** 1.85;
    // secondary oblique ridge system (spurs / 支脉)
    const [u2, v2] = rot(x, z, -0.3);
    const c2 = ridged(u2 * 0.85, v2 * 1.4, seed + 53, 4) ** 2.1 * 0.5;
    let h = 0.6 * c1 + 0.4 * c2;
    // broad mass variation (clusters of high ground)
    h *= 0.72 + 0.56 * fbm(x * 0.26, z * 0.26, seed + 11, 3);
    // multi-scale erosion texture
    h *= 0.66 + 0.5 * fbm(x * 2.4, z * 2.4, seed + 71, 4);
    // valley incision between ridges
    const valley = fbm(x * 0.3, z * 0.3, seed + 97, 3);
    const carve = Math.max(0, (valley - 0.52) / 0.48) ** 1.4;
    h = Math.max(0, h * (1 - 0.4 * carve));
    return h ** 1.16 * 3.6;
  };
}

/** 高原 — high flat surface with ragged steep edges + river-cut gorges */
export function generatePlateauTerrain(seed: number): HeightFn {
  // a few meandering gorge lines incising the top
  const gorgeDepth = (x: number, z: number): number => {
    let cut = 0;
    for (let i = 0; i < 2; i += 1) {
      const path = buildPathSamples(riverPath2D(), 4);
      const { dist } = nearestPath(x, z, path);
      const w = 0.55 + i * 0.25;
      cut += Math.exp(-(dist * dist) / (2 * w * w)) * (1.0 - i * 0.15);
    }
    return cut;
  };
  return (x, z) => {
    // ragged plateau edge from 5-octave fbm -> near-cliff falloff
    const e = fbm(x * 0.5, z * 0.5, seed + 5, 5);
    const mask = smoothstep(0.475, 0.555, e);
    // top micro-relief
    const relief = 0.08 * (fbm(x * 1.6, z * 1.6, seed + 23, 3) - 0.5);
    // subtle edge gullying
    const rim = 0.1 * (1 - mask) * (1 + fbm(x * 1.1, z * 1.1, seed + 39, 3));
    let h = 0.28 + mask * (2.05 + relief) - rim;
    // gorges cut deep through the plateau surface
    h -= mask * gorgeDepth(x, z) * 0.62;
    return Math.max(0.18, h);
  };
}

/** 盆地 — irregular peripheral ridge ring + low centre + outlet gap (no round bowl) */
export function generateBasinTerrain(seed: number): HeightFn {
  const outletAngle = -2.1; // south-west gap where a river exits
  return (x, z) => {
    const r = Math.hypot(x, z);
    const ang = Math.atan2(z, x);
    // asymmetric, angular ring radius
    const ringR =
      2.62 +
      0.5 * Math.sin(ang * 2 + 0.8) +
      0.34 * Math.sin(ang * 5 - 1.1) +
      0.55 * (fbm(x * 0.3, z * 0.3, seed + 9, 3) - 0.5) * 2;
    const ringDist = Math.abs(r - ringR);
    const ring = Math.max(0, 1 - ringDist / 1.15) ** 2.2;
    // outlet gap breaks the ring
    const outlet = Math.exp(-(((ang - outletAngle) / 0.36) ** 2));
    const ringBroken = ring * (1 - 0.92 * outlet);
    // ridge detail along the ring
    const rid = 0.85 + 0.95 * ridged(x * 0.62, z * 0.62, seed + 17, 4);
    const axisBoost = 0.75 + 0.55 * fbm(x * 0.2, z * 0.2, seed + 7, 3);
    const ridgeHeight = ringBroken * rid * 1.62 * axisBoost;
    // gentle, low centre with a tilt toward the outlet
    const floor = 0.28 + 0.2 * fbm(x * 0.85, z * 0.85, seed + 29, 3);
    const tilt =
      0.1 *
      Math.max(0, x * 0.55 + z * 0.25) *
      smoothstep(0.9, 2.2, ringR - r) *
      0;
    const edgeMask = smoothstep(1.45, ringR * 0.94, r);
    // outer flank extends past the ring, descending to lowland
    const outer = 1 + 0.35 * smoothstep(ringR * 0.98, ringR * 1.3, r);
    let h = floor + ridgeHeight * edgeMask * outer + tilt;
    // soften the outer edge so it reads as mountain foot, not a wall
    h *= 1 - 0.22 * smoothstep(r, ringR + 1.6, ringR + 2.1);
    return Math.max(0.15, h);
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

/** 火山锥 — broad shield base + tall cone + crater + lava apron (continuous terrain) */
export function generateVolcanicTerrain(seed: number): HeightFn {
  return (x, z) => {
    const r = Math.hypot(x, z);
    const ang = Math.atan2(z, x);
    // broad low shield apron
    const apron = Math.max(0, 1 - r / 3.7) ** 1.7 * 1.0;
    // main cone
    const cone = r < 1.55 ? Math.max(0, 1 - r / 1.55) ** 1.32 * 2.9 : 0;
    // crater depression near the summit
    const crater = Math.exp(-((r - 0.24) * (r - 0.24)) / 0.05) * 0.5;
    // smooth lava apron lobes on one flank (main lower sector)
    const flowSector = Math.exp(-(((ang + 1.7) / 0.75) ** 2));
    const flowBand = smoothstep(1.7, 3.1, r);
    const lava = flowSector * flowBand * Math.exp(-(r * r) / 12) * 0.5;
    // erosion micro-texture (stronger near base, muted on the clean cone)
    const detail = fbm(x * 1.5, z * 1.5, seed + 31, 3);
    const texture = 0.05 + 0.16 * detail * smoothstep(0.5, 2.6, r);
    return Math.max(0.05, apron + cone + texture * r * 0.5 - crater + lava);
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

/** 岩溶 — sharp tower karst plus dissolution sinkholes */
export function generateKarstTerrain(seed: number): HeightFn {
  const sink = (x: number, z: number): number => {
    const d =
      (Math.max(0, fbm(x * 2.0, z * 2.0, seed + 31, 3) - 0.6) / 0.4) ** 1.3;
    return d * 0.75;
  };
  return (x, z) => {
    // tower field: strongly peaked residual peaks
    const towers = ridged(x * 1.15, z * 1.15, seed, 4) ** 3.4 * 2.45;
    // broad base plateau of the karst plain
    const base = 0.22 + 0.12 * fbm(x * 0.6, z * 0.6, seed + 7, 3);
    return Math.max(0.08, base + towers - sink(x, z));
  };
}

/** 沙漠 — transverse / barchan dune field with asymmetric slopes + flat playa */
export function generateDesertTerrain(seed: number): HeightFn {
  return (x, z) => {
    // undulating transverse crest lines (wind along +z)
    const phase = fbm(x * 0.5, z * 0.55, seed, 4) * 2.4 + z * 0.85;
    const crest = (0.55 + 0.45 * Math.sin(phase)) ** 2.5;
    // crest height varies along the line
    const crestH = 0.55 + 0.8 * fbm(x * 0.35, z * 0.35, seed + 13, 3);
    let h = crest * crestH + 0.14;
    // gentle overall dome
    h += 0.1 * (1 - Math.min(1, Math.hypot(x / 4, z / 4) / 1.4));
    // flatten one region into playa
    const playa = smoothstep(2.6, 1.6, Math.hypot(x + 3, z - 3)) * 0.25;
    h -= playa;
    h += 0.03 * fbm(x * 1.8, z * 1.8, seed + 29, 3);
    return Math.max(0.05, h);
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

/** 构造 — parallel faults: central horst, flanking grabens, prominent scarps */
export function generateStructuralTerrain(seed: number): HeightFn {
  return (x, z) => {
    const [u, v] = rot(x, z, 0.34);
    const nz = 0.25 + 0.3 * fbm(v * 0.4, u * 0.4, seed, 3);
    // three tilted blocks between two fault lines at u = +/- 1.2
    const blockU = (u + 1.25) / 2.5; // 0..1 across
    // block profile with steep scarps at the fault lines
    const grabenLow = 0.32;
    const horstHigh = 2.15;
    // ramp 0 ->1 across the graben, 1 across the horst, settling into the far block
    const inGraben = smoothstep(0.0, 0.32, blockU);
    const inHorst = smoothstep(0.38, 0.55, blockU);
    const outHorst = smoothstep(0.86, 1.0, blockU);
    const raw =
      grabenLow * (1 - inGraben) +
      horstHigh * inGraben * (1 - inHorst) +
      grabenLow * inHorst +
      (grabenLow + 0.5) * outHorst;
    // fault scarps: sharpen the two vertical jumps
    let h = raw;
    h = h * (0.9 + 0.35 * fbm(u * 2.2, v * 0.9, seed + 17, 3));
    h += nz * 0;
    return Math.max(0.1, h * 0.85 + 0.12);
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
