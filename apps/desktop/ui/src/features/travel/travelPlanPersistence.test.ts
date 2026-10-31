import { describe, expect, it } from "vitest";
import type { TravelPlan } from "../../types";
import {
  clearPendingTravelPlanIfUnchanged,
  listPendingTravelPlans,
  readPendingTravelPlan,
  reconcilePendingTravelPlan,
  writePendingTravelPlan,
  type PendingTravelPlanDraft,
} from "./travelPlanPersistence";

class MemoryStorage {
  private readonly values = new Map<string, string>();

  get length(): number {
    return this.values.size;
  }

  key(index: number): string | null {
    return [...this.values.keys()][index] ?? null;
  }

  getItem(key: string): string | null {
    return this.values.get(key) ?? null;
  }

  setItem(key: string, value: string): void {
    this.values.set(key, value);
  }

  removeItem(key: string): void {
    this.values.delete(key);
  }
}

function plan(overrides: Partial<TravelPlan> = {}): TravelPlan {
  return {
    context: { city: "杭州", days: 2, date_range: null },
    title: "西湖慢行",
    departure: "杭州东站",
    notes: "预留午休",
    custom_places: { "custom:west-lake": { name: "西湖", note: "傍晚散步" } },
    selected_keys: ["custom:west-lake"],
    assigned_days: { "custom:west-lake": 1 },
    must_visit_keys: [],
    revision: 0,
    updated_at: 0,
    ...overrides,
  };
}

function draft(overrides: Partial<PendingTravelPlanDraft> = {}): PendingTravelPlanDraft {
  return {
    schema_version: 1,
    base_revision: 0,
    plan: plan(),
    ...overrides,
  };
}

describe("Travel pending-plan recovery", () => {
  it("keeps a newer local draft when an older save response arrives", () => {
    const storage = new MemoryStorage();
    const key = "travel-plan:杭州:2::";
    const older = draft();
    const newer = draft({ plan: plan({ notes: "新加一条备注" }) });

    expect(writePendingTravelPlan(storage, key, older)).toBe(true);
    expect(writePendingTravelPlan(storage, key, newer)).toBe(true);
    expect(clearPendingTravelPlanIfUnchanged(storage, key, older)).toBe(false);
    expect(readPendingTravelPlan(storage, key)).toEqual({ ok: true, draft: newer });

    expect(clearPendingTravelPlanIfUnchanged(storage, key, newer)).toBe(true);
    expect(readPendingTravelPlan(storage, key)).toEqual({ ok: true, draft: null });
  });

  it("keeps drafts from separate browser tabs discoverable", () => {
    const storage = new MemoryStorage();
    const value = draft();
    writePendingTravelPlan(storage, "travel-plan:杭州:2::", value, "tab-a");
    writePendingTravelPlan(storage, "travel-plan:杭州:2::", value, "tab-b");

    expect(listPendingTravelPlans(storage)).toHaveLength(2);
    expect(listPendingTravelPlans(storage).map((entry) => entry.writerId).sort()).toEqual([
      "tab-a",
      "tab-b",
    ]);
  });

  it("recovers a draft only when its base revision still matches the server", () => {
    const pending = draft({ base_revision: 4, plan: plan({ revision: 4 }) });
    const remote = plan({ revision: 4, notes: "服务器旧备注" });

    expect(reconcilePendingTravelPlan(remote, pending).kind).toBe("recover");
    expect(
      reconcilePendingTravelPlan(plan({ revision: 5, notes: "另一客户端" }), pending).kind,
    ).toBe("conflict");
    expect(reconcilePendingTravelPlan(plan({ revision: 5 }), pending).kind).toBe("already_saved");
  });
});
