import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ArrowRight, BookmarkSimple, CaretLeft, Compass, MagnifyingGlass, MapPin, Sparkle } from "@phosphor-icons/react";
import { useCallback, useEffect, useState } from "react";
import type { HistoryDetail, HistoryDetailView, HistoryHome, HistoryNode, HistoryNodeKind, HistoryRelationKind, HistorySearchGroup } from "../../types";
import { errorMessage, isTauriRuntime } from "../../utils";

const KIND_LABELS: Record<HistoryNodeKind, string> = { person: "人物", event: "事件", dynasty: "朝代", place: "地点", war: "战争", institution: "制度", artifact: "文物", culture: "文化" };
const RELATION_LABELS: Record<HistoryRelationKind, string> = { occurred_in: "发生于", participated_in: "参与", belongs_to: "属于", cause: "前因", consequence: "后果", related_to: "相关", family: "亲属", political_ally: "政治盟友", political_opponent: "政治对手", monarch_minister: "君臣", military_opponent: "军事对手", predecessor: "前任", successor: "继任" };

function year(value: number | null) { if (value === null) return "时间待考"; return value < 0 ? `公元前 ${Math.abs(value)} 年` : `${value} 年`; }
function range(start: number | null, end: number | null) { return end === null || end === start ? year(start) : `${year(start)} · ${year(end)}`; }

const TIMELINE_START = -2100;
const TIMELINE_END = 2026;
const TIMELINE_CARD_WIDTH = 114;
const TIMELINE_PIXELS_PER_YEAR = 0.64;
const TIMELINE_HORIZONTAL_PADDING = 64;

function compactYear(value: number) { return value < 0 ? `前${Math.abs(value)}` : `${value}`; }

function arrangeTimeline(periods: HistoryHome["timeline"]) {
  const occupied: Record<"top" | "bottom", number[]> = { top: [-Infinity, -Infinity], bottom: [-Infinity, -Infinity] };
  return periods.map((period, index) => {
    const position = TIMELINE_HORIZONTAL_PADDING + (period.start_year - TIMELINE_START) * TIMELINE_PIXELS_PER_YEAR;
    const preferredSide: "top" | "bottom" = index % 2 === 0 ? "top" : "bottom";
    const sides: Array<"top" | "bottom"> = [preferredSide, preferredSide === "top" ? "bottom" : "top"];
    let side = preferredSide;
    let tier = 0;
    let placed = false;

    for (const candidate of sides) {
      const availableTier = occupied[candidate].findIndex((lastRight) => position - TIMELINE_CARD_WIDTH / 2 >= lastRight + 12);
      if (availableTier !== -1) {
        side = candidate;
        tier = availableTier;
        placed = true;
        break;
      }
    }
    if (!placed) {
      side = Math.min(...occupied.top) <= Math.min(...occupied.bottom) ? "top" : "bottom";
      tier = occupied[side][0] <= occupied[side][1] ? 0 : 1;
    }
    occupied[side][tier] = position + TIMELINE_CARD_WIDTH / 2;
    return { period, position, side, tier };
  });
}

function HistoryTimeline({ periods, onOpen }: { periods: HistoryHome["timeline"]; onOpen: (period: HistoryHome["timeline"][number]) => void }) {
  const width = Math.ceil((TIMELINE_END - TIMELINE_START) * TIMELINE_PIXELS_PER_YEAR + TIMELINE_HORIZONTAL_PADDING * 2);
  const ticks = [-2000, -1500, -1000, -500, 0, 500, 1000, 1500, 1900];
  const placements = arrangeTimeline(periods);
  return <section className="history-timeline">
    <div className="history-timeline-heading"><div><p>中国历史时间轴</p><h2>从夏商周到近现代</h2></div><span>按实际年代比例排列 · 点击朝代查看详情</span></div>
    <div className="history-timeline-scroll"><div className="history-timeline-canvas" style={{ width }}>
      <div className="history-timeline-axis" />
      {ticks.map((tick) => <span className="history-timeline-tick" key={tick} style={{ left: TIMELINE_HORIZONTAL_PADDING + (tick - TIMELINE_START) * TIMELINE_PIXELS_PER_YEAR }}>{year(tick)}</span>)}
      {placements.map(({ period, position, side, tier }) => <div className={`history-timeline-item is-${side} is-tier-${tier}`} key={period.id} style={{ left: position }}>
        <span className="history-timeline-link" aria-hidden="true" />
        <span className="history-timeline-marker" aria-hidden="true" />
        <button className="history-timeline-period" title={period.summary} onClick={() => onOpen(period)}>
          <b>{period.name}</b><small>{compactYear(period.start_year)}—{compactYear(period.end_year)}</small>
        </button>
      </div>)}
    </div></div>
  </section>;
}

function NodeChip({ node, onOpen }: { node: HistoryNode; onOpen: (id: string) => void }) {
  return <button className="history-node-chip" onClick={() => onOpen(node.id)}><small>{KIND_LABELS[node.kind]}</small><b>{node.title}</b><span>{node.summary}</span></button>;
}

function DetailSections({ detail }: { detail: HistoryDetail }) {
  if (detail.detail_type === "event") return <>
    <HistoryList title="发生了什么" items={[detail.overview]} />
    <HistoryList title="为什么发生" items={detail.background} />
    {detail.trigger ? <HistoryList title="导火索" items={[detail.trigger]} /> : null}
    <HistoryList title="过程" items={detail.course} />
    <HistoryList title="结果" items={detail.results} />
    <HistoryList title="长期影响" items={detail.impacts} />
    <HistoryList title="不同解释" items={detail.debates} muted />
  </>;
  if (detail.detail_type === "person") return <>
    <HistoryList title="身份" items={detail.identities} />
    <HistoryList title="人生脉络" items={detail.biography} />
    <HistoryList title="成就与影响" items={detail.achievements} />
    <HistoryList title="争议与评价" items={detail.controversies} muted />
  </>;
  if (detail.detail_type === "dynasty") return <><HistoryList title="王朝概览" items={[detail.overview]} />{detail.capital ? <HistoryList title="都城" items={[detail.capital]} /> : null}</>;
  if (detail.detail_type === "place") return <><HistoryList title="地点概览" items={[detail.overview]} />{detail.modern_name ? <HistoryList title="现代名称" items={[detail.modern_name]} /> : null}{detail.historical_names.length ? <HistoryList title="历史名称" items={detail.historical_names} /> : null}</>;
  if (detail.detail_type === "institution") return <><HistoryList title="制度概览" items={[detail.overview]} /><HistoryList title="理解要点" items={detail.key_points} /></>;
  if (detail.detail_type === "artifact") return <><HistoryList title="文物概览" items={[detail.overview]} />{detail.collection ? <HistoryList title="收藏单位" items={[detail.collection]} /> : null}</>;
  return <><HistoryList title="概览" items={[detail.overview]} /><HistoryList title="要点" items={detail.key_points} /></>;
}

function HistoryList({ title, items, muted = false }: { title: string; items: string[]; muted?: boolean }) {
  if (!items.length) return null;
  return <section className={"history-detail-section" + (muted ? " subdued" : "")}><h2>{title}</h2><ul>{items.map((item, index) => <li key={index}>{item}</li>)}</ul></section>;
}

interface HistoryPageProps { active: boolean; setNotice: (message: string) => void; }

export function HistoryPage({ active, setNotice }: HistoryPageProps) {
  const [home, setHome] = useState<HistoryHome | null>(null);
  const [query, setQuery] = useState("");
  const [groups, setGroups] = useState<HistorySearchGroup[]>([]);
  const [detail, setDetail] = useState<HistoryDetailView | null>(null);
  const [loading, setLoading] = useState(false);
  const [cursor, setCursor] = useState(0);

  const loadHome = useCallback(async (nextCursor = cursor) => {
    if (!isTauriRuntime()) return;
    setLoading(true);
    try { setHome(await invoke<HistoryHome>("history_home", { cursor: nextCursor })); } catch (error) { setNotice(errorMessage(error)); } finally { setLoading(false); }
  }, [cursor, setNotice]);

  useEffect(() => { if (active && !home) void loadHome(); }, [active, home, loadHome]);

  const openDetail = useCallback(async (id: string) => {
    if (!isTauriRuntime()) return;
    setLoading(true);
    try { const value = await invoke<HistoryDetailView | null>("history_detail", { id }); if (value) { setDetail(value); setGroups([]); setQuery(""); void loadHome(cursor + 1); } } catch (error) { setNotice(errorMessage(error)); } finally { setLoading(false); }
  }, [cursor, loadHome, setNotice]);

  const searchFor = useCallback(async (value: string) => {
    const normalized = value.trim();
    if (!normalized || !isTauriRuntime()) { setGroups([]); return; }
    setLoading(true);
    try { setGroups(await invoke<HistorySearchGroup[]>("history_search", { query: normalized })); setDetail(null); } catch (error) { setNotice(errorMessage(error)); } finally { setLoading(false); }
  }, [setNotice]);

  const search = useCallback(async () => { await searchFor(query); }, [query, searchFor]);

  const openPeriod = useCallback(async (period: HistoryHome["timeline"][number]) => {
    if (!isTauriRuntime()) return;
    setLoading(true);
    try {
      const value = await invoke<HistoryDetailView | null>("history_detail", { id: period.id });
      if (value) { setDetail(value); setGroups([]); setQuery(""); }
      else { setQuery(period.name); await searchFor(period.name); }
    } catch (error) { setNotice(errorMessage(error)); } finally { setLoading(false); }
  }, [searchFor, setNotice]);

  const toggleFavorite = useCallback(async () => {
    if (!detail || !isTauriRuntime()) return;
    try { const saved = await invoke<boolean>("history_toggle_favorite", { id: detail.document.node.id }); setHome((current) => current ? { ...current, favorite_ids: saved ? [...new Set([...current.favorite_ids, detail.document.node.id])] : current.favorite_ids.filter((id) => id !== detail.document.node.id) } : current); } catch (error) { setNotice(errorMessage(error)); }
  }, [detail, setNotice]);

  if (!isTauriRuntime()) return <div className="history-page history-empty"><Compass size={28} /><h1>History Explorer</h1><p>历史探索需要通过桌面应用启动，以访问内置的离线历史资料库。</p></div>;
  const favorite = Boolean(detail && home?.favorite_ids.includes(detail.document.node.id));
  return <div className="history-page">
    <header className="history-heading"><p>HISTORY EXPLORER</p><h1>探索中国历史</h1><span>沿着时间、人物、事件与地点继续前行。</span></header>
    <form className="history-search" onSubmit={(event) => { event.preventDefault(); void search(); }}><MagnifyingGlass size={20} /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索人物、事件、朝代、年份、地点……" /><button type="submit" disabled={loading || !query.trim()}>搜索</button></form>

    {detail ? <article className="history-detail">
      <button className="history-back" onClick={() => setDetail(null)}><CaretLeft size={16} />返回探索</button>
      <header><div><small>{KIND_LABELS[detail.document.node.kind]} · {range(detail.document.node.start_year, detail.document.node.end_year)}</small><h1>{detail.document.node.title}</h1><p>{detail.document.node.summary}</p></div><button className={"history-save" + (favorite ? " saved" : "")} onClick={() => void toggleFavorite()}><BookmarkSimple size={18} weight={favorite ? "fill" : "regular"} />{favorite ? "已收藏" : "收藏"}</button></header>
      <DetailSections detail={detail.document.detail} />
      <section className="history-detail-section"><h2>继续探索</h2><div className="history-relations">{detail.relations.map(({ relation, node }) => <button key={`${relation.from_id}-${relation.to_id}-${relation.kind}`} onClick={() => void openDetail(node.id)}><small>{RELATION_LABELS[relation.kind]}</small><b>{node.title}</b><span>{relation.note}</span><ArrowRight size={15} /></button>)}</div></section>
      <section className="history-detail-section"><h2>资料来源</h2><div className="history-sources">{detail.sources.map((source) => <button key={source.id} onClick={() => { if (isTauriRuntime()) void openUrl(source.url); }}><b>{source.title}</b><span>{source.authority} · {source.source_type}</span><ArrowRight size={14} /></button>)}</div></section>
    </article> : groups.length ? <section className="history-results"><header><p>搜索结果</p><button onClick={() => { setGroups([]); setQuery(""); }}>清除</button></header>{groups.map((group) => <section key={group.kind}><h2>{KIND_LABELS[group.kind]}</h2><div className="history-node-grid">{group.items.map((node) => <NodeChip key={node.id} node={node} onOpen={(id) => void openDetail(id)} />)}</div></section>)}</section> : home ? <>
      <HistoryTimeline periods={home.timeline} onOpen={(period) => void openPeriod(period)} />
      <section className="history-featured"><div className="history-featured-copy"><p><Sparkle size={15} />今日探索</p><small>{KIND_LABELS[home.recommendation.kind]} · {range(home.recommendation.start_year, home.recommendation.end_year)}</small><h2>{home.recommendation.title}</h2><span>{home.recommendation.summary}</span><div>{home.recommendation.tags.slice(0, 4).map((tag) => <i key={tag}>{tag}</i>)}</div><button onClick={() => void openDetail(home.recommendation.id)}>开始探索 <ArrowRight size={16} /></button></div><div className="history-featured-graphic"><span>TIME</span><b>{home.recommendation.start_year}</b><i /></div></section>
      <section className="history-discover"><header><div><p>随机发现</p><h2>换一个视角，继续探索</h2></div><button onClick={() => { const next = cursor + 1; setCursor(next); void loadHome(next); }}>换一批</button></header><div className="history-node-grid">{home.discoveries.map((node) => <NodeChip key={node.id} node={node} onOpen={(id) => void openDetail(id)} />)}</div></section>
      {home.recent.length ? <section className="history-recent"><p>最近浏览</p><div>{home.recent.map((node) => <button key={node.id} onClick={() => void openDetail(node.id)}><MapPin size={14} />{node.title}</button>)}</div></section> : null}
    </> : <div className="history-loading">正在准备离线历史资料…</div>}
  </div>;
}
