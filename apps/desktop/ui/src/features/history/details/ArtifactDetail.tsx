import type { HistoryDetailView, HistoryNode } from "../../../types";
import { byNodeKind } from "../relations";
import { HistoryList } from "../components/HistoryList";

/**
 * ArtifactDetail：文物详情 — 概览 / 收藏单位 / 相关条目。
 */
export function ArtifactDetail({
  view,
  onOpenNode,
}: {
  view: HistoryDetailView;
  onOpenNode: (node: HistoryNode) => void;
}) {
  const detail = view.document.detail;
  if (detail.detail_type !== "artifact") return null;
  const relations = view.relations;

  const related = byNodeKind(relations, [
    "person",
    "event",
    "dynasty",
    "place",
    "institution",
    "culture",
  ]);

  return (
    <>
      <HistoryList title="文物概览" items={[detail.overview]} />
      {detail.collection ? (
        <HistoryList title="收藏单位" items={[detail.collection]} />
      ) : null}

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
