import type { HistoryDetailView, HistoryNode } from "../../../types";
import { yearText } from "../labels";
import { byNodeKind, byRelationKind } from "../relations";
import { HistoryList } from "../components/HistoryList";

/**
 * EventDetail：事件详情 — 结构化为
 * 时间 / 地点 / 参与人物 / 背景 / 导火索 / 过程 / 结果 / 长期影响 / 不同解释 /
 * 前因 / 后果 / 相关事件。
 */
export function EventDetail({
  view,
  onOpenNode,
}: {
  view: HistoryDetailView;
  onOpenNode: (node: HistoryNode) => void;
}) {
  const detail = view.document.detail;
  if (detail.detail_type !== "event") return null;
  const relations = view.relations;

  const places = byRelationKind(relations, ["occurred_in"]).concat(
    byNodeKind(relations, ["place"]),
  );
  const participants = byRelationKind(relations, ["participated_in"]).concat(
    byNodeKind(relations, ["person"]).filter(
      (item) => item.relation.kind !== "participated_in",
    ),
  );
  const causal = byRelationKind(relations, [
    "cause",
    "consequence",
    "predecessor",
    "successor",
  ]);
  const relatedEvents = byNodeKind(relations, ["event", "war"]).filter(
    (item) => !causal.some((c) => c.node.id === item.node.id),
  );

  return (
    <>
      <section className="history-event-meta">
        <div>
          <span>时间</span>
          <b>
            {yearText(detail.start_year)}
            {detail.end_year !== null && detail.end_year !== detail.start_year
              ? ` — ${yearText(detail.end_year)}`
              : ""}
          </b>
        </div>
        {places.length ? (
          <div>
            <span>地点</span>
            <div className="history-event-meta-list">
              {places.map(({ node }) => (
                <button type="button" key={node.id} onClick={() => onOpenNode(node)}>
                  {node.title}
                </button>
              ))}
            </div>
          </div>
        ) : null}
        {participants.length ? (
          <div>
            <span>参与人物</span>
            <div className="history-event-meta-list">
              {participants.map(({ node }) => (
                <button type="button" key={node.id} onClick={() => onOpenNode(node)}>
                  {node.title}
                </button>
              ))}
            </div>
          </div>
        ) : null}
      </section>

      <HistoryList title="背景" items={detail.background} />
      {detail.trigger ? <HistoryList title="导火索" items={[detail.trigger]} /> : null}
      <HistoryList title="过程" items={detail.course} />
      <HistoryList title="结果" items={detail.results} />
      <HistoryList title="长期影响" items={detail.impacts} />
      <HistoryList title="不同解释" items={detail.debates} muted />

      {causal.length ? (
        <section className="history-detail-section">
          <h2>前因与后果</h2>
          <div className="history-relation-chips">
            {causal.map(({ relation, node }) => (
              <button type="button" key={node.id} onClick={() => onOpenNode(node)}>
                <small>{relation.note ?? "关联"}</small>
                <b>{node.title}</b>
              </button>
            ))}
          </div>
        </section>
      ) : null}

      {relatedEvents.length ? (
        <section className="history-detail-section">
          <h2>相关事件</h2>
          <div className="history-relation-chips">
            {relatedEvents.map(({ relation, node }) => (
              <button type="button" key={node.id} onClick={() => onOpenNode(node)}>
                <small>{relation.note ?? "相关事件"}</small>
                <b>{node.title}</b>
              </button>
            ))}
          </div>
        </section>
      ) : null}
    </>
  );
}