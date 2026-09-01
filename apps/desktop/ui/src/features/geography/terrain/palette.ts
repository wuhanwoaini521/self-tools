/**
 * Muted scientific terrain palette — warm geological greys/taupes that sit naturally
 * on the app's off-white + warm-orange page. No saturated greens/blues/reds.
 */

export interface RampStop {
  t: number;
  color: [number, number, number]; // rgb 0..1
}

export function rampColor(
  stops: RampStop[],
  t: number,
): [number, number, number] {
  const x = Math.min(Math.max(t, 0), 1);
  for (let i = 0; i < stops.length - 1; i += 1) {
    const a = stops[i];
    const b = stops[i + 1];
    if (x <= b.t) {
      const u = b.t === a.t ? 0 : (x - a.t) / (b.t - a.t);
      return [
        a.color[0] + (b.color[0] - a.color[0]) * u,
        a.color[1] + (b.color[1] - a.color[1]) * u,
        a.color[2] + (b.color[2] - a.color[2]) * u,
      ];
    }
  }
  return stops[stops.length - 1].color;
}

/** dry rock/soil ramp (low -> high) */
export const LAND_RAMP: RampStop[] = [
  { t: 0.0, color: [0.86, 0.8, 0.69] }, // warm sand / beige
  { t: 0.3, color: [0.76, 0.69, 0.56] }, // tawny earth
  { t: 0.55, color: [0.66, 0.59, 0.47] }, // olive-grey soil
  { t: 0.78, color: [0.56, 0.5, 0.41] }, // grey-brown
  { t: 1.0, color: [0.48, 0.43, 0.36] }, // rocky grey
];

/** submerged ocean floor ramp (below water level, darker/cooler) */
export const SEA_RAMP: RampStop[] = [
  { t: 0.0, color: [0.36, 0.42, 0.46] }, // deep
  { t: 0.6, color: [0.5, 0.55, 0.58] }, // mid shelf
  { t: 1.0, color: [0.62, 0.66, 0.67] }, // near shore
];

/** a per-type tint multiplier applied on top of the shared ramps (subtle) */
export type PaletteTint = [number, number, number];

export const DEFAULT_TINT: PaletteTint = [1, 1, 1];
