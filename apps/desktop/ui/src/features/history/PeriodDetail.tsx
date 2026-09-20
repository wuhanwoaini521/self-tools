/**
 * Period Detail —— 时代详情页的信息架构重构（2026-09）。
 *
 * 阅读顺序：上下文（前后时代）→ 时期标题/简介 → 历史阶段 →
 * 关键转折 → 核心人物 → 政权与时间尺度 → 完整时间线（章节 + 主线关系）。
 * 全部内容来自真实数据；阶段由后端 data-driven 分段（service.rs derive_stages）。
 */
import {
  ArrowDown,
  ArrowRight,
  CaretRight,
  UsersThree,
} from "@phosphor-icons/react";
import { useEffect, useMemo, useRef, useState, type RefObject } from "react";
import type {
  SemanticEventRelation,
  SemanticPeriod,
  SemanticPeriodDetail,
  SemanticPeriodEvent,
  SemanticPeriodPerson,
  SemanticPeriodStage,
  SemanticRegime,
} from "./semanticTypes";
import {
  chainVerb,
  eraNeighbors,
  eventTypeLabel,
  eventsForStage,
  primaryOutgoingRelation,
  rangeText,
  stageChain,
  stageLandmarks,
  turningPointLinks,
  typeSummary,
  yearText,
} from "./periodDetailHelpers";

interface PeriodDetailProps {
  period: SemanticPeriod;
  allPeriods: SemanticPeriod[];
  detail: SemanticPeriodDetail;
  onSelectPeriod: (period: SemanticPeriod) => void;
  onOpenEvent: (id: string) => void;
  onOpenPerson: (id: string) => void;
}

export function PeriodDetail({
  period,
  allPeriods,
  detail,
  onSelectPeriod,
  onOpenEvent,
  onOpenPerson,
}: PeriodDetailProps) {
  const { prev, next } = eraNeighbors(allPeriods, period);
  const [railOpen, setRailOpen] = useState(false);
  const [expandedPeople, setExpandedPeople] = useState(false);
  const [activeStage, setActiveStage] = useState<number | null>(null);
  const chapterRefs = useRef<Map<number, HTMLElement>>(new Map());
  const reduceMotion = usePrefersReducedMotion();

  // 切换时期后清理本页 UI 状态（IO 会在新章节上重新驱动 activeStage）。
  useEffect(() => {
    setActiveStage(null);
    setExpandedPeople(false);
  }, [period.id]);

  const events = detail.events;
  const criticalEvents = useMemo(
    () => events.filter((event) => event.importance === "critical"),
    [events],
  );
  const primaryByEvent = useMemo(() => {
    const map = new Map<
      string,
      {
        kind: "causal" | "sequential" | "associative";
        relation: SemanticEventRelation;
      }
    >();
    for (const event of events) {
      const primary = primaryOutgoingRelation(event.id, detail.relations);
      if (primary) map.set(event.id, primary);
    }
    return map;
  }, [events, detail.relations]);

  // 章节（阶段）↔ 时间线的单一事实源：IO 观察时间线章节标题，联动 spine 高亮。
  useEffect(() => {
    if (!detail.stages.length) return;
    const chapterNodes = chapterRefs.current;
    const observer = new IntersectionObserver(
      (entries) => {
        const visible = entries.filter(
          (entry) => entry.isIntersecting && entry.intersectionRatio > 0,
        );
        if (!visible.length) {
          setActiveStage(null);
          return;
        }
        // 视野帯内占用像素面积最大的章节为 active（章节高度差异大，
        // intersectionRatio 会偏向小元素；取 intersectionRect 实际可见面积）。
        const top = [...visible].sort(
          (left, right) =>
            right.intersectionRect.width * right.intersectionRect.height -
            left.intersectionRect.width * left.intersectionRect.height,
        )[0];
        const index = Number((top.target as HTMLElement).dataset.stageIndex);
        if (!Number.isNaN(index)) setActiveStage(index);
      },
      { rootMargin: "-12% 0px -62% 0px", threshold: [0, 0.05, 0.2] },
    );
    for (const node of chapterNodes.values()) observer.observe(node);
    return () => observer.disconnect();
  }, [detail.stages]);

  const scrollToChapter = (index: number) => {
    const node = chapterRefs.current.get(index);
    if (!node) return;
    node.scrollIntoView({
      behavior: reduceMotion ? "auto" : "smooth",
      block: "start",
    });
  };

  const earliest = useMemo(
    () =>
      [...events].sort(
        (a, b) => (a.start_year ?? Infinity) - (b.start_year ?? Infinity),
      )[0],
    [events],
  );
  const latest = useMemo(
    () =>
      [...events].sort(
        (a, b) => (b.start_year ?? -Infinity) - (a.start_year ?? -Infinity),
      )[0],
    [events],
  );

  return (
    <main className="history-v2-period">
      <EraNavigator
        period={period}
        allPeriods={allPeriods}
        prev={prev}
        next={next}
        railOpen={railOpen}
        onToggleRail={() => setRailOpen((value) => !value)}
        onSelectPeriod={onSelectPeriod}
      />
      <header className="history-v2-period-header">
        <div className="history-v2-period-title">
          <span className="history-v2-kicker">CHINA HISTORY · 时代详情</span>
          <h1>{period.name_zh_cn}</h1>
          <p className="history-v2-period-range">
            {rangeText(period.start_year, period.end_year)}
          </p>
        </div>
        <div className="history-v2-period-context">
          {period.description_zh_cn ? (
            <p className="history-v2-period-intro">
              {period.description_zh_cn}
            </p>
          ) : null}
          {earliest && latest && earliest.id !== latest.id ? (
            <p className="history-v2-period-bounds">
              收录 {events.length} 个事件，始于《{earliest.name_zh_cn}》（
              {yearText(earliest.start_year)}），终于《{latest.name_zh_cn}》（
              {yearText(latest.start_year)}）。
            </p>
          ) : null}
          <p className="history-v2-period-stats">
            {events.length} 个事件 · {detail.people.length} 位人物 ·{" "}
            {detail.regimes.length} 个政权
            {criticalEvents.length
              ? ` · ${criticalEvents.length} 个关键转折`
              : ""}
          </p>
        </div>
      </header>

      {detail.stages.length ? (
        <StageNavigator
          stages={detail.stages}
          events={events}
          activeStage={activeStage}
          onSelectStage={scrollToChapter}
        />
      ) : null}

      {criticalEvents.length ? (
        <TurningPoints
          events={criticalEvents}
          relations={detail.relations}
          onOpenEvent={onOpenEvent}
        />
      ) : null}

      {detail.people.length ? (
        <PeopleSection
          people={detail.people}
          expanded={expandedPeople}
          onToggle={() => setExpandedPeople((value) => !value)}
          onOpenPerson={onOpenPerson}
        />
      ) : null}

      {detail.regimes.length ? (
        <RegimeAxis period={period} regimes={detail.regimes} />
      ) : null}

      <ChapterTimeline
        period={period}
        stages={detail.stages}
        events={events}
        primaryByEvent={primaryByEvent}
        chapterRefs={chapterRefs}
        activeStage={activeStage}
        onSelectStage={scrollToChapter}
        onOpenEvent={onOpenEvent}
      />
    </main>
  );
}

function usePrefersReducedMotion(): boolean {
  const [reduced, setReduced] = useState(false);
  useEffect(() => {
    const query = window.matchMedia("(prefers-reduced-motion: reduce)");
    setReduced(query.matches);
    const listener = (event: MediaQueryListEvent) => setReduced(event.matches);
    query.addEventListener("change", listener);
    return () => query.removeEventListener("change", listener);
  }, []);
  return reduced;
}

// ---------- Era Navigator ----------

function EraNavigator({
  period,
  allPeriods,
  prev,
  next,
  railOpen,
  onToggleRail,
  onSelectPeriod,
}: {
  period: SemanticPeriod;
  allPeriods: SemanticPeriod[];
  prev: SemanticPeriod | null;
  next: SemanticPeriod | null;
  railOpen: boolean;
  onToggleRail: () => void;
  onSelectPeriod: (period: SemanticPeriod) => void;
}) {
  return (
    <section className="history-v2-era" aria-label="前后时代导航">
      <div className="history-v2-era-row">
        {prev ? (
          <button
            type="button"
            className="history-v2-era-neighbor is-prev"
            onClick={() => onSelectPeriod(prev)}
          >
            <span className="history-v2-era-direction">← 上一个时期</span>
            <strong>{prev.name_zh_cn}</strong>
            <small>{rangeText(prev.start_year, prev.end_year)}</small>
          </button>
        ) : (
          <span
            className="history-v2-era-neighbor is-empty"
            aria-hidden="true"
          />
        )}
        <div className="history-v2-era-current">
          <span className="history-v2-era-direction">当前时期</span>
          <strong>{period.name_zh_cn}</strong>
          <small>{rangeText(period.start_year, period.end_year)}</small>
        </div>
        {next ? (
          <button
            type="button"
            className="history-v2-era-neighbor is-next"
            onClick={() => onSelectPeriod(next)}
          >
            <span className="history-v2-era-direction">下一个时期 →</span>
            <strong>{next.name_zh_cn}</strong>
            <small>{rangeText(next.start_year, next.end_year)}</small>
          </button>
        ) : (
          <span
            className="history-v2-era-neighbor is-empty"
            aria-hidden="true"
          />
        )}
      </div>
      <button
        type="button"
        className={`history-v2-era-railtoggle${railOpen ? " is-open" : ""}`}
        onClick={onToggleRail}
        aria-expanded={railOpen}
      >
        {railOpen ? "收起全部时代" : `查看全部时代 · ${allPeriods.length}`}
        <CaretRight size={13} />
      </button>
      {railOpen ? (
        <EraRail
          periods={allPeriods}
          currentId={period.id}
          onSelect={onSelectPeriod}
        />
      ) : null}
    </section>
  );
}

function EraRail({
  periods,
  currentId,
  onSelect,
}: {
  periods: SemanticPeriod[];
  currentId: string;
  onSelect: (period: SemanticPeriod) => void;
}) {
  const railRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const node = railRef.current;
    if (!node) return;
    const current = node.querySelector<HTMLButtonElement>(
      `[data-period-id="${currentId}"]`,
    );
    current?.scrollIntoView({ block: "nearest", inline: "center" });
  }, [currentId]);
  return (
    <div
      ref={railRef}
      className="history-v2-era-rail"
      aria-label="全部历史时代"
    >
      {periods.map((item) => (
        <button
          type="button"
          key={item.id}
          data-period-id={item.id}
          className={`history-v2-era-chip${item.id === currentId ? " is-current" : ""}`}
          onClick={() => onSelect(item)}
        >
          <strong>{item.name_zh_cn}</strong>
          <small>{rangeText(item.start_year, item.end_year)}</small>
        </button>
      ))}
    </div>
  );
}

// ---------- Historical Stages（阶段导航，与 Timeline 章节同源） ----------

function StageNavigator({
  stages,
  events,
  activeStage,
  onSelectStage,
}: {
  stages: SemanticPeriodStage[];
  events: SemanticPeriodEvent[];
  activeStage: number | null;
  onSelectStage: (index: number) => void;
}) {
  return (
    <section
      className="history-v2-stages"
      aria-labelledby="history-stages-title"
    >
      <div className="history-v2-section-label">
        <span id="history-stages-title" className="history-v2-section-title">
          历史阶段
        </span>
        <small>{stages.length} 个阶段 · 从关键节点分段</small>
      </div>
      <ol className="history-v2-stage-spine">
        {stages.map((stage) => {
          const stageEvents = eventsForStage(events, stage);
          const chain = stageChain(stage, stages, events);
          const landmarks = stageLandmarks(stageEvents);
          const tags = typeSummary(stageEvents);
          const active = activeStage === stage.index;
          return (
            <li
              key={stage.index}
              className={`history-v2-stage-row${active ? " is-active" : ""}`}
            >
              <button
                type="button"
                className="history-v2-stage-head"
                onClick={() => onSelectStage(stage.index)}
                aria-current={active ? "true" : undefined}
              >
                <span className="history-v2-stage-marker" aria-hidden="true" />
                <span className="history-v2-stage-title">
                  <span className="history-v2-stage-meta">
                    <i>CHAPTER {String(stage.index).padStart(2, "0")}</i>
                    <b>
                      {stage.start_year} — {stage.end_year}
                    </b>
                  </span>
                  <strong>
                    {chain.from === "时期开端" ? "时期开端" : chain.from}
                  </strong>
                  <em>
                    {chain.to === "时期终点"
                      ? "至时期终点"
                      : `开创 ${chain.to}`}
                  </em>
                </span>
                <span className="history-v2-stage-side">
                  <span className="history-v2-stage-tags">
                    {tags || `${stage.event_count} 个事件`}
                  </span>
                  <span className="history-v2-stage-count">
                    {stage.event_count} 个事件
                  </span>
                </span>
              </button>
              <div className="history-v2-stage-body">
                {stage.opening_event_id ? (
                  <span
                    className="history-v2-stage-anchor"
                    title="本阶段开启的关键事件"
                  >
                    <i>{yearText(stage.start_year)}</i> 以{" "}
                    <b>{stage.opening_event_name}</b> 开启
                    {stage.opening_event_type ? (
                      <b className="is-type">
                        {eventTypeLabel(stage.opening_event_type)}
                      </b>
                    ) : null}
                  </span>
                ) : null}
                {landmarks.length ? (
                  <span className="history-v2-stage-landmarks">
                    {landmarks.join(" · ")}
                  </span>
                ) : null}
                <CaretRight size={14} className="history-v2-stage-arrow" />
              </div>
            </li>
          );
        })}
      </ol>
      <div className="history-v2-stage-hint">
        <ArrowDown size={13} /> 点击任一阶段，直达下方时间线对应章节
      </div>
    </section>
  );
}

// ---------- Key Turning Points（为什么重要 + 它连接到了什么） ----------

function TurningPoints({
  events,
  relations,
  onOpenEvent,
}: {
  events: SemanticPeriodEvent[];
  relations: SemanticEventRelation[];
  onOpenEvent: (id: string) => void;
}) {
  return (
    <section
      className="history-v2-turning"
      aria-labelledby="history-turning-title"
    >
      <div className="history-v2-section-label">
        <span id="history-turning-title" className="history-v2-section-title">
          关键转折
        </span>
        <small>{events.length} 个改变历史方向的关键节点</small>
      </div>
      <ol className="history-v2-turning-list">
        {events.map((event) => {
          const links = turningPointLinks(event.id, relations);
          return (
            <li key={event.id} className="history-v2-turning-row">
              <button
                type="button"
                className="history-v2-turning-main"
                onClick={() => onOpenEvent(event.id)}
              >
                <span className="history-v2-turning-year">
                  {yearText(event.start_year)}
                </span>
                <span className="history-v2-turning-marker" aria-hidden="true">
                  ◆
                </span>
                <span className="history-v2-turning-copy">
                  <span className="history-v2-turning-name">
                    {event.name_zh_cn}
                    <i>{eventTypeLabel(event.event_type)}</i>
                  </span>
                  <span className="history-v2-turning-summary">
                    {event.summary_zh_cn ||
                      event.result_zh_cn ||
                      "事件叙述正在整理中。"}
                  </span>
                </span>
                <CaretRight size={15} className="history-v2-turning-arrow" />
              </button>
              {links.length ? (
                <div className="history-v2-turning-links">
                  {links.map((link) => {
                    const name = link.target_event_name || "关联事件";
                    return (
                      <span
                        key={`${link.source_event_id}-${link.target_event_id}-${link.relation_type}`}
                      >
                        <em>↓ {chainVerb(link.relation_type)}</em>
                        <button
                          type="button"
                          onClick={() => onOpenEvent(link.target_event_id)}
                        >
                          {name}
                          <CaretRight size={12} />
                        </button>
                      </span>
                    );
                  })}
                </div>
              ) : null}
            </li>
          );
        })}
      </ol>
    </section>
  );
}

// ---------- Key People（分层：策展人物优先，计数只作弱 metadata） ----------

const PEOPLE_PRIMARY = 8;

function PeopleSection({
  people,
  expanded,
  onToggle,
  onOpenPerson,
}: {
  people: SemanticPeriodPerson[];
  expanded: boolean;
  onToggle: () => void;
  onOpenPerson: (id: string) => void;
}) {
  const ranked = useMemo(() => {
    const curated = people.filter((person) =>
      person.person_id.startsWith("curated-person-"),
    );
    const imported = people.filter(
      (person) => !person.person_id.startsWith("curated-person-"),
    );
    const sort = (list: SemanticPeriodPerson[]) =>
      [...list].sort(
        (a, b) =>
          b.event_count - a.event_count ||
          a.canonical_name_zh_cn.localeCompare(b.canonical_name_zh_cn, "zh-CN"),
      );
    return [...sort(curated), ...sort(imported)];
  }, [people]);
  const primary = ranked.slice(0, PEOPLE_PRIMARY);
  const more = ranked.slice(PEOPLE_PRIMARY);
  const visible = expanded ? ranked : primary;
  return (
    <section
      className="history-v2-people"
      aria-labelledby="history-people-title"
    >
      <div className="history-v2-section-label">
        <span id="history-people-title" className="history-v2-section-title">
          核心人物
        </span>
        <small>{people.length} 位活跃人物 · 事件数仅作辅助参考</small>
      </div>
      <ol className={`history-v2-person-list${expanded ? " is-expanded" : ""}`}>
        {visible.map((person) => (
          <li key={person.person_id} className="history-v2-person-row">
            <button
              type="button"
              className="history-v2-person-main"
              onClick={() => onOpenPerson(person.person_id)}
            >
              <span className="history-v2-person-copy">
                <strong>{person.canonical_name_zh_cn}</strong>
                {person.intro_zh_cn ? <em>{person.intro_zh_cn}</em> : null}
                {(person.birth_year !== null &&
                  person.birth_year !== undefined) ||
                (person.death_year !== null &&
                  person.death_year !== undefined) ? (
                  <small>
                    {rangeText(person.birth_year, person.death_year)}
                  </small>
                ) : null}
              </span>
              <span className="history-v2-person-meta">
                <small>{person.event_count} 个关联事件</small>
                <UsersThree size={13} />
              </span>
            </button>
          </li>
        ))}
      </ol>
      {more.length ? (
        <button
          type="button"
          className="history-v2-people-more"
          onClick={onToggle}
          aria-expanded={expanded}
        >
          {expanded ? "收起人物列表" : `查看全部 ${people.length} 位人物 →`}
        </button>
      ) : null}
    </section>
  );
}

// ---------- 政权与政治实体（与时期共享时间尺度） ----------

function RegimeAxis({
  period,
  regimes,
}: {
  period: SemanticPeriod;
  regimes: SemanticRegime[];
}) {
  const start =
    period.start_year ??
    Math.min(...regimes.map((regime) => regime.start_year ?? 0), 0);
  const end =
    period.end_year ??
    Math.max(...regimes.map((regime) => regime.end_year ?? start), start);
  const span = Math.max(1, end - start);
  const pos = (year: number | null | undefined) => {
    if (year === null || year === undefined) return null;
    return Math.min(100, Math.max(0, ((year - start) / span) * 100));
  };
  const primary = regimes.find(
    (regime) =>
      regime.start_year === period.start_year &&
      regime.end_year === period.end_year,
  );
  const ticks = [0, 1, 2, 3, 4].map(
    (index) => start + Math.round((span * index) / 4),
  );
  const tickStyle = (year: number): React.CSSProperties =>
    ({
      "--axis-pos": `${((year - start) / span) * 100}%`,
    }) as React.CSSProperties;
  return (
    <section
      className="history-v2-regimes"
      aria-labelledby="history-regimes-title"
    >
      <div className="history-v2-section-label">
        <span id="history-regimes-title" className="history-v2-section-title">
          政权与政治实体
        </span>
        <small>{rangeText(start, end)} · 与时期共享时间尺度</small>
      </div>
      <div className="history-v2-regime-axis">
        <div className="history-v2-regime-row is-ticks">
          <span className="history-v2-regime-name" aria-hidden="true" />
          <span className="history-v2-regime-track">
            <span className="history-v2-regime-hairline" aria-hidden="true" />
            {ticks.map((year) => (
              <span
                key={year}
                className="history-v2-regime-tick"
                style={tickStyle(year)}
                aria-hidden="true"
              />
            ))}
            {ticks.map((year) => (
              <span
                key={`label-${year}`}
                className="history-v2-regime-tick-label"
                style={tickStyle(year)}
              >
                {year}
              </span>
            ))}
          </span>
          <span aria-hidden="true" />
        </div>
        <div className="history-v2-regime-rows">
          {regimes.map((regime) => {
            const left = pos(regime.start_year);
            const width =
              (pos(regime.end_year) ?? 0) - (pos(regime.start_year) ?? 0);
            const main = regime.id === primary?.id;
            const barStyle = (): React.CSSProperties =>
              ({
                "--axis-pos": `${left ?? 0}%`,
                "--axis-width": `${Math.max(width, 0.6)}%`,
              }) as React.CSSProperties;
            return (
              <div key={regime.id} className="history-v2-regime-row">
                <span className="history-v2-regime-name">
                  {regime.name_zh_cn}
                  <i>{main ? "主体政权" : "并存政权"}</i>
                </span>
                <span className="history-v2-regime-track">
                  <span
                    className="history-v2-regime-hairline"
                    aria-hidden="true"
                  />
                  {left === null ? null : (
                    <span
                      className={`history-v2-regime-bar${main ? " is-main" : ""}`}
                      style={barStyle()}
                    >
                      {main ? (
                        <span className="history-v2-regime-bar-label">
                          {rangeText(regime.start_year, regime.end_year)}
                        </span>
                      ) : null}
                    </span>
                  )}
                </span>
                <span className="history-v2-regime-dates">
                  {rangeText(regime.start_year, regime.end_year)}
                </span>
              </div>
            );
          })}
        </div>
      </div>
      <p className="history-v2-regime-notes">
        {regimes
          .map((regime) => regime.description_zh_cn)
          .filter(Boolean)
          .slice(0, 2)
          .join("；") || `${regimes.length} 个政治实体在同一时期并存。`}
      </p>
    </section>
  );
}

// ---------- Full Event Timeline（章节 = 阶段；每事件一条主线关系） ----------

function ChapterTimeline({
  period,
  stages,
  events,
  primaryByEvent,
  chapterRefs,
  activeStage,
  onSelectStage,
  onOpenEvent,
}: {
  period: SemanticPeriod;
  stages: SemanticPeriodStage[];
  events: SemanticPeriodEvent[];
  primaryByEvent: Map<
    string,
    {
      kind: "causal" | "sequential" | "associative";
      relation: SemanticEventRelation;
    }
  >;
  chapterRefs: RefObject<Map<number, HTMLElement>>;
  activeStage: number | null;
  onSelectStage: (index: number) => void;
  onOpenEvent: (id: string) => void;
}) {
  const setChapterRef = (index: number) => (node: HTMLElement | null) => {
    if (node) chapterRefs.current.set(index, node);
    else chapterRefs.current.delete(index);
  };
  const chapters = stages.length
    ? stages
    : [
        {
          index: 0,
          start_year: period.start_year ?? 0,
          end_year: period.end_year ?? period.start_year ?? 0,
          opening_event_id: "",
          opening_event_name: "",
          opening_event_type: null,
          event_count: events.length,
        },
      ];
  const grouped: {
    stage: (typeof chapters)[number];
    stageEvents: SemanticPeriodEvent[];
  }[] = chapters.map((stage) =>
    stages.length
      ? { stage, stageEvents: eventsForStage(events, stage) }
      : { stage, stageEvents: events },
  );
  const activeChapter =
    activeStage !== null && stages.length ? activeStage : null;
  const activeChapterNode =
    activeChapter === null
      ? undefined
      : chapters.find((candidate) => candidate.index === activeChapter);

  const renderedCount = grouped.reduce(
    (total, group) => total + group.stageEvents.length,
    0,
  );

  return (
    <section
      className="history-v2-timeline"
      aria-labelledby="history-timeline-title"
    >
      <div className="history-v2-section-label history-v2-timeline-label">
        <span id="history-timeline-title" className="history-v2-section-title">
          完整时间线
        </span>
        <small>{renderedCount} 个事件 · 按时间推进 · 点击进入详情</small>
      </div>
      {activeChapterNode ? (
        <button
          type="button"
          className="history-v2-stage-indicator"
          onClick={() => onSelectStage(activeChapter!)}
        >
          <span
            className="history-v2-stage-indicator-marker"
            aria-hidden="true"
          />
          <span>
            第 {activeChapter} 阶段 · {activeChapterNode.start_year}—
            {activeChapterNode.end_year}
          </span>
          <span className="history-v2-stage-indicator-name">
            {activeChapterNode.opening_event_name || "时间线"}
          </span>
        </button>
      ) : null}
      <div className="history-v2-timeline-body">
        {grouped.map(({ stage, stageEvents }) => {
          if (!stageEvents.length) return null;
          const chain = stages.length
            ? stageChain(stage, stages, events)
            : null;
          return (
            <article
              key={stage.index}
              className="history-v2-chapter"
              data-stage-index={stage.index}
              ref={setChapterRef(stage.index)}
            >
              <header className="history-v2-chapter-head">
                <span className="history-v2-chapter-mark" aria-hidden="true" />
                <span className="history-v2-chapter-meta">
                  {stages.length ? (
                    <b>CHAPTER {String(stage.index).padStart(2, "0")}</b>
                  ) : (
                    <b>全部事件</b>
                  )}
                  <i>
                    {stage.start_year} — {stage.end_year} · {stageEvents.length}{" "}
                    个事件
                  </i>
                </span>
                {chain ? (
                  <span className="history-v2-chapter-chain">
                    《{chain.from}》<ArrowRight size={13} />《{chain.to}》
                  </span>
                ) : null}
              </header>
              <ol className="history-v2-chapter-events">
                {stageEvents.map((event, position) => {
                  const primary = primaryByEvent.get(event.id);
                  const showYear =
                    position === 0 ||
                    stageEvents[position - 1].start_year !== event.start_year;
                  return (
                    <li
                      key={event.id}
                      className={`history-v2-tl-row${event.importance === "critical" ? " is-critical" : ""}`}
                    >
                      <span className="history-v2-tl-year">
                        {showYear ? yearText(event.start_year) : ""}
                      </span>
                      <span className="history-v2-tl-rail" aria-hidden="true">
                        <i className="history-v2-tl-dot" />
                        {position < stageEvents.length - 1 ? (
                          <em className="history-v2-tl-line" />
                        ) : null}
                      </span>
                      <button
                        type="button"
                        className="history-v2-tl-main"
                        onClick={() => onOpenEvent(event.id)}
                      >
                        <span className="history-v2-tl-name">
                          {event.name_zh_cn}
                          {event.importance === "critical" ? (
                            <i className="is-critical">关键转折</i>
                          ) : null}
                          {event.importance === "major" ? (
                            <i className="is-major">
                              {eventTypeLabel(event.event_type)}
                            </i>
                          ) : null}
                        </span>
                        <span className="history-v2-tl-summary">
                          {event.summary_zh_cn ||
                            event.result_zh_cn ||
                            "事件叙述正在整理中。"}
                        </span>
                        <span className="history-v2-tl-meta">
                          {event.people_count ? (
                            <span>{event.people_count} 位人物</span>
                          ) : null}
                          {event.evidence_count ? (
                            <span>{event.evidence_count} 条证据</span>
                          ) : null}
                        </span>
                      </button>
                      {primary && primary.kind === "causal" ? (
                        <span className="history-v2-tl-relation">
                          <em>↓ {chainVerb(primary.relation.relation_type)}</em>
                          <button
                            type="button"
                            onClick={() =>
                              onOpenEvent(primary.relation.target_event_id)
                            }
                          >
                            {primary.relation.target_event_name || "关联事件"}
                          </button>
                        </span>
                      ) : null}
                    </li>
                  );
                })}
              </ol>
            </article>
          );
        })}
      </div>
    </section>
  );
}
