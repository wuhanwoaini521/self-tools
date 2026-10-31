import { describe, expect, it } from "vitest";
import { travelPlanRoute } from "./HomePage";
import type { TravelPlan } from "../../types";

describe("Personal Hub travel resume route", () => {
  it("retains the saved plan's encoded city, duration, and date range", () => {
    const plan: TravelPlan = {
      context: {
        city: "洛阳 & 西安",
        days: 2,
        date_range: { start: "2026-11-01", end: "2026-11-02" },
      },
      title: "周末行程",
      departure: "",
      notes: "",
      custom_places: {},
      selected_keys: [],
      assigned_days: {},
      must_visit_keys: [],
      revision: 1,
      updated_at: 1_793_111_008,
    };

    expect(travelPlanRoute(plan)).toBe(
      "#travel?city=%E6%B4%9B%E9%98%B3+%26+%E8%A5%BF%E5%AE%89&days=2&date_start=2026-11-01&date_end=2026-11-02",
    );
  });

  it("omits date params for plans without a saved range", () => {
    const plan: TravelPlan = {
      context: { city: "杭州", days: 3, date_range: null },
      title: "杭州慢游",
      departure: "",
      notes: "",
      custom_places: {},
      selected_keys: [],
      assigned_days: {},
      must_visit_keys: [],
      revision: 1,
      updated_at: 1_793_111_008,
    };

    expect(travelPlanRoute(plan)).toBe("#travel?city=%E6%9D%AD%E5%B7%9E&days=3");
  });
});
