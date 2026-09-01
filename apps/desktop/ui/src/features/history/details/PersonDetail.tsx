import type { HistoryDetailView, HistoryNode } from "../../../types";
import { compactYear, yearText } from "../labels";
import { byNodeKind, byRelationKind } from "../relations";
import { HistoryList } from "../components/HistoryList";

/**
 * PersonDetail：人物详情 — 专属布局。
 * 人物 Hero（姓名 / 生卒 / 身份）+ 人生时间轴（出生 → 脉络 → 去世）+
 * 关键事件 / 相关人物 / 相关地点 / 相关制度。
 */
export function PersonDetail({
  view,
  onOpenNode,
}: {
  view: HistoryDetailView;
  onOpenNode: (node: HistoryNode) => void;
}) {
  const detail = view.document.detail;
  if (detail.detail_type !== "person") return null;
  const relations = view.relations;

  const keyEvents = byRelationKind(relations, ["participated_in"]).concat(
    byNodeKind(relations, ["event"]),
  );
  const people = byNodeKind(relations, ["person"]);
  const places = byNodeKind(relations, ["place"]);
  const institutions = byNodeKind(relations, ["institution"]);
  const dynasty = byRelationKind(relations, ["belongs_to"]);

  const milestones = detail.biography;

  return (
    <>
      <section className="history-person-hero">
        <div>
          <span className="history-person-life">
            {detail.born_year === null
              ? "生卒时间待考"
              : `${yearText(detail.born_year)} — ${detail.died_year === null ? "卒年待考" : yearText(detail.died_year)}`}
          </span>
          <h2>{detail.name}</h2>
        </div>
        <div className="history-person-identities">
          {detail.identities.map((identity) => (
            <span key={identity}>{identity}</span>
          ))}
        </div>
      </section>

      {milestones.length ? (
        <section className="history-person-timeline">
          <div className="history-section-head">
            <div>
              <span>LIFE</span>
              <h2>人生脉络</h2>
            </div>
          </div>
          <ol>
            {detail.born_year === null ? null : (
              <li className="is-boundary">
                <time>{compactYear(detail.born_year)}</time>
                <b>出生</b>
              </li>
            )}
            {milestones.map((item, index) => (
              <li key={index}>
                <time>{String(index + 1).padStart(2, "0")}</time>
                <p>{item}</p>
              </li>
            ))}
            {detail.died_year === null ? null : (
              <li className="is-boundary">
                <time>{compactYear(detail.died_year)}</time>
                <b>去世</b>
              </li>
            )}
          </ol>
        </section>
      ) : null}

      <HistoryList title="成就与影响" items={detail.achievements} />
      <HistoryList title="争议与评价" items={detail.controversies} muted />

      {keyEvents.length ? (
        <section className="history-detail-section">
          <h2>关键事件</h2>
          <div className="history-relation-chips">
            {keyEvents.map(({ relation, node }) => (
              <button type="button" key={node.id} onClick={() => onOpenNode(node)}>
                <small>{relation.note ?? "事件"}</small>
                <b>{node.title}</b>
              </button>
            ))}
          </div>
        </section>
      ) : null}

      {dynasty.length ? (
        <section className="history-detail-section">
          <h2>所属时代</h2>
          <div className="history-relation-chips">
            {dynasty.map(({ node }) => (
              <button type="button" key={node.id} onClick={() => onOpenNode(node)}>
                <small>时代</small>
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
                <small>{relation.note ?? "相关人物"}</small>
                <b>{node.title}</b>
              </button>
            ))}
          </div>
        </section>
      ) : null}

      {places.length ? (
        <section className="history-detail-section">
          <h2>相关地点</h2>
          <div className="history-relation-chips">
            {places.map(({ relation, node }) => (
              <button type="button" key={node.id} onClick={() => onOpenNode(node)}>
                <small>{relation.note ?? "地点"}</small>
                <b>{node.title}</b>
              </button>
            ))}
          </div>
        </section>
      ) : null}

      {institutions.length ? (
        <section className="history-detail-section">
          <h2>相关制度</h2>
          <div className="history-relation-chips">
            {institutions.map(({ relation, node }) => (
              <button type="button" key={node.id} onClick={() => onOpenNode(node)}>
                <small>{relation.note ?? "制度"}</small>
                <b>{node.title}</b>
              </button>
            ))}
          </div>
        </section>
      ) : null}
    </>
  );
}