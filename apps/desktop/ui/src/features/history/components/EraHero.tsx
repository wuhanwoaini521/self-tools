import {
  ArrowRight,
  Buildings,
  CompassRose,
  MapPin,
  UserFocus,
} from "@phosphor-icons/react";
import type { HistoryNode } from "../../../types";
import { ERA_ICONS, type HistoryEra } from "../historyEras";
import { compactYear } from "../labels";
import { EraVisualPlate } from "./EraVisualPlate";

function durationOf(era: HistoryEra) {
  return Math.max(1, era.endYear - era.startYear);
}

/**
 * Era Hero：History 首页核心区域。
 * 左栏 = 时代视觉（EraVisualPlate，数据驱动的视觉身份）；
 * 右栏 = 时代信息（定位、起止、都城、制度、交流、代表人物、标签）。
 */
export function EraHero({
  era,
  persons,
  onOpenEra,
  onOpenNode,
  onStartStory,
}: {
  era: HistoryEra;
  /** 该时代下的人物节点（真实数据，来自 history_period_nodes） */
  persons: HistoryNode[];
  onOpenEra: (era: HistoryEra) => void;
  onOpenNode: (node: HistoryNode) => void;
  onStartStory?: () => void;
}) {
  const capitalFact = era.facts.find((fact) => fact.icon === "capital");
  const systemFact = era.facts.find((fact) => fact.icon === "system");
  const exchangeFact = era.facts.find((fact) => fact.icon === "exchange");
  const ExchangeIcon = ERA_ICONS.exchange;

  return (
    <section className="history-hero" aria-label={`${era.name} 时代概览`}>
      <EraVisualPlate era={era} className="history-hero-visual" />
      <div className="history-hero-info">
        <header className="history-hero-head">
          <div>
            <span className="history-hero-kicker">当前时代</span>
            <h2>{era.name}</h2>
            <p className="history-hero-range">
              {era.yearLabel} · 历时 {durationOf(era)} 年
            </p>
          </div>
          <button
            type="button"
            className="history-hero-open"
            onClick={() => onOpenEra(era)}
          >
            深入了解{era.name} <ArrowRight size={15} />
          </button>
        </header>

        {era.tagline ? (
          <p className="history-hero-tagline">{era.tagline}</p>
        ) : null}

        <dl className="history-hero-facts">
          {capitalFact ? (
            <div>
              <dt>
                <MapPin size={16} weight="duotone" />
                {capitalFact.label}
              </dt>
              <dd>
                {capitalFact.value}
                <small>{capitalFact.caption}</small>
              </dd>
            </div>
          ) : null}
          {systemFact ? (
            <div>
              <dt>
                <Buildings size={16} weight="duotone" />
                {systemFact.label}
              </dt>
              <dd>
                {systemFact.value}
                <small>{systemFact.caption}</small>
              </dd>
            </div>
          ) : null}
          {exchangeFact ? (
            <div>
              <dt>
                <ExchangeIcon size={16} weight="duotone" />
                {exchangeFact.label}
              </dt>
              <dd>
                {exchangeFact.value}
                <small>{exchangeFact.caption}</small>
              </dd>
            </div>
          ) : null}
        </dl>

        {persons.length ? (
          <div className="history-hero-persons">
            <span className="history-hero-persons-label">
              <UserFocus size={14} weight="duotone" />
              代表人物
            </span>
            <div className="history-hero-persons-list">
              {persons.slice(0, 4).map((person) => (
                <button
                  type="button"
                  key={person.id}
                  onClick={() => onOpenNode(person)}
                  title={person.summary}
                >
                  {person.title}
                  <small>
                    {person.start_year === null
                      ? ""
                      : `${compactYear(person.start_year)}—${person.end_year === null ? "" : compactYear(person.end_year)}`}
                  </small>
                </button>
              ))}
            </div>
          </div>
        ) : null}

        <div className="history-hero-tags">
          {era.tags.map((tag) => (
            <span key={tag}>{tag}</span>
          ))}
        </div>

        {onStartStory ? (
          <footer className="history-hero-actions">
            <button
              type="button"
              className="history-hero-story"
              onClick={onStartStory}
            >
              <CompassRose size={15} weight="duotone" />
              从一条学习路线开始探索
            </button>
          </footer>
        ) : null}
      </div>
    </section>
  );
}
