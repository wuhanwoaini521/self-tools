/**
 * 教学用地形配色：按高程分层，让孩子一眼看出低地、坡面、山顶和水面。
 */

export interface RampStop {
  t: number;
  color: [number, number, number];
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

/** 陆地高程色带：河谷绿 → 山坡褐 → 岩灰 → 雪顶 */
export const LAND_RAMP: RampStop[] = [
  { t: 0.0, color: [0.42, 0.62, 0.34] },
  { t: 0.22, color: [0.62, 0.72, 0.38] },
  { t: 0.45, color: [0.78, 0.62, 0.38] },
  { t: 0.68, color: [0.62, 0.48, 0.36] },
  { t: 0.86, color: [0.55, 0.52, 0.5] },
  { t: 1.0, color: [0.94, 0.96, 0.98] },
];

/** 海床：深蓝到近岸浅青 */
export const SEA_RAMP: RampStop[] = [
  { t: 0.0, color: [0.08, 0.22, 0.42] },
  { t: 0.55, color: [0.12, 0.42, 0.62] },
  { t: 1.0, color: [0.32, 0.7, 0.78] },
];

export type PaletteTint = [number, number, number];

export const DEFAULT_TINT: PaletteTint = [1, 1, 1];
