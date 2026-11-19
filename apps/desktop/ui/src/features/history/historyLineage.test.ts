import { describe, expect, it } from "vitest";
import { groupPeriodsByEra } from "./HistoryLineage";
import { PERIOD_ERA_GROUPS } from "./periodDetailHelpers";

describe("groupPeriodsByEra", () => {
  it("按大时代切段，未知时期落到其他", () => {
    const periods = [
      { id: "period-qin", name_zh_cn: "秦", start_year: -221, end_year: -207 },
      { id: "period-tang", name_zh_cn: "唐", start_year: 618, end_year: 907 },
      { id: "period-extra", name_zh_cn: "旁支", start_year: 1, end_year: 2 },
    ];
    const grouped = groupPeriodsByEra(periods);
    expect(grouped.map((group) => group.label)).toEqual([
      "秦汉",
      "隋唐五代",
      "其他",
    ]);
    expect(grouped[0].periods.map((period) => period.id)).toEqual(["period-qin"]);
    expect(PERIOD_ERA_GROUPS.some((group) => group.label === "其他")).toBe(false);
  });
});
