import { useId } from "react";
import type { HistoryNode, HistoryRelationView } from "../../../types";
import { KIND_LABELS, RELATION_LABELS } from "../labels";

/**
 * HistoryRelationGraph：人物 / 事件 / 制度等节点的关系图。
 * 轻量自绘 SVG（不引入图库）：中心节点 + 按关系类型环绕的相关节点，
 * 边标注关系类型（前因 / 后果 / 参与 / 君臣……），点击节点进入对应 Detail。
 */

const VIEW_W = 680;
const VIEW_H = 460;
const CX = VIEW_W / 2;
const CY = VIEW_H / 2;

function nodeBox(title: string): { width: number; height: number } {
  const width = Math.min(160, Math.max(92, title.length * 17 + 24));
  return { width, height: 42 };
}

export function HistoryRelationGraph({
  center,
  relations,
  onOpenNode,
}: {
  center: HistoryNode;
  relations: HistoryRelationView[];
  onOpenNode: (node: HistoryNode) => void;
}) {
  const titleId = useId();

  if (!relations.length) {
    return (
      <section className="history-relation-graph is-empty">
        <header className="history-section-head">
          <div>
            <span>RELATIONS</span>
            <h2 id={titleId}>关系脉络</h2>
          </div>
        </header>
        <p>暂无已收录的关系数据。</p>
      </section>
    );
  }

  const centerBox = nodeBox(center.title);
  const rx = VIEW_W / 2 - 118;
  const ry = VIEW_H / 2 - 72;
  const seen = new Set<string>();
  const unique = relations.filter(({ node }) => {
    if (seen.has(node.id)) return false;
    seen.add(node.id);
    return true;
  });

  return (
    <section className="history-relation-graph" aria-labelledby={titleId}>
      <header className="history-section-head">
        <div>
          <span>RELATIONS</span>
          <h2 id={titleId}>关系脉络</h2>
        </div>
        <p>
          点击节点进入详情 · 箭头指向即「{KIND_LABELS[center.kind]} → 相关」
        </p>
      </header>
      <div className="history-relation-graph-canvas">
        <svg
          viewBox={`0 0 ${VIEW_W} ${VIEW_H}`}
          role="img"
          aria-label={`${center.title} 的关系图`}
        >
          {unique.map(({ relation, node }, index) => {
            const angle =
              -Math.PI / 2 + ((2 * Math.PI) / unique.length) * index;
            const x = CX + rx * Math.cos(angle);
            const y = CY + ry * Math.sin(angle);
            const midX = (CX + x) / 2;
            const midY = (CY + y) / 2;
            const labelRight = midX > CX;
            return (
              <g
                key={node.id}
                className="history-graph-edge"
                onClick={() => onOpenNode(node)}
              >
                <line x1={CX} y1={CY} x2={x} y2={y} />
                <text
                  x={midX}
                  y={midY - 6}
                  textAnchor={labelRight ? "start" : "end"}
                  className="history-graph-edge-label"
                >
                  {RELATION_LABELS[relation.kind]}
                </text>
                {relation.note ? (
                  <text
                    x={midX}
                    y={midY + 8}
                    textAnchor={labelRight ? "start" : "end"}
                    className="history-graph-edge-note"
                  >
                    {relation.note.length > 14
                      ? `${relation.note.slice(0, 14)}…`
                      : relation.note}
                  </text>
                ) : null}
              </g>
            );
          })}
          {/* 中心节点 */}
          <g
            className="history-graph-center"
            onClick={() => onOpenNode(center)}
          >
            <rect
              x={CX - centerBox.width / 2}
              y={CY - centerBox.height / 2}
              width={centerBox.width}
              height={centerBox.height}
              rx={8}
            />
            <text
              x={CX}
              y={CY - 1}
              textAnchor="middle"
              className="history-graph-center-title"
            >
              {center.title}
            </text>
            <text
              x={CX}
              y={CY + 13}
              textAnchor="middle"
              className="history-graph-center-kind"
            >
              {KIND_LABELS[center.kind]}
            </text>
          </g>
          {/* 环绕节点 */}
          {unique.map(({ node }, index) => {
            const angle =
              -Math.PI / 2 + ((2 * Math.PI) / unique.length) * index;
            const x = CX + rx * Math.cos(angle);
            const y = CY + ry * Math.sin(angle);
            const box = nodeBox(node.title);
            const left = x - box.width / 2;
            const top = y - box.height / 2;
            return (
              <g
                key={`node-${node.id}`}
                className="history-graph-node"
                onClick={() => onOpenNode(node)}
              >
                <rect
                  x={left}
                  y={top}
                  width={box.width}
                  height={box.height}
                  rx={7}
                />
                <text
                  x={x}
                  y={y - 1}
                  textAnchor="middle"
                  className="history-graph-node-title"
                >
                  {node.title}
                </text>
                <text
                  x={x}
                  y={y + 13}
                  textAnchor="middle"
                  className="history-graph-node-kind"
                >
                  {KIND_LABELS[node.kind]}
                </text>
              </g>
            );
          })}
        </svg>
      </div>
    </section>
  );
}
