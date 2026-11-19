import { describe, expect, it } from "vitest";
import { PERIOD_ERA_GROUPS } from "./periodDetailHelpers";
import { buildDynastyStrip } from "./dynastyStripLayout";

/** 与主干期列表保持一致的 31 个 period id（新增时期而不归组时本测试会失败）。 */
const PERIOD_IDS = [
  "period-antiquity",
  "period-xia",
  "period-shang",
  "period-western-zhou",
  "period-spring-autumn",
  "period-warring-states",
  "period-qin",
  "period-western-han",
  "period-xin",
  "period-eastern-han",
  "period-late-eastern-han",
  "period-three-kingdoms",
  "period-western-jin",
  "period-sixteen-kingdoms",
  "period-eastern-jin",
  "period-northern-southern",
  "period-sui",
  "period-tang",
  "period-five-dynasties-ten-kingdoms",
  "period-song-liao-jin",
  "period-liao",
  "period-northern-song",
  "period-western-xia",
  "period-jin",
  "period-southern-song",
  "period-yuan",
  "period-ming",
  "period-qing",
  "period-late-qing",
  "period-modern",
  "period-republic",
];

describe("PERIOD_ERA_GROUPS（朝代总览分组）", () => {
  const groupedIds = PERIOD_ERA_GROUPS.flatMap((group) => group.ids);

  it("每个时期都恰好归入一个分组", () => {
    for (const id of PERIOD_IDS) {
      expect(groupedIds.filter((grouped) => grouped === id)).toHaveLength(1);
    }
  });

  it("分组里不含未知 id", () => {
    for (const id of groupedIds) {
      expect(PERIOD_IDS).toContain(id);
    }
  });

  it("分组顺序按年代先后，且标签非空", () => {
    expect(PERIOD_ERA_GROUPS.map((group) => group.label)).toEqual([
      "上古 · 夏商周",
      "春秋战国",
      "秦汉",
      "魏晋南北朝",
      "隋唐五代",
      "宋辽金夏",
      "元明清",
      "近现代",
    ]);
  });
});

describe("buildDynastyStrip（朝代长卷几何）", () => {
  const periods = [
    { id: "a", name_zh_cn: "夏", start_year: -2070, end_year: -1600 },
    { id: "b", name_zh_cn: "商", start_year: -1600, end_year: -1046 },
    { id: "c", name_zh_cn: "西周", start_year: -1046, end_year: -771 },
    { id: "d", name_zh_cn: "西晋", start_year: 265, end_year: 316 },
    { id: "e", name_zh_cn: "三国", start_year: 220, end_year: 280 },
    { id: "f", name_zh_cn: "上古", start_year: null, end_year: -2070 },
  ];
  const layout = buildDynastyStrip(periods, { width: 1000, eraGroups: [] });

  it("首尾相接的相继王朝落在同一轨（形成连续年表带）", () => {
    const laneOf = (id: string) => layout.bars.find((bar) => bar.period.id === id)!.lane;
    expect(laneOf("a")).toBe(laneOf("b"));
    expect(laneOf("b")).toBe(laneOf("c"));
  });

  it("时间重叠的并立政权分到不同轨", () => {
    const laneOf = (id: string) => layout.bars.find((bar) => bar.period.id === id)!.lane;
    expect(laneOf("d")).not.toBe(laneOf("e"));
  });

  it("无起始年代的时期按轴起点处理且标记 openStart", () => {
    const ancient = layout.bars.find((bar) => bar.period.id === "f")!;
    expect(ancient.openStart).toBe(true);
    expect(ancient.left).toBe(0);
  });

  it("窄条的名称放到条外，宽条放条内", () => {
    const wide = layout.bars.find((bar) => bar.period.id === "a")!;
    const narrow = layout.bars.find((bar) => bar.period.id === "f")!;
    expect(wide.placement).toBe("inside");
    expect(narrow.placement).not.toBe("inside");
  });
});
