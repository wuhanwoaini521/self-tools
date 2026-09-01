import type { HistoryDetailView, HistoryNode } from "../../../types";
import { byNodeKind, byRelationKind } from "../relations";
import { HistoryList } from "../components/HistoryList";

/**
 * PlaceDetail：地点详情 — 概览 / 现代名称 / 历史名称 / 相关；
 * 「在地图中查看」通过 DetailShell 的 onMap 提供（低耦合跨 Feature adapter）。
 */
export function PlaceDetail({
  view,
  onOpenNode,
}: {
  view: HistoryDetailView;
  onOpenNode: (node: HistoryNode) => void;
}) {
  const detail = view.document.detail;
  if (detail.detail_type !== "place") return null;
  const relations = view.relations;

  const events = byRelationKind(relations, ["occurred_in"]).concat(
    byNodeKind(relations, ["event", "war"]),
  );
  const people = byNodeKind(relations, ["person"]);
  const dynasties = byNodeKind(relations, ["dynasty"]);

  return (
    <>
      {detail.latitude !== null && detail.longitude !== null ? (
        <p className="history-place-coords">
          {detail.latitude.toFixed(3)}° N · {detail.longitude.toFixed(3)}° E
        </p>
      ) : null}
      <HistoryList title="地点概览" items={[detail.overview]} />
      {detail.modern_name ? (
        <HistoryList title="现代名称" items={[detail.modern_name]} />
      ) : null}
      {detail.historical_names.length ? (
        <HistoryList title="历史名称" items={detail.historical_names} />
      ) : null}

      {events.length ? (
        <section className="history-detail-section">
          <h2>发生于此的事件</h2>
          <div className="history-relation-chips">
            {events.map(({ relation, node }) => (
              <button type="button" key={node.id} onClick={() => onOpenNode(node)}>
                <small>{relation.note ?? "事件"}</small>
                <b>{node.title}</b>
              </button>
            ))}
          </div>
        </section>
      ) : null}

      {people.length ? (
        <section className="history-detail-section">
          <h2>相关人物</h2>
          <div className="history-relation-chips">
            {people.map(({ relation, node }) => (
              <button type="button" key={node.id} onClick={() => onOpenNode(node)}>
                <small>{relation.note ?? "人物"}</small>
                <b>{node.title}</b>
              </button>
            ))}
          </div>
        </section>
      ) : null}

      {dynasties.length ? (
        <section className="history-detail-section">
          <h2>相关时代</h2>
          <div className="history-relation-chips">
            {dynasties.map(({ relation, node }) => (
              <button type="button" key={node.id} onClick={() => onOpenNode(node)}>
                <small>{relation.note ?? "时代"}</small>
                <b>{node.title}</b>
              </button>
            ))}
          </div>
        </section>
      ) : null}
    </>
  );
}