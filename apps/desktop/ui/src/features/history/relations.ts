import type {
  HistoryNodeKind,
  HistoryRelationKind,
  HistoryRelationView,
} from "../../types";

/** 按关系类型过滤 detail 的相关视图。 */
export function byRelationKind(
  relations: HistoryRelationView[],
  kinds: HistoryRelationKind[],
): HistoryRelationView[] {
  return relations.filter((item) => kinds.includes(item.relation.kind));
}

/** 按目标节点类型过滤 detail 的相关视图。 */
export function byNodeKind(
  relations: HistoryRelationView[],
  kinds: HistoryNodeKind[],
): HistoryRelationView[] {
  return relations.filter((item) => kinds.includes(item.node.kind));
}
