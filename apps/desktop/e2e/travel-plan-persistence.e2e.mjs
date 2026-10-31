import { expect } from "@wdio/globals";
import { waitForApp } from "./helpers.mjs";

describe("Travel plan persists through the native Tauri boundary", () => {
  before(async () => waitForApp());

  it("exposes the assembled PersonalAgent registry and accepts run cancellation", async () => {
    const result = await browser.execute(async () => {
      const invoke = window.__TAURI_INTERNALS__?.invoke;
      if (typeof invoke !== "function") throw new Error("Tauri invoke bridge unavailable");
      const status = await invoke("personal_ai_status");
      const cancelled = await invoke("personal_ai_cancel_run", { traceId: "not-an-active-run" });
      return {
        configured: status.configured,
        modules: status.modules.map((module) => module.id),
        tools: status.tools.map((tool) => tool.name),
        cancelled,
        json: JSON.stringify(status),
      };
    });

    expect(result.configured).toBe(false);
    expect(result.modules).toEqual(expect.arrayContaining(["geography", "history", "exploration", "learning", "travel"]));
    expect(result.tools).toEqual(expect.arrayContaining(["learning.due_reviews", "travel.propose_add_place"]));
    expect(result.cancelled).toBe(false);
    expect(result.json.toLowerCase()).not.toContain("api_key");
  });

  it("round-trips a backend-owned itinerary through Travel commands", async () => {
    const city = `E2E 行程测试城市 ${Date.now()}-${Math.random().toString(36).slice(2)}`;
    const result = await browser.execute(async (city) => {
      const invoke = window.__TAURI_INTERNALS__?.invoke;
      if (typeof invoke !== "function") throw new Error("Tauri invoke bridge unavailable");
      const context = {
        city,
        days: 2,
        date_range: { start: "2026-10-10", end: "2026-10-11" },
      };
      await invoke("travel_plan_save", {
        plan: {
          context,
          selected_keys: ["e2e-poi-1"],
          assigned_days: { "e2e-poi-1": 2 },
          must_visit_keys: ["e2e-poi-1"],
        },
      });
      const loaded = await invoke("travel_plan_get", { context });
      const cleared = await invoke("travel_plan_save", {
        plan: {
          ...loaded,
          selected_keys: [],
          assigned_days: {},
          must_visit_keys: [],
        },
      });
      return {
        selectedKeys: loaded?.selected_keys,
        serverTimestamp: cleared.updated_at > 1,
      };
    }, city);

    expect(result).toEqual({ selectedKeys: ["e2e-poi-1"], serverTimestamp: true });
  });
});
