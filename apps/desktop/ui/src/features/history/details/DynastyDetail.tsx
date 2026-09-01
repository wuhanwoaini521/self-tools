import type { HistoryDetailView } from "../../../types";
import type { HistoryEra } from "../historyEras";
import { ERA_ICONS } from "../historyEras";
import type { HistorySection } from "../../../types";
import { HistoryList } from "../components/HistoryList";

/**
 * DynastyDetail：朝代详情 — 时代导读。
 * 兼容两种资料形态：结构化 Dynasty（概览 + 分节）与 Topic（概览 + 要点）；
 * 并用本页 historyEras 的 facts / highlights / tags 补全时代信息。
 */
export function DynastyDetail({
  view,
  era,
  onOpenEra,
}: {
  view: HistoryDetailView;
  era: HistoryEra | null;
  onOpenEra: (era: HistoryEra) => void;
}) {
  const detail = view.document.detail;
  const kind = detail.detail_type;
  const isDynasty = kind === "dynasty";
  const name = isDynasty ? detail.name : view.document.node.title;
  const overview = isDynasty
    ? detail.overview
    : (detail as { overview: string }).overview;
  const sections: HistorySection[] = isDynasty ? detail.sections : [];
  const keyPoints =
    kind === "topic" ? (detail as { key_points: string[] }).key_points : [];

  return (
    <>
      <section className="history-study-guide">
        <div className="history-study-lead">
          <div>
            <small>时代导读</small>
            <h2>先建立一张 {name} 的时代地图</h2>
          </div>
          <p>{overview}</p>
        </div>

        {era ? (
          <>
            <div className="history-study-facts">
              {era.facts.map((fact) => {
                const Icon = ERA_ICONS[fact.icon];
                return (
                  <article key={`${fact.label}-${fact.value}`}>
                    <Icon size={20} weight="duotone" />
                    <div>
                      <small>{fact.label}</small>
                      <b>{fact.value}</b>
                      <span>{fact.caption}</span>
                    </div>
                  </article>
                );
              })}
            </div>
            <section className="history-study-highlights">
              <header>
                <h2>从三条线索开始</h2>
                <span>事件 · 人物 · 文化</span>
              </header>
              <div>
                {era.highlights.map((highlight) => (
                  <button
                    type="button"
                    key={highlight.eyebrow}
                    className="history-study-highlight-card"
                    onClick={() => onOpenEra(era)}
                  >
                    <small>{highlight.eyebrow}</small>
                    <b>{highlight.title}</b>
                    <p>{highlight.description}</p>
                  </button>
                ))}
              </div>
            </section>
          </>
        ) : null}

        {sections.map((section) => (
          <HistoryList
            key={section.title}
            title={section.title}
            items={section.items}
          />
        ))}
        {keyPoints.length ? (
          <HistoryList title="理解要点" items={keyPoints} />
        ) : null}

        {isDynasty ? (
          <div className="history-detail-section">
            <h2>政权性质</h2>
            <ul>
              <li>{detail.regime_type}</li>
            </ul>
            {detail.capital ? (
              <HistoryList title="都城" items={[detail.capital]} />
            ) : null}
          </div>
        ) : null}

        {era ? (
          <div className="history-study-tags">
            <span>本页关键词</span>
            {era.tags.map((tag) => (
              <b key={tag}>{tag}</b>
            ))}
          </div>
        ) : null}
      </section>
    </>
  );
}
