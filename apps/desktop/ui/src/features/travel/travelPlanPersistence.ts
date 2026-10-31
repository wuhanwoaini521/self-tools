import type { TravelPlan } from "../../types";

const PENDING_PREFIX = "travel-plan-pending:v1:";
const WRITER_ID_KEY = "travel-plan-writer:v1";
const DEFAULT_WRITER_ID = "default";
let cachedWriterId: string | null = null;

export interface PendingTravelPlanDraft {
  schema_version: 1;
  base_revision: number;
  plan: TravelPlan;
}

export interface TravelPlanStorage {
  readonly length?: number;
  getItem(key: string): string | null;
  key?(index: number): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

export interface PendingTravelPlanEntry {
  contextKey: string;
  writerId: string;
  draft: PendingTravelPlanDraft;
}

export type PendingTravelPlanRead =
  | { ok: true; draft: PendingTravelPlanDraft | null }
  | { ok: false; error: string };

export function pendingTravelPlanStorageKey(
  contextKey: string,
  writerId = DEFAULT_WRITER_ID,
): string {
  return `${PENDING_PREFIX}${encodeURIComponent(writerId)}:${contextKey}`;
}

export function travelPlanWriterId(): string {
  if (cachedWriterId) return cachedWriterId;
  try {
    const storage = window.sessionStorage;
    const existing = storage.getItem(WRITER_ID_KEY);
    if (existing) return (cachedWriterId = existing);
    const generated = globalThis.crypto?.randomUUID?.() ??
      `tab-${Date.now()}-${Math.random().toString(36).slice(2)}`;
    storage.setItem(WRITER_ID_KEY, generated);
    return (cachedWriterId = generated);
  } catch {
    return (cachedWriterId = `tab-${Date.now()}-${Math.random().toString(36).slice(2)}`);
  }
}

export function writePendingTravelPlan(
  storage: TravelPlanStorage,
  contextKey: string,
  draft: PendingTravelPlanDraft,
  writerId?: string,
): boolean {
  if (
    draft.schema_version !== 1 ||
    !Number.isSafeInteger(draft.base_revision) ||
    draft.base_revision < 0
  ) {
    return false;
  }
  try {
    storage.setItem(pendingTravelPlanStorageKey(contextKey, writerId), JSON.stringify(draft));
    return true;
  } catch {
    return false;
  }
}

export function readPendingTravelPlan(
  storage: TravelPlanStorage,
  contextKey: string,
  writerId?: string,
): PendingTravelPlanRead {
  let raw: string | null;
  try {
    raw = storage.getItem(pendingTravelPlanStorageKey(contextKey, writerId));
  } catch {
    return { ok: false, error: "无法读取本机的待同步行程草稿；原有内容未删除。" };
  }
  if (raw === null) return { ok: true, draft: null };

  try {
    const value = JSON.parse(raw) as Partial<PendingTravelPlanDraft>;
    if (
      value.schema_version !== 1 ||
      !Number.isSafeInteger(value.base_revision) ||
      (value.base_revision ?? -1) < 0 ||
      !isTravelPlan(value.plan)
    ) {
      return { ok: false, error: "待同步行程草稿格式无法识别；原有内容未删除。" };
    }
    return { ok: true, draft: value as PendingTravelPlanDraft };
  } catch {
    return { ok: false, error: "待同步行程草稿无法解析；原有内容未删除。" };
  }
}

export function clearPendingTravelPlanIfUnchanged(
  storage: TravelPlanStorage,
  contextKey: string,
  expected: PendingTravelPlanDraft,
  writerId?: string,
): boolean {
  const current = readPendingTravelPlan(storage, contextKey, writerId);
  if (
    !current.ok ||
    !current.draft ||
    current.draft.base_revision !== expected.base_revision ||
    !sameTravelPlanContent(current.draft.plan, expected.plan)
  ) {
    return false;
  }
  try {
    storage.removeItem(pendingTravelPlanStorageKey(contextKey, writerId));
    return true;
  } catch {
    return false;
  }
}

export function listPendingTravelPlans(
  storage: TravelPlanStorage,
): PendingTravelPlanEntry[] {
  const entries: PendingTravelPlanEntry[] = [];
  const length = storage.length ?? 0;
  try {
    for (let index = 0; index < length; index += 1) {
      const key = storage.key?.(index);
      if (!key?.startsWith(PENDING_PREFIX)) continue;
      const remainder = key.slice(PENDING_PREFIX.length);
      const separator = remainder.indexOf(":");
      if (separator < 0) continue;
      const writerId = decodeURIComponent(remainder.slice(0, separator));
      const contextKey = remainder.slice(separator + 1);
      const result = readPendingTravelPlan(storage, contextKey, writerId);
      if (result.ok && result.draft) {
        entries.push({ contextKey, writerId, draft: result.draft });
      }
    }
  } catch {
    return entries;
  }
  return entries.sort((left, right) =>
    right.draft.plan.updated_at - left.draft.plan.updated_at,
  );
}

export function reconcilePendingTravelPlan(
  remote: TravelPlan | null,
  pending: PendingTravelPlanDraft,
): { kind: "recover" | "already_saved" | "conflict" } {
  if (
    remote &&
    remote.revision >= pending.base_revision &&
    sameTravelPlanContent(remote, pending.plan)
  ) {
    return { kind: "already_saved" };
  }
  const remoteRevision = remote?.revision ?? 0;
  return remoteRevision === pending.base_revision
    ? { kind: "recover" }
    : { kind: "conflict" };
}

export function sameTravelPlanContent(left: TravelPlan, right: TravelPlan): boolean {
  const snapshot = (plan: TravelPlan) => ({
    context: plan.context,
    title: plan.title,
    departure: plan.departure,
    notes: plan.notes,
    custom_places: Object.entries(plan.custom_places ?? {}).sort(([a], [b]) => a.localeCompare(b)),
    selected_keys: plan.selected_keys,
    assigned_days: Object.entries(plan.assigned_days).sort(([a], [b]) => a.localeCompare(b)),
    must_visit_keys: plan.must_visit_keys,
  });
  return JSON.stringify(snapshot(left)) === JSON.stringify(snapshot(right));
}

function isTravelPlan(value: unknown): value is TravelPlan {
  if (!value || typeof value !== "object") return false;
  const plan = value as Partial<TravelPlan>;
  return Boolean(
    plan.context &&
      typeof plan.context.city === "string" &&
      Number.isInteger(plan.context.days) &&
      Array.isArray(plan.selected_keys) &&
      plan.assigned_days &&
      typeof plan.assigned_days === "object" &&
      Array.isArray(plan.must_visit_keys) &&
      Number.isSafeInteger(plan.revision),
  );
}
