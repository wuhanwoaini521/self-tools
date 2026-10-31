import { describe, expect, it } from "vitest";
import type { SemanticPeriod } from "./semanticTypes";
import {
  buildEraTimelineLayout,
  eraDensityHeight,
  isApproximatePeriod,
  periodRangeLabel,
} from "./EraTimeline";

function period(
  id: string,
  name: string,
  start: number | null,
  end: number | null,
  precision: string | null = "range",
): SemanticPeriod {
  return {
    id,
    name_zh_cn: name,
    start_year: start,
    end_year: end,
    date_precision: precision,
  };
}

const HOME_PERIODS: SemanticPeriod[] = [
  period("xia", "夏", -2070, -1600, "approximate"),
  period("shang", "商", -1600, -1046, "approximate"),
  period("qin", "秦", -221, -207),
  period("western-han", "西汉", -206, 8),
  period("umbrella", "宋辽金时期", 907, 1279),
  period("liao", "辽", 916, 1125),
  period("northern-song", "北宋", 960, 1127),
  period("jin", "金", 1115, 1234),
  period("southern-song", "南宋", 1127, 1279),
  period("prehistory", "上古", null, -2070, "approximate"),
];

describe("buildEraTimelineLayout", () => {
  it("按真实年份比例放置公元前时期，并生成公元前刻度", () => {
    const layout = buildEraTimelineLayout(
      [period("xia", "夏", -2070, -1600), period("shang", "商", -1600, -1046)],
      1043,
      false,
    );
    const xia = layout.segments.find((s) => s.period.id === "xia");
    const shang = layout.segments.find((s) => s.period.id === "shang");
    expect(xia && shang).toBeTruthy();
    // 夏在前，商在后；宽度按跨度比例（商 554 年 > 夏 470 年）。
    expect(xia!.left).toBeLessThan(shang!.left);
    expect(shang!.width).toBeGreaterThan(xia!.width);
    expect(layout.ticks.some((tick) => tick.label === "前2000")).toBe(true);
    expect(layout.ticks.some((tick) => tick.label === "前1500")).toBe(true);
    // 全是公元前时没有公元分界。
    expect(layout.epochLeft).toBeNull();
  });

  it("跨公元时给出公元分界与「公元」刻度", () => {
    const layout = buildEraTimelineLayout(HOME_PERIODS, 1043, false);
    expect(layout.epochLeft).not.toBeNull();
    expect(layout.epochLeft!).toBeGreaterThan(0);
    expect(layout.ticks.find((tick) => tick.year === 0)?.label).toBe("公元");
  });

  it("极短时期补到最小可点宽度，且不顶出轨道", () => {
    const layout = buildEraTimelineLayout(HOME_PERIODS, 1043, false);
    const qin = layout.segments.find((s) => s.period.id === "qin");
    expect(qin!.width).toBeGreaterThanOrEqual(32);
    for (const segment of layout.segments) {
      expect(segment.left + segment.width).toBeLessThanOrEqual(
        layout.trackWidth + 0.01,
      );
    }
    const compact = buildEraTimelineLayout(HOME_PERIODS, 1040, true);
    for (const segment of compact.segments) {
      expect(segment.width).toBeGreaterThanOrEqual(46);
      expect(segment.left + segment.width).toBeLessThanOrEqual(
        compact.trackWidth + 0.01,
      );
    }
  });

  it("时间重叠的时期进入不同轨道，保证都可点击", () => {
    const layout = buildEraTimelineLayout(HOME_PERIODS, 1043, false);
    const byId = new Map(layout.segments.map((s) => [s.period.id, s]));
    const overlapping = ["umbrella", "liao", "northern-song", "jin"] as const;
    const lanes = overlapping.map((id) => byId.get(id)!.lane);
    expect(new Set(lanes).size).toBe(overlapping.length);
    expect(layout.laneCount).toBeGreaterThanOrEqual(4);
  });

  it("标记约略年代，并把缺失起点如实保留为开放段", () => {
    const layout = buildEraTimelineLayout(HOME_PERIODS, 1043, false);
    const xia = layout.segments.find((s) => s.period.id === "xia")!;
    const prehistory = layout.segments.find(
      (s) => s.period.id === "prehistory",
    )!;
    expect(xia.approximate).toBe(true);
    expect(isApproximatePeriod(xia.period)).toBe(true);
    expect(prehistory.openStart).toBe(true);
    expect(prehistory.left).toBeGreaterThanOrEqual(0);
    expect(periodRangeLabel(prehistory.period)).toBe("年代待考 — 前2070");
  });

  it("两端都缺年份时不硬造比例段", () => {
    const layout = buildEraTimelineLayout(
      [period("unknown", "待考", null, null)],
      1043,
      false,
    );
    expect(layout.segments).toHaveLength(0);
    expect(layout.domain).toBeNull();
  });
});

describe("eraDensityHeight", () => {
  it("无计数或最大值为 0 时不画柱子", () => {
    expect(eraDensityHeight(0, 10, 24)).toBe(0);
    expect(eraDensityHeight(null, 10, 24)).toBe(0);
    expect(eraDensityHeight(5, 0, 24)).toBe(0);
  });

  it("按事件数归一化，并保留最小可见高度", () => {
    expect(eraDensityHeight(50, 100, 24)).toBe(12);
    expect(eraDensityHeight(1, 1000, 24)).toBe(3);
    expect(eraDensityHeight(100, 100, 24)).toBe(24);
  });
});
