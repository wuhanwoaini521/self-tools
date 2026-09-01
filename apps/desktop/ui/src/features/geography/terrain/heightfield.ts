/**
 * Heightfield / procedural-noise core.
 *
 * A terrain is described by a pure `HeightFn(x, z) -> height`, then sampled into a
 * grid `Heightfield`. All landform generators live in `generators.ts` and share these
 * primitives (deterministic value noise + fbm + ridged fbm). No `Math.random()` here:
 * every shape comes from a structured height function, so each landform reads as a
 * continuous, eroded terrain rather than a pile of random bumps.
 */

export type HeightFn = (x: number, z: number) => number;

export interface Heightfield {
  /** grid cells per side (mesh vertices per side = resolution + 1) */
  resolution: number;
  /** half-extent of the world, i.e. world spans [-extent, +extent] on X and Z */
  extent: number;
  /** vertices per side = resolution + 1 */
  count: number;
  /** row-major heights: data[iz * count + ix] */
  data: Float32Array;
  min: number;
  max: number;
}

/** deterministic integer hash -> [0, 1) */
export function hash2(ix: number, iz: number, seed: number): number {
  let h =
    Math.imul(ix, 374761393) +
    Math.imul(iz, 668265263) +
    Math.imul(seed, 2246822519);
  h = Math.imul(h ^ (h >>> 13), 1274126177);
  return ((h ^ (h >>> 16)) >>> 0) / 4294967296;
}

/** smooth value noise at continuous (x, z) */
export function valueNoise(x: number, z: number, seed: number): number {
  const ix = Math.floor(x);
  const iz = Math.floor(z);
  const tx = x - ix;
  const tz = z - iz;
  const ux = tx * tx * (3 - 2 * tx);
  const uz = tz * tz * (3 - 2 * tz);
  const a = hash2(ix, iz, seed);
  const b = hash2(ix + 1, iz, seed);
  const c = hash2(ix, iz + 1, seed);
  const d = hash2(ix + 1, iz + 1, seed);
  return a + (b - a) * ux + (c - a) * uz + (a - b - c + d) * ux * uz;
}

/** fractional Brownian motion, 0..1-ish, smooth broad variation */
export function fbm(
  x: number,
  z: number,
  seed: number,
  octaves = 4,
  lacunarity = 2,
  gain = 0.5,
): number {
  let sum = 0;
  let amp = 0.5;
  let freq = 1;
  let norm = 0;
  for (let i = 0; i < octaves; i += 1) {
    sum += amp * valueNoise(x * freq, z * freq, seed + i * 1013);
    norm += amp;
    amp *= gain;
    freq *= lacunarity;
  }
  return sum / norm;
}

/**
 * ridged fbm. `1 - |2n - 1|` turns each noise octave into a sharp ridge crest.
 * With anisotropic frequency (freqA << freqB) it produces elongated mountain chains.
 * Output is roughly 0..1 with crests near 1.
 */
export function ridged(
  x: number,
  z: number,
  seed: number,
  octaves = 5,
  lacunarity = 2.1,
  gain = 0.55,
): number {
  let sum = 0;
  let amp = 0.5;
  let freq = 1;
  let norm = 0;
  for (let i = 0; i < octaves; i += 1) {
    const n = valueNoise(x * freq, z * freq, seed + i * 1013);
    const r = 1 - Math.abs(2 * n - 1);
    sum += amp * r * r;
    norm += amp;
    amp *= gain;
    freq *= lacunarity;
  }
  return sum / norm;
}

export function smoothstep(e0: number, e1: number, x: number): number {
  const t = Math.min(Math.max((x - e0) / (e1 - e0), 0), 1);
  return t * t * (3 - 2 * t);
}

export function rot(x: number, z: number, ang: number): [number, number] {
  const c = Math.cos(ang);
  const s = Math.sin(ang);
  return [x * c - z * s, x * s + z * c];
}

export function makeHeightfield(
  fn: HeightFn,
  resolution: number,
  extent: number,
): Heightfield {
  const count = resolution + 1;
  const cell = (2 * extent) / resolution;
  const data = new Float32Array(count * count);
  let min = Infinity;
  let max = -Infinity;
  for (let iz = 0; iz < count; iz += 1) {
    const z = -extent + iz * cell;
    for (let ix = 0; ix < count; ix += 1) {
      const x = -extent + ix * cell;
      const h = fn(x, z);
      const p = iz * count + ix;
      data[p] = h;
      if (h < min) min = h;
      if (h > max) max = h;
    }
  }
  return { resolution, extent, count, data, min, max };
}

/** bilinear sample of the heightfield at continuous (x, z) */
export function heightAt(hf: Heightfield, x: number, z: number): number {
  const { resolution, extent, count, data } = hf;
  const fx = ((x + extent) / (2 * extent)) * resolution;
  const fz = ((z + extent) / (2 * extent)) * resolution;
  const ix = Math.floor(fx);
  const iz = Math.floor(fz);
  const tx = Math.min(Math.max(fx - ix, 0), 1);
  const tz = Math.min(Math.max(fz - iz, 0), 1);
  const ix0 = Math.min(Math.max(ix, 0), resolution);
  const iz0 = Math.min(Math.max(iz, 0), resolution);
  const ix1 = Math.min(ix0 + 1, resolution);
  const iz1 = Math.min(iz0 + 1, resolution);
  const a = data[iz0 * count + ix0];
  const b = data[iz0 * count + ix1];
  const c = data[iz1 * count + ix0];
  const d = data[iz1 * count + ix1];
  return a + (b - a) * tx + (c - a) * tz + (a - b - c + d) * tx * tz;
}

/** sample a straight line A->B (in world XZ) into raw heights */
export function sampleLine(
  hf: Heightfield,
  ax: number,
  az: number,
  bx: number,
  bz: number,
  steps: number,
): number[] {
  const out: number[] = [];
  for (let i = 0; i <= steps; i += 1) {
    const t = i / steps;
    out.push(heightAt(hf, ax + (bx - ax) * t, az + (bz - az) * t));
  }
  return out;
}
