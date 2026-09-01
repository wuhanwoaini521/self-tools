import type { HistoryDetailView, HistoryNode } from "../../../types";
import { byNodeKind } from "../relations";
import { HistoryList } from "../components/HistoryList";

/**
 * CultureDetail：文化 / 话题详情（detail_type=topic 形态）— 概览 + 要点 + 相关条目。
 */
export function CultureDetail({
  view,
  onOpenNode,
}: {
  view: HistoryDetailView;
  onOpenNode: (node: HistoryNode) => void;
}) {
  const detail = view.document.detail;
  if (detail.detail_type !== "topic") return null;
  const relations = view.relations;

  const related = byNodeKind(relations, [
    "person",
    "event",
    "place",
    "institution",
    "artifact",
  ]);

  return (
    <>
      <HistoryList title="概览" items={[detail.overview]} />
      <HistoryList title="要点" items={detail.key_points} />

      {related.length ? (
        <section className="history-detail-section">
          <h2>相关条目</h2>
          <div className="history-relation-chips">
            {related.map(({ relation, node }) => (
              <button
                type="button"
                key={node.id}
                onClick={() => onOpenNode(node)}
              >
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
