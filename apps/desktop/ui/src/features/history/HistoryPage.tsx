import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  ArrowRight,
  BookmarkSimple,
  CaretDown,
  CaretLeft,
  CaretUp,
  Compass,
  MagnifyingGlass,
} from "@phosphor-icons/react";
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import tangPalaceImage from "../../assets/history-tang-palace.png";
import type {
  HistoryDetail,
  HistoryDetailView,
  HistoryHome,
  HistoryNode,
  HistoryNodeKind,
  HistoryRelationKind,
  HistorySearchGroup,
} from "../../types";
import { errorMessage, isTauriRuntime } from "../../utils";
import {
  DEFAULT_ERA_INDEX,
  ERA_ICONS,
  clampEraIndex,
  historyEras,
  type HistoryEra,
} from "./historyEras";

const KIND_LABELS: Record<HistoryNodeKind, string> = {
  person: "人物",
  event: "事件",
  dynasty: "朝代",
  place: "地点",
  war: "战争",
  institution: "制度",
  artifact: "文物",
  culture: "文化",
};
const RELATION_LABELS: Record<HistoryRelationKind, string> = {
  occurred_in: "发生于",
  participated_in: "参与",
  belongs_to: "属于",
  cause: "前因",
  consequence: "后果",
  related_to: "相关",
  family: "亲属",
  political_ally: "政治盟友",
  political_opponent: "政治对手",
  monarch_minister: "君臣",
  military_opponent: "军事对手",
  predecessor: "前任",
  successor: "继任",
};

function year(value: number | null) {
  if (value === null) return "时间待考";
  return value < 0 ? `公元前 ${Math.abs(value)} 年` : `${value} 年`;
}
function range(start: number | null, end: number | null) {
  return end === null || end === start
    ? year(start)
    : `${year(start)} · ${year(end)}`;
}
function durationOf(era: HistoryEra) {
  return Math.max(1, era.endYear - era.startYear);
}

const WHEEL_ITEM_H = 88;
const WHEEL_H = 440;
const SWITCH_LOCK_MS = 380;

function wheelOffset(index: number) {
  return (WHEEL_H - WHEEL_ITEM_H) / 2 - index * WHEEL_ITEM_H;
}

/** 左侧：滚轮式历史时间选择器。 */
function EraWheel({
  active,
  currentIndex,
  onSelect,
}: {
  active: boolean;
  currentIndex: number;
  onSelect: (index: number) => void;
}) {
  const wheelRef = useRef<HTMLFieldSetElement>(null);
  const lockRef = useRef(0);

  useEffect(() => {
    const el = wheelRef.current;
    if (!el) return;
    const onWheel = (event: WheelEvent) => {
      if (!active) return;
      event.preventDefault();
      const now = Date.now();
      if (now - lockRef.current < SWITCH_LOCK_MS) return;
      lockRef.current = now;
      onSelect(clampEraIndex(currentIndex + (event.deltaY > 0 ? 1 : -1)));
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [active, currentIndex, onSelect]);

  return (
    <fieldset
      className="history-wheel"
      ref={wheelRef}
      aria-label="历史时代滚轮"
    >
      <button
        type="button"
        className="history-wheel-step up"
        aria-label="上一个时代"
        onClick={() => onSelect(clampEraIndex(currentIndex - 1))}
      >
        <CaretUp size={18} />
      </button>
      <div className="history-wheel-viewport">
        <div
          className="history-wheel-track"
          style={{ transform: `translateY(${wheelOffset(currentIndex)}px)` }}
        >
          {historyEras.map((era, index) => {
            const dist = Math.abs(index - currentIndex);
            const dim = Math.min(dist, 3);
            const opacity = [1, 0.72, 0.48, 0.16][dim];
            const scale = [1, 0.84, 0.7, 0.58][dim];
            return (
              <div
                key={era.id}
                className={`history-wheel-item${dist === 0 ? " is-current" : ""}`}
                style={{
                  height: WHEEL_ITEM_H,
                  opacity,
                  transform: `scale(${scale})`,
                }}
                aria-hidden={dist !== 0}
              >
                <span className="history-wheel-name">{era.name}</span>
                <span className="history-wheel-years">{era.yearLabel}</span>
              </div>
            );
          })}
        </div>
        <div className="history-wheel-window" aria-hidden="true">
          <i className="history-wheel-notch top-left" />
          <i className="history-wheel-notch top-right" />
          <i className="history-wheel-notch bottom-left" />
          <i className="history-wheel-notch bottom-right" />
        </div>
        <div className="history-wheel-rail" aria-hidden="true" />
      </div>
      <button
        type="button"
        className="history-wheel-step down"
        aria-label="下一个时代"
        onClick={() => onSelect(clampEraIndex(currentIndex + 1))}
      >
        <CaretDown size={18} />
      </button>
    </fieldset>
  );
}

/** 右侧：当前时代详情。 */
function EraPanel({
  era,
  onOpen,
}: {
  era: HistoryEra;
  onOpen: (era: HistoryEra) => void;
}) {
  const hasInk = era.id === "tang";
  return (
    <article className="history-era-panel">
      {hasInk ? (
        <img
          className="history-era-panel-ink"
          src={tangPalaceImage}
          alt=""
          aria-hidden="true"
        />
      ) : null}
      <div className="history-era-panel-head">
        <div>
          <span className="history-era-kicker">当前时代</span>
          <h2>{era.name}</h2>
          <p className="history-era-range">
            {era.yearLabel} · 共 {durationOf(era)} 年
          </p>
        </div>
        <button
          type="button"
          className="history-era-open"
          onClick={() => onOpen(era)}
        >
          展开详情 <ArrowRight size={15} />
        </button>
      </div>
      <div className="history-era-tags">
        {era.tags.map((tag) => (
          <span key={tag}>{tag}</span>
        ))}
      </div>
      <p className="history-era-overview">{era.description}</p>
      <div className="history-era-facts">
        {era.facts.map((fact) => {
          const Icon = ERA_ICONS[fact.icon];
          return (
            <div key={fact.label}>
              <Icon size={22} weight="duotone" />
              <p>
                <small>{fact.label}</small>
                <b>{fact.value}</b>
                <span>{fact.caption}</span>
              </p>
            </div>
          );
        })}
      </div>
      <section className="history-era-highlights">
        <header>
          <b>精彩速览</b>
          <span>事件 · 人物 · 文化</span>
        </header>
        <div>
          {era.highlights.map((highlight) => (
            <button
              type="button"
              key={highlight.eyebrow}
              onClick={() => onOpen(era)}
            >
              <small>{highlight.eyebrow}</small>
              <b>{highlight.title}</b>
              <span>{highlight.description}</span>
            </button>
          ))}
        </div>
      </section>
    </article>
  );
}

/** 底部：中国历史总时间轴。 */
function EraRail({
  currentIndex,
  onSelect,
}: {
  currentIndex: number;
  onSelect: (index: number) => void;
}) {
  return (
    <nav className="history-era-rail" aria-label="中国历史总时间轴">
      <span className="history-era-rail-label">中国历史总时间轴</span>
      <div className="history-era-rail-track">
        {historyEras.map((era, index) => (
          <button
            type="button"
            key={era.id}
            className={index === currentIndex ? "selected" : ""}
            onClick={() => onSelect(index)}
            aria-current={index === currentIndex ? "true" : undefined}
          >
            <i aria-hidden="true" />
            <b>{era.name}</b>
            <small>{era.yearLabel}</small>
          </button>
        ))}
      </div>
    </nav>
  );
}

function NodeChip({
  node,
  onOpen,
}: {
  node: HistoryNode;
  onOpen: (id: string) => void;
}) {
  return (
    <button
      type="button"
      className="history-node-chip"
      onClick={() => onOpen(node.id)}
    >
      <small>{KIND_LABELS[node.kind]}</small>
      <b>{node.title}</b>
      <span>{node.summary}</span>
    </button>
  );
}

function eraForNode(node: HistoryNode): HistoryEra | null {
  return (
    historyEras.find(
      (era) => era.id === node.id || era.id === node.period_id,
    ) ?? null
  );
}

/** 王朝详情：复用首页时代资料，形成真正可阅读的时代导读。 */
function EraStudyGuide({ era }: { era: HistoryEra }) {
  return (
    <section className="history-study-guide">
      <div className="history-study-lead">
        <div>
          <small>时代导读</small>
          <h2>先建立一张 {era.name} 的时代地图</h2>
        </div>
        <p>{era.description}</p>
      </div>
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
            <article key={highlight.eyebrow}>
              <small>{highlight.eyebrow}</small>
              <b>{highlight.title}</b>
              <p>{highlight.description}</p>
            </article>
          ))}
        </div>
      </section>
      <div className="history-study-tags">
        <span>本页关键词</span>
        {era.tags.map((tag) => (
          <b key={tag}>{tag}</b>
        ))}
      </div>
    </section>
  );
}

function EraContext({ era }: { era: HistoryEra }) {
  return (
    <aside className="history-era-context">
      <div>
        <small>所属时代</small>
        <b>{era.name}</b>
        <span>{era.yearLabel}</span>
      </div>
      <p>{era.description}</p>
      <div className="history-era-context-tags">
        {era.tags.map((tag) => (
          <span key={tag}>{tag}</span>
        ))}
      </div>
    </aside>
  );
}

function DetailSections({ detail }: { detail: HistoryDetail }) {
  if (detail.detail_type === "event")
    return (
      <>
        <HistoryList title="发生了什么" items={[detail.overview]} />
        <HistoryList title="为什么发生" items={detail.background} />
        {detail.trigger ? (
          <HistoryList title="导火索" items={[detail.trigger]} />
        ) : null}
        <HistoryList title="过程" items={detail.course} />
        <HistoryList title="结果" items={detail.results} />
        <HistoryList title="长期影响" items={detail.impacts} />
        <HistoryList title="不同解释" items={detail.debates} muted />
      </>
    );
  if (detail.detail_type === "person")
    return (
      <>
        <HistoryList title="身份" items={detail.identities} />
        <HistoryList title="人生脉络" items={detail.biography} />
        <HistoryList title="成就与影响" items={detail.achievements} />
        <HistoryList title="争议与评价" items={detail.controversies} muted />
      </>
    );
  if (detail.detail_type === "dynasty")
    return (
      <>
        <HistoryList title="王朝概览" items={[detail.overview]} />
        {detail.sections.map((section) => (
          <HistoryList
            key={section.title}
            title={section.title}
            items={section.items}
          />
        ))}
        <HistoryList title="政权性质" items={[detail.regime_type]} />
        {detail.capital ? (
          <HistoryList title="都城" items={[detail.capital]} />
        ) : null}
      </>
    );
  if (detail.detail_type === "place")
    return (
      <>
        <HistoryList title="地点概览" items={[detail.overview]} />
        {detail.modern_name ? (
          <HistoryList title="现代名称" items={[detail.modern_name]} />
        ) : null}
        {detail.historical_names.length ? (
          <HistoryList title="历史名称" items={detail.historical_names} />
        ) : null}
      </>
    );
  if (detail.detail_type === "institution")
    return (
      <>
        <HistoryList title="制度概览" items={[detail.overview]} />
        <HistoryList title="理解要点" items={detail.key_points} />
      </>
    );
  if (detail.detail_type === "artifact")
    return (
      <>
        <HistoryList title="文物概览" items={[detail.overview]} />
        {detail.collection ? (
          <HistoryList title="收藏单位" items={[detail.collection]} />
        ) : null}
      </>
    );
  return (
    <>
      <HistoryList title="概览" items={[detail.overview]} />
      <HistoryList title="要点" items={detail.key_points} />
    </>
  );
}

function HistoryList({
  title,
  items,
  muted = false,
}: {
  title: string;
  items: string[];
  muted?: boolean;
}) {
  if (!items.length) return null;
  return (
    <section className={`history-detail-section${muted ? " subdued" : ""}`}>
      <h2>{title}</h2>
      <ul>
        {items.map((item, index) => (
          <li key={index}>{item}</li>
        ))}
      </ul>
    </section>
  );
}

interface HistoryPageProps {
  active: boolean;
  setNotice: (message: string) => void;
  intent?: { id: string; nonce: number } | null;
}

export function HistoryPage({ active, setNotice, intent }: HistoryPageProps) {
  const [home, setHome] = useState<HistoryHome | null>(null);
  const [query, setQuery] = useState("");
  const [groups, setGroups] = useState<HistorySearchGroup[]>([]);
  const [detail, setDetail] = useState<HistoryDetailView | null>(null);
  const [loading, setLoading] = useState(false);
  const [currentEraIndex, setCurrentEraIndex] = useState(DEFAULT_ERA_INDEX);

  const currentEra = historyEras[currentEraIndex] ?? historyEras[0];

  const loadHome = useCallback(async () => {
    if (!isTauriRuntime()) return;
    setLoading(true);
    try {
      setHome(await invoke<HistoryHome>("history_home", { cursor: 0 }));
    } catch (error) {
      setNotice(errorMessage(error));
    } finally {
      setLoading(false);
    }
  }, [setNotice]);

  useEffect(() => {
    if (active && !home) void loadHome();
  }, [active, home, loadHome]);

  const openDetail = useCallback(
    async (id: string) => {
      if (!isTauriRuntime()) return;
      setLoading(true);
      try {
        const value = await invoke<HistoryDetailView | null>("history_detail", {
          id,
        });
        if (value) {
          setDetail(value);
          setGroups([]);
          setQuery("");
        }
      } catch (error) {
        setNotice(errorMessage(error));
      } finally {
        setLoading(false);
      }
    },
    [setNotice],
  );

  useEffect(() => {
    if (active && intent?.id) void openDetail(intent.id);
  }, [active, intent, openDetail]);

  const searchFor = useCallback(
    async (value: string) => {
      const normalized = value.trim();
      if (!normalized || !isTauriRuntime()) {
        setGroups([]);
        return;
      }
      setLoading(true);
      try {
        setGroups(
          await invoke<HistorySearchGroup[]>("history_search", {
            query: normalized,
          }),
        );
        setDetail(null);
      } catch (error) {
        setNotice(errorMessage(error));
      } finally {
        setLoading(false);
      }
    },
    [setNotice],
  );

  const search = useCallback(async () => {
    await searchFor(query);
  }, [query, searchFor]);

  const openEraDetail = useCallback(
    async (era: HistoryEra) => {
      if (!isTauriRuntime()) return;
      setLoading(true);
      try {
        const value = await invoke<HistoryDetailView | null>("history_detail", {
          id: era.id,
        });
        if (value) {
          setDetail(value);
          setGroups([]);
          setQuery("");
        } else {
          setQuery(era.name);
          await searchFor(era.name);
        }
      } catch (error) {
        setNotice(errorMessage(error));
      } finally {
        setLoading(false);
      }
    },
    [searchFor, setNotice],
  );

  const toggleFavorite = useCallback(async () => {
    if (!detail || !isTauriRuntime()) return;
    try {
      const saved = await invoke<boolean>("history_toggle_favorite", {
        id: detail.document.node.id,
      });
      setHome((current) =>
        current
          ? {
              ...current,
              favorite_ids: saved
                ? [
                    ...new Set([
                      ...current.favorite_ids,
                      detail.document.node.id,
                    ]),
                  ]
                : current.favorite_ids.filter(
                    (id) => id !== detail.document.node.id,
                  ),
            }
          : current,
      );
    } catch (error) {
      setNotice(errorMessage(error));
    }
  }, [detail, setNotice]);

  const selectEra = useCallback(
    (index: number) => setCurrentEraIndex(clampEraIndex(index)),
    [],
  );
  const isHome = Boolean(home) && !detail && !groups.length;

  // 键盘 ↑ ↓ 切换时代（仅在历史首页且焦点不在输入框时）。
  useEffect(() => {
    if (!active || !isHome) return;
    const onKey = (event: KeyboardEvent) => {
      const el = document.activeElement;
      if (
        el &&
        (el.tagName === "INPUT" ||
          el.tagName === "TEXTAREA" ||
          el.tagName === "SELECT")
      )
        return;
      if (event.key === "ArrowUp") {
        event.preventDefault();
        setCurrentEraIndex((index) => clampEraIndex(index - 1));
      } else if (event.key === "ArrowDown") {
        event.preventDefault();
        setCurrentEraIndex((index) => clampEraIndex(index + 1));
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [active, isHome]);

  if (!isTauriRuntime())
    return (
      <div className="history-page history-empty">
        <Compass size={28} />
        <h1>History Explorer</h1>
        <p>历史探索需要通过桌面应用启动，以访问内置的离线历史资料库。</p>
      </div>
    );
  const favorite = Boolean(
    detail && home?.favorite_ids.includes(detail.document.node.id),
  );
  const detailEra = detail ? eraForNode(detail.document.node) : null;
  const searchForm = (
    <form
      className="history-search"
      onSubmit={(event) => {
        event.preventDefault();
        void search();
      }}
    >
      <MagnifyingGlass size={20} />
      <input
        value={query}
        onChange={(event) => setQuery(event.target.value)}
        placeholder="搜索人物、事件、年代、地点……"
      />
      <button type="submit" disabled={loading || !query.trim()}>
        搜索
      </button>
    </form>
  );

  let content: ReactNode;
  if (detail) {
    content = (
      <article className="history-detail">
        <button
          type="button"
          className="history-back"
          onClick={() => setDetail(null)}
        >
          <CaretLeft size={16} />
          返回探索
        </button>
        <header>
          <div>
            <small>
              {KIND_LABELS[detail.document.node.kind]} ·{" "}
              {range(
                detail.document.node.start_year,
                detail.document.node.end_year,
              )}
            </small>
            <h1>{detail.document.node.title}</h1>
            <p>{detail.document.node.summary}</p>
          </div>
          <button
            type="button"
            className={`history-save${favorite ? " saved" : ""}`}
            onClick={() => void toggleFavorite()}
          >
            <BookmarkSimple size={18} weight={favorite ? "fill" : "regular"} />
            {favorite ? "已收藏" : "收藏"}
          </button>
        </header>
        {detailEra && detail.document.node.kind === "dynasty" ? (
          <EraStudyGuide era={detailEra} />
        ) : null}
        {detailEra && detail.document.node.kind !== "dynasty" ? (
          <EraContext era={detailEra} />
        ) : null}
        <DetailSections detail={detail.document.detail} />
        <section className="history-detail-section">
          <h2>继续探索</h2>
          <div className="history-relations">
            {detail.relations.map(({ relation, node }) => (
              <button
                type="button"
                key={`${relation.from_id}-${relation.to_id}-${relation.kind}`}
                onClick={() => void openDetail(node.id)}
              >
                <small>{RELATION_LABELS[relation.kind]}</small>
                <b>{node.title}</b>
                <span>{relation.note}</span>
                <ArrowRight size={15} />
              </button>
            ))}
          </div>
        </section>
        <section className="history-detail-section">
          <h2>资料来源</h2>
          <div className="history-sources">
            {detail.sources.map((source) => (
              <button
                type="button"
                key={source.id}
                onClick={() => {
                  if (isTauriRuntime()) void openUrl(source.url);
                }}
              >
                <b>{source.title}</b>
                <span>
                  {source.authority} · {source.source_type}
                </span>
                <ArrowRight size={14} />
              </button>
            ))}
          </div>
        </section>
      </article>
    );
  } else if (groups.length) {
    content = (
      <section className="history-results">
        <header>
          <p>搜索结果</p>
          <button
            type="button"
            onClick={() => {
              setGroups([]);
              setQuery("");
            }}
          >
            清除
          </button>
        </header>
        {groups.map((group) => (
          <section key={group.kind}>
            <h2>{KIND_LABELS[group.kind]}</h2>
            <div className="history-node-grid">
              {group.items.map((node) => (
                <NodeChip
                  key={node.id}
                  node={node}
                  onOpen={(id) => void openDetail(id)}
                />
              ))}
            </div>
          </section>
        ))}
      </section>
    );
  } else if (home) {
    content = (
      <div className="history-explorer">
        <div className="history-explorer-main">
          <EraWheel
            active={isHome}
            currentIndex={currentEraIndex}
            onSelect={selectEra}
          />
          <EraPanel
            era={currentEra}
            onOpen={(era) => void openEraDetail(era)}
          />
        </div>
        <EraRail currentIndex={currentEraIndex} onSelect={selectEra} />
        <p className="history-chronology-hint">
          滚动鼠标或使用 ↑ ↓ 切换时代 · 点击底部时间轴可跳转
        </p>
      </div>
    );
  } else {
    content = <div className="history-loading">正在准备离线历史资料…</div>;
  }

  return (
    <div className={`history-page${isHome ? " history-home" : ""}`}>
      {isHome ? (
        <div className="history-home-masthead">
          <header className="history-heading">
            <p>HISTORY EXPLORER</p>
            <h1>中国历史</h1>
            <span>滚动时光之轮，探索中华文明的演进脉络</span>
          </header>
          {searchForm}
        </div>
      ) : (
        <>
          <header className="history-heading">
            <p>HISTORY EXPLORER</p>
            <h1>探索中国历史</h1>
            <span>沿着时间、人物、事件与地点继续前行。</span>
          </header>
          {searchForm}
        </>
      )}
      {content}
    </div>
  );
}
