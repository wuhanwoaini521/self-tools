/**
 * 朝代长卷（DynastyStrip）的几何计算：真时间轴 + 分轨 + 标签避让。
 *
 * 与 EraTimeline 的区别：不横向滚动，整段 4000 年按容器宽度压进一屏，
 * 目的是「一眼看完所有朝代」的时空总览。窄条（秦、新、五代十国）的名称
 * 放到条外并做上下避让，避免互相压字。
 * 纯函数，供单测覆盖。
 */

export interface StripPeriodInput {
  id: string;
  name_zh_cn: string;
  start_year: number | null;
  end_year: number | null;
  event_count?: number | null;
}

export type StripLabelPlacement = "inside" | "above" | "below" | "above2";

export interface StripBar {
  period: StripPeriodInput;
  /** 百分比 0–100（相对容器宽度）。 */
  left: number;
  width: number;
  lane: number;
  placement: StripLabelPlacement;
  /** 无起始年代（上古）：左端用虚线收口。 */
  openStart: boolean;
}

export interface StripTick {
  year: number;
  left: number;
  label: string;
}

export interface StripEra {
  label: string;
  left: number;
  width: number;
  /** 带宽不足以放标签时不画字，只留色带。 */
  showLabel: boolean;
}

export interface StripLayout {
  bars: StripBar[];
  ticks: StripTick[];
  eras: StripEra[];
  laneCount: number;
  minYear: number;
  maxYear: number;
}

export interface StripOptions {
  /** 容器宽度（px），用于判断标签放哪。 */
  width: number;
  eraGroups: { label: string; ids: string[] }[];
}

const EARLIEST = -2100;
const LATEST = 2010;
/** 条宽达到这个像素才把名称放条内。 */
const INSIDE_LABEL_MIN_PX = 44;
const LABEL_CHAR_PX = 10;
const LABEL_PAD_PX = 4;

function clampYear(value: number | null | undefined, fallback: number): number {
  if (value === null || value === undefined || Number.isNaN(value)) return fallback;
  return value;
}

export function yearTickLabel(year: number): string {
  if (year < 0) return `前${Math.abs(year)}`;
  if (year === 0) return "公元";
  return `${year}`;
}

export function buildDynastyStrip(
  periods: StripPeriodInput[],
  options: StripOptions,
): StripLayout {
  const span = LATEST - EARLIEST;
  const toPct = (year: number) => ((year - EARLIEST) / span) * 100;
  const width = Math.max(320, options.width);

  const rows = periods.map((period) => {
    const start = clampYear(period.start_year, EARLIEST);
    const end = clampYear(period.end_year, LATEST);
    return {
      period,
      start: Math.max(EARLIEST, Math.min(start, end)),
      end: Math.min(LATEST, Math.max(start, end)),
      openStart: period.start_year === null,
    };
  });

  rows.sort((a, b) => a.start - b.start || b.end - a.end);

  // 贪心分轨：同轨不得重叠。
  const laneEnds: number[] = [];
  const laneRow = rows.map((row) => {
    const left = toPct(row.start);
    const right = toPct(row.end);
    let lane = laneEnds.findIndex((end) => left >= end - 1e-9);
    if (lane === -1) {
      lane = laneEnds.length;
      laneEnds.push(0);
    }
    laneEnds[lane] = right;
    return { row, lane, left, right };
  });

  // 标签避让：窄条的条外标签优先放上方，撞车则放下方。
  const aboveBusy = new Map<number, [number, number][]>();
  const belowBusy = new Map<number, [number, number][]>();
  const above2Busy = new Map<number, [number, number][]>();

  const bars: StripBar[] = laneRow.map(({ row, lane, left, right }) => {
    const barWidth = Math.max(0.25, right - left);
    const widthPx = (barWidth / 100) * width;
    let placement: StripLabelPlacement = "inside";
    if (widthPx < INSIDE_LABEL_MIN_PX) {
      const labelPx = row.period.name_zh_cn.length * LABEL_CHAR_PX + LABEL_PAD_PX;
      const x0 = (left / 100) * width;
      const x1 = x0 + labelPx;
      const isFree = (busy: [number, number][]) =>
        busy.every(([a, b]) => x1 < a || x0 > b);
      const above = aboveBusy.get(lane) ?? [];
      const below = belowBusy.get(lane) ?? [];
      const above2 = above2Busy.get(lane) ?? [];
      if (isFree(above)) {
        above.push([x0, x1]);
        aboveBusy.set(lane, above);
        placement = "above";
      } else if (isFree(below)) {
        below.push([x0, x1]);
        belowBusy.set(lane, below);
        placement = "below";
      } else if (isFree(above2)) {
        above2.push([x0, x1]);
        above2Busy.set(lane, above2);
        placement = "above2";
      } else {
        placement = "above";
      }
    }
    return {
      period: row.period,
      left,
      width: barWidth,
      lane,
      placement,
      openStart: row.openStart,
    };
  });

  const tickYears = [-2000, -1500, -1000, -500, 1, 500, 1000, 1500, 2000];
  const ticks: StripTick[] = tickYears.map((year) => ({
    year,
    left: toPct(year),
    label: yearTickLabel(year),
  }));

  const eras: StripEra[] = options.eraGroups
    .map((group) => {
      const members = rows.filter((row) => group.ids.includes(row.period.id));
      if (!members.length) return null;
      const start = Math.min(...members.map((row) => row.start));
      const end = Math.max(...members.map((row) => row.end));
      const eraWidth = toPct(end) - toPct(start);
      return {
        label: group.label,
        left: toPct(start),
        width: eraWidth,
        showLabel: (eraWidth / 100) * width >= group.label.length * 11 + 20,
      };
    })
    .filter((era): era is StripEra => Boolean(era));

  return {
    bars,
    ticks,
    eras,
    laneCount: Math.max(1, laneEnds.length),
    minYear: EARLIEST,
    maxYear: LATEST,
  };
}
