import type { HistoryNodeKind, HistoryRelationKind } from "../../types";

export const KIND_LABELS: Record<HistoryNodeKind, string> = {
  person: "人物",
  event: "事件",
  dynasty: "朝代",
  place: "地点",
  war: "战争",
  institution: "制度",
  artifact: "文物",
  culture: "文化",
};

export const RELATION_LABELS: Record<HistoryRelationKind, string> = {
  occurred_in: "发生于",
  participated_in: "参与",
  belongs_to: "属于",
  cause: "前因",
  consequence: "后果",
  related_to: "相关",
  family: "亲属",
  political_ally: "政治盟友",
  political_opponent: "政治对手",
  monarch_minister: "君臣",
  military_opponent: "军事对手",
  predecessor: "前任",
  successor: "继任",
};

/** 关系是否属于「因果 / 时间先后」类，用于详情里分组展示。 */
export function isCausalKind(kind: HistoryRelationKind): boolean {
  return (
    kind === "cause" ||
    kind === "consequence" ||
    kind === "predecessor" ||
    kind === "successor"
  );
}

export function yearText(value: number | null): string {
  if (value === null) return "时间待考";
  return value < 0 ? `公元前 ${Math.abs(value)} 年` : `${value} 年`;
}

export function rangeText(start: number | null, end: number | null): string {
  return end === null || end === start
    ? yearText(start)
    : `${yearText(start)} · ${yearText(end)}`;
}

export function compactYear(value: number): string {
  return value < 0 ? `前${Math.abs(value)}` : `${value}`;
}
