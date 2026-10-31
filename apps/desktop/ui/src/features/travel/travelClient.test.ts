import { describe, expect, it, vi } from "vitest";
import type { CommandTransport } from "../../transport";
import { createTravelClient } from "./travelClient";

function fakeTransport(invoke: CommandTransport["invoke"]): CommandTransport {
  return {
    invoke,
    isTauriRuntime: () => true,
    subscribe: () => () => {},
  };
}

describe("travelClient persisted itinerary contract", () => {
  it("lists backend-owned saved plans without depending on guide generation", async () => {
    const plans = [{ context: { city: "杭州", days: 2, date_range: null }, revision: 3 }];
    const invoke = vi.fn().mockResolvedValue(plans);
    const client = createTravelClient(fakeTransport(invoke));

    await expect(client.recentPlans(8)).resolves.toEqual(plans);
    expect(invoke).toHaveBeenCalledWith("travel_plan_recent", { limit: 8 });
  });

  it("loads a plan by the semantic trip context", async () => {
    const invoke = vi.fn().mockResolvedValue(null);
    const client = createTravelClient(fakeTransport(invoke));
    const context = {
      city: "杭州",
      days: 3,
      date_range: { start: "2026-10-01", end: "2026-10-03" },
    };

    await expect(client.getPlan(context)).resolves.toBeNull();
    expect(invoke).toHaveBeenCalledWith("travel_plan_get", { context });
  });

  it("writes the semantic plan through the backend command", async () => {
    const plan = {
      context: { city: "杭州", days: 2, date_range: null },
      title: "西湖周末行",
      departure: "杭州东站",
      notes: "预留午休",
      custom_places: {},
      selected_keys: ["poi-west-lake"],
      assigned_days: { "poi-west-lake": 2 },
      must_visit_keys: ["poi-west-lake"],
      revision: 0,
      updated_at: 0,
    };
    const invoke = vi.fn().mockResolvedValue({ ...plan, updated_at: 123 });
    const client = createTravelClient(fakeTransport(invoke));

    await expect(client.savePlan(plan)).resolves.toEqual({ ...plan, updated_at: 123 });
    expect(invoke).toHaveBeenCalledWith("travel_plan_save", { plan });
  });
});
