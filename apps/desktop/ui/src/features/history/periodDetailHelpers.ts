/**
 * Period Detail 的纯数据辅助（无 React 依赖，便于单元理解与复用）。
 *
 * 所有"叙事语义"均从真实数据推导：
 * - 阶段（stages）由后端以 critical 锚点分段（见 application/history/service.rs）；
 * - 前后时代由时期边界重叠推导（不引入硬编码排序）；
 * - 主线关系从时期关系池中选择因果类（leads_to / causes / contributes_to）。
 */
import type {
  SemanticEventRelation,
  SemanticPeriod,
  SemanticPeriodEvent,
  SemanticPeriodStage,
} from "./semanticTypes";

export const EVENT_TYPE_LABELS: Record<string, string> = {
  war: "战争",
  political: "政治",
  foundation: "制度",
  diplomatic: "外交",
  rebellion: "起义",
  treaty: "条约",
  cultural: "文化",
  succession: "权力更替",
  alliance: "联盟",
  reform: "改革",
  "political-military": "军政",
};

export function eventTypeLabel(value: string | null | undefined): string {
  return value ? (EVENT_TYPE_LABELS[value] ?? value) : "事件";
}

/** 前进方向的关系动词（用于「↓ 引向 五四运动」一类的主线链）。 */
export const CHAIN_VERBS: Record<string, string> = {
  leads_to: "引向",
  causes: "导致",
  contributes_to: "促成",
  precedes: "先于",
  follows: "随后",
  part_of: "隶属",
  related_to: "关联",
  caused_by: "缘起",
};

export function chainVerb(type: string): string {
  return CHAIN_VERBS[type] ?? "关联";
}

export function yearText(value: number | null | undefined): string {
  if (value === null || value === undefined) return "年代待考";
  return value < 0 ? `前${Math.abs(value)}` : `${value}`;
}

export function rangeText(
  start: number | null | undefined,
  end: number | null | undefined,
): string {
  if (start === null || start === undefined) return "年代待考";
  if (end === null || end === undefined || start === end)
    return yearText(start);
  return `${yearText(start)} — ${yearText(end)}`;
}

/**
 * 前后时代：以"边界不重叠"推导 —— 上一个为开始年份最大的前驱，
 * 下一个为开始年份 ≥ 本时期结束年份的首个后继（按现有 rail 排序）。
 * 结果不含"包含型"时期（如近现代 1840–2000 不会出现在中华民国的上一格）。
 */
export function eraNeighbors(
  periods: SemanticPeriod[],
  current: SemanticPeriod | null,
): { prev: SemanticPeriod | null; next: SemanticPeriod | null } {
  if (!current) return { prev: null, next: null };
  const start = current.start_year;
  const end = current.end_year;
  let prev: SemanticPeriod | null = null;
  let next: SemanticPeriod | null = null;
  for (const period of periods) {
    const candidate = period.start_year;
    if (candidate === null || candidate === undefined) continue;
    if (period.id === current.id) continue;
    if (candidate < (start ?? Number.NEGATIVE_INFINITY)) {
      // 并列起年时取结束更早者（晚清 1911 先于 近现代 2000，确定性不依赖 id 排序）。
      const prevStart = prev?.start_year ?? Number.NEGATIVE_INFINITY;
      const prevEnd = prev?.end_year ?? Number.POSITIVE_INFINITY;
      const sameStart = candidate === prevStart && period.id !== prev?.id;
      if (
        candidate > prevStart ||
        (sameStart && (period.end_year ?? Number.POSITIVE_INFINITY) < prevEnd)
      ) {
        prev = period;
      }
    } else if (
      end !== null &&
      end !== undefined &&
      candidate >= end &&
      (!next || candidate < (next.start_year ?? Number.POSITIVE_INFINITY))
    ) {
      next = period;
    }
  }
  return { prev, next };
}

/** 事件是否落在某阶段区间内（阶段由起止年份定义）。 */
export function eventInStage(
  event: SemanticPeriodEvent,
  stage: SemanticPeriodStage,
): boolean {
  const year = event.start_year;
  if (year === null || year === undefined) return false;
  return year >= stage.start_year && year <= stage.end_year;
}

/** 阶段内事件（保持传入顺序 = 年份升序）。 */
export function eventsForStage(
  events: SemanticPeriodEvent[],
  stage: SemanticPeriodStage,
): SemanticPeriodEvent[] {
  return events.filter((event) => eventInStage(event, stage));
}

/** 主线关系：该事件作为源头时，最有叙事价值的一条出向关系。 */
export interface PrimaryRelation {
  kind: "causal" | "sequential" | "associative";
  relation: SemanticEventRelation;
}

const CAUSAL_TYPES = new Set(["leads_to", "causes", "contributes_to"]);
const SEQUENTIAL_TYPES = new Set(["precedes", "follows", "part_of"]);

function relationKind(type: string): PrimaryRelation["kind"] {
  if (CAUSAL_TYPES.has(type)) return "causal";
  if (SEQUENTIAL_TYPES.has(type)) return "sequential";
  return "associative";
}

/** 关系作为叙事主线的评分：因果 > 顺序 > 关联，confidence 作为次级排序。 */
function primaryScore(
  kind: PrimaryRelation["kind"],
  confidence: number | null | undefined,
): number {
  let kindPenalty: number;
  if (kind === "causal") {
    kindPenalty = 0;
  } else if (kind === "sequential") {
    kindPenalty = 1;
  } else {
    kindPenalty = 2;
  }
  return (confidence ?? 0) * 0.01 - kindPenalty;
}

export function primaryOutgoingRelation(
  eventId: string,
  relations: SemanticEventRelation[],
): PrimaryRelation | null {
  let best: PrimaryRelation | null = null;
  for (const relation of relations) {
    if (relation.source_event_id !== eventId) continue;
    const kind = relationKind(relation.relation_type ?? "");
    const candidate: PrimaryRelation = { kind, relation };
    if (
      !best ||
      primaryScore(kind, relation.confidence) >
        primaryScore(best.kind, best.relation.confidence)
    ) {
      best = candidate;
    }
  }
  return best;
}

/** 关键转折的事件级关系链（因果优先，最多 3 条，用于 turning points 的「它连接到了什么」）。 */
export function turningPointLinks(
  eventId: string,
  relations: SemanticEventRelation[],
): SemanticEventRelation[] {
  const ranked = relations
    .filter((relation) => relation.source_event_id === eventId)
    .map((relation) => ({
      relation,
      score: primaryScore(
        relationKind(relation.relation_type ?? ""),
        relation.confidence,
      ),
    }))
    .sort((left, right) => right.score - left.score);
  return ranked.slice(0, 3).map((item) => item.relation);
}

/** 事件类型分布摘要（"政治、战争"），用于阶段与章节 header。 */
export function typeSummary(events: SemanticPeriodEvent[]): string {
  const counts = new Map<string, number>();
  for (const event of events) {
    const label = eventTypeLabel(event.event_type);
    counts.set(label, (counts.get(label) ?? 0) + 1);
  }
  return Array.from(counts.entries())
    .sort((left, right) => right[1] - left[1])
    .slice(0, 3)
    .map(([label, count]) => `${label} ${count}`)
    .join(" · ");
}

/** 阶段内“里程碑”事件名（按 relation_count 降序，最多 3 个），用于章前导语。 */
export function stageLandmarks(events: SemanticPeriodEvent[]): string[] {
  return [...events]
    .sort((left, right) => right.relation_count - left.relation_count)
    .slice(0, 3)
    .map((event) => event.name_zh_cn);
}

/** 章标题的叙事链：本阶段锚点名 → 下一阶段锚点名（末章用本时期最后事件代替）。 */
export function stageChain(
  stage: SemanticPeriodStage,
  stages: SemanticPeriodStage[],
  events: SemanticPeriodEvent[],
): { from: string; to: string } {
  const from = stage.opening_event_name || "时期开端";
  const next = stages.find((candidate) => candidate.index === stage.index + 1);
  let to = next?.opening_event_name;
  if (!to) {
    const last = [...events]
      .filter((event) => eventInStage(event, stage))
      .at(-1);
    to = last?.name_zh_cn ?? "时期终点";
  }
  if (to === from) to = "时期终点";
  return { from, to };
}
