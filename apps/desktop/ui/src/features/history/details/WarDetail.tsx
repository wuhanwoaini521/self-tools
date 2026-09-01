import type { HistoryDetailView, HistoryNode } from "../../../types";
import { yearText } from "../labels";
import { byNodeKind, byRelationKind } from "../relations";
import { HistoryList } from "../components/HistoryList";

/**
 * WarDetail：战争详情 — 与普通事件不同：
 * 时间 / 地点 / 交战相关方 / 战争地图占位 / 战争阶段时间轴 /
 * 重要人物 / 结果 / 长期影响。
 * 当前资料库中战争以 Event 形态存储（如淝水之战），本组件按战争语义呈现。
 */
export function WarDetail({
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
  const participants = byRelationKind(relations, ["participated_in"]);
  const sides = byNodeKind(relations, ["dynasty", "person"]).filter(
    (item) => !participants.some((p) => p.node.id === item.node.id),
  );
  const battles = byNodeKind(relations, ["war", "event"]);

  return (
    <>
      <section className="history-war-hero">
        <div className="history-war-facts">
          <div>
            <span>时间</span>
            <b>{yearText(detail.start_year)}</b>
          </div>
          {places.length ? (
            <div>
              <span>地点</span>
              <b>{places.map(({ node }) => node.title).join(" · ")}</b>
            </div>
          ) : null}
          {sides.length ? (
            <div>
              <span>交战相关方</span>
              <div className="history-event-meta-list">
                {sides.map(({ node }) => (
                  <button type="button" key={node.id} onClick={() => onOpenNode(node)}>
                    {node.title}
                  </button>
                ))}
              </div>
            </div>
          ) : null}
        </div>
        <div className="history-war-map" role="img" aria-label="战争战场示意图（待补充）">
          <span>战场示意图</span>
          <p>详细战图与疆域位置将在后续版本补充。</p>
        </div>
      </section>

      {detail.course.length ? (
        <section className="history-war-phases">
          <div className="history-section-head">
            <div>
              <span>PHASES</span>
              <h2>战争阶段</h2>
            </div>
          </div>
          <ol>
            {detail.course.map((phase, index) => (
              <li key={index}>
                <time>{String(index + 1).padStart(2, "0")}</time>
                <p>{phase}</p>
              </li>
            ))}
          </ol>
        </section>
      ) : null}

      {participants.length ? (
        <section className="history-detail-section">
          <h2>重要人物</h2>
          <div className="history-relation-chips">
            {participants.map(({ relation, node }) => (
              <button type="button" key={node.id} onClick={() => onOpenNode(node)}>
                <small>{relation.note ?? "人物"}</small>
                <b>{node.title}</b>
              </button>
            ))}
          </div>
        </section>
      ) : null}

      {battles.length ? (
        <section className="history-detail-section">
          <h2>相关战役与事件</h2>
          <div className="history-relation-chips">
            {battles.map(({ relation, node }) => (
              <button type="button" key={node.id} onClick={() => onOpenNode(node)}>
                <small>{relation.note ?? "关联"}</small>
                <b>{node.title}</b>
              </button>
            ))}
          </div>
        </section>
      ) : null}

      <HistoryList title="结果" items={detail.results} />
      <HistoryList title="长期影响" items={detail.impacts} />
      <HistoryList title="史料中的不同解释" items={detail.debates} muted />
    </>
  );
}