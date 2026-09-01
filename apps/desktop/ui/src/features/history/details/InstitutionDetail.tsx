import { useId } from "react";
import type { HistoryDetailView, HistoryNode } from "../../../types";
import { byNodeKind } from "../relations";
import { HistoryList } from "../components/HistoryList";

/**
 * InstitutionDetail：制度详情 — 理解要点 + 制度结构示意图（hub-spoke SVG，数据驱动），
 * 不引入重量级图库。
 */
export function InstitutionDetail({
  view,
  onOpenNode,
}: {
  view: HistoryDetailView;
  onOpenNode: (node: HistoryNode) => void;
}) {
  const detail = view.document.detail;
  if (detail.detail_type !== "institution") return null;
  const relations = view.relations;
  const titleId = useId();

  const related = byNodeKind(relations, [
    "person",
    "event",
    "dynasty",
    "place",
    "institution",
  ]);

  const points = detail.key_points;
  const W = 640;
  const H = 300;
  const CX = W / 2;
  const CY = H / 2;

  return (
    <>
      <HistoryList title="制度概览" items={[detail.overview]} />

      {points.length ? (
        <section className="history-institution-diagram" aria-labelledby={titleId}>
          <div className="history-section-head">
            <div>
              <span>STRUCTURE</span>
              <h2 id={titleId}>制度示意</h2>
            </div>
            <p>理解要点围绕制度核心展开</p>
          </div>
          <svg viewBox={`0 0 ${W} ${H}`} role="img" aria-label={`${detail.name} 的制度结构示意`}>
            {points.map((point, index) => {
              const angle = -Math.PI / 2 + ((2 * Math.PI) / points.length) * index;
              const x = CX + Math.cos(angle) * (W / 2 - 180);
              const y = CY + Math.sin(angle) * (H / 2 - 60);
              const shortened =
                point.length > 16 ? `${point.slice(0, 16)}…` : point;
              return (
                <g key={index} className="history-institution-spoke">
                  <line x1={CX} y1={CY} x2={x} y2={y} />
                  <circle cx={x} cy={y} r={44} />
                  <text x={x} y={y + 4} textAnchor="middle" className="history-institution-point">
                    {shortened}
                  </text>
                </g>
              );
            })}
            <g className="history-institution-core">
              <rect x={CX - 76} y={CY - 26} width={152} height={52} rx={10} />
              <text x={CX} y={CY + 5} textAnchor="middle" className="history-institution-name">
                {detail.name}
              </text>
            </g>
          </svg>
        </section>
      ) : null}

      <HistoryList title="理解要点" items={detail.key_points} />

      {related.length ? (
        <section className="history-detail-section">
          <h2>相关条目</h2>
          <div className="history-relation-chips">
            {related.map(({ relation, node }) => (
              <button type="button" key={node.id} onClick={() => onOpenNode(node)}>
                <small>{relation.note ?? "关联"}</small>
                <b>{node.title}</b>
              </button>
            ))}
          </div>
        </section>
      ) : null}
    </>
  );
}