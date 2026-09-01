import { ArrowRight } from "@phosphor-icons/react";
import type { HistoryNode } from "../../../types";
import { KIND_LABELS } from "../labels";
import {
  EXPLORE_KIND_LABELS,
  EXPLORE_KINDS,
  type ExploreKind,
} from "../types/history";

/**
 * EraExploreGrid：「认识这个时代」。
 * 按 人物 / 事件 / 地点 / 战争 / 制度 / 文化 / 文物 分类展示当前时代的真实节点
 * （来自 history_period_nodes），每类 3–5 个；没有数据的分类不展示。
 */
export function EraExploreGrid({
  nodes,
  onOpenNode,
}: {
  nodes: HistoryNode[];
  onOpenNode: (node: HistoryNode) => void;
}) {
  const grouped = EXPLORE_KINDS.map((kind) => ({
    kind,
    items: nodes.filter((node) => node.kind === kind),
  })).filter((group) => group.items.length > 0);

  if (!grouped.length) {
    return (
      <section className="history-explore-grid is-empty">
        <header className="history-section-head">
          <div>
            <span>EXPLORE</span>
            <h2>认识这个时代</h2>
          </div>
        </header>
        <p className="history-explore-empty">
          该时代的专题资料正在整理中。可以先从下方时间线与时代信息继续探索。
        </p>
      </section>
    );
  }

  return (
    <section className="history-explore-grid">
      <header className="history-section-head">
        <div>
          <span>EXPLORE</span>
          <h2>认识这个时代</h2>
        </div>
        <p>点击任意条目进入详情 · 从人物到事件、地点与制度继续展开</p>
      </header>
      <div className="history-explore-cols">
        {grouped.map((group) => (
          <ExploreColumn
            key={group.kind}
            kind={group.kind}
            items={group.items.slice(0, 5)}
            onOpenNode={onOpenNode}
          />
        ))}
      </div>
    </section>
  );
}

function ExploreColumn({
  kind,
  items,
  onOpenNode,
}: {
  kind: ExploreKind;
  items: HistoryNode[];
  onOpenNode: (node: HistoryNode) => void;
}) {
  return (
    <div className="history-explore-col">
      <h3>{EXPLORE_KIND_LABELS[kind]}</h3>
      <ul>
        {items.map((node) => (
          <li key={node.id}>
            <button type="button" onClick={() => onOpenNode(node)}>
              <span className="history-explore-kind">{KIND_LABELS[node.kind]}</span>
              <b>{node.title}</b>
              <small>{node.summary}</small>
              <ArrowRight size={14} aria-hidden="true" />
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}