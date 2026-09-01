import { UserFocus } from "@phosphor-icons/react";
import type { HistoryNode } from "../../../types";
import type { HistoryEra } from "../historyEras";
import { compactYear } from "../labels";
import type { HistoryViewMode } from "../types/history";

/**
 * People View（人物）：展示当前时代下已收录的真实人物节点。
 * 数据来自 history_period_nodes；人物资料较少时如实说明，不伪造。
 */
export function PeopleView({
  era,
  persons,
  onOpenNode,
  onSwitchView,
}: {
  era: HistoryEra;
  persons: HistoryNode[];
  onOpenNode: (node: HistoryNode) => void;
  onSwitchView: (view: HistoryViewMode) => void;
}) {
  const people = persons.filter((node) => node.kind === "person");

  return (
    <section className="history-people-view">
      <header className="history-section-head">
        <div>
          <span>PEOPLE</span>
          <h2>{era.name} · 人物</h2>
        </div>
        <p>
          当前收录 {people.length} 位 · 更多人物资料随离线资料库持续补充
        </p>
      </header>

      {people.length ? (
        <div className="history-people-grid">
          {people.map((person) => (
            <button
              type="button"
              key={person.id}
              className="history-person-card"
              onClick={() => onOpenNode(person)}
            >
              <span className="history-person-card-icon">
                <UserFocus size={20} weight="duotone" />
              </span>
              <b>{person.title}</b>
              <small>
                {person.start_year === null
                  ? ""
                  : `${compactYear(person.start_year)}—${person.end_year === null ? "" : compactYear(person.end_year)}`}
              </small>
              <p>{person.summary}</p>
            </button>
          ))}
        </div>
      ) : (
        <p className="history-view-empty">
          该时代的人物资料仍在整理中。可以先前往
          <button type="button" onClick={() => onSwitchView("timeline")}>
            时间视图
          </button>
          浏览该时代的时间线。
        </p>
      )}
    </section>
  );
}