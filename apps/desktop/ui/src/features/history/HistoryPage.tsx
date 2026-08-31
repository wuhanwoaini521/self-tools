import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ArrowLeft, ArrowRight, BookmarkSimple, Buildings, CaretLeft, Compass, Crown, MagnifyingGlass, MapPin, Scroll, Sparkle } from "@phosphor-icons/react";
import { useCallback, useEffect, useState } from "react";
import chronologyDrumImage from "../../assets/history-chronology-drum.png";
import tangPalaceImage from "../../assets/history-tang-palace.png";
import type { HistoryDetail, HistoryDetailView, HistoryHome, HistoryNode, HistoryNodeKind, HistoryRelationKind, HistorySearchGroup } from "../../types";
import { errorMessage, isTauriRuntime } from "../../utils";

const KIND_LABELS: Record<HistoryNodeKind, string> = { person: "人物", event: "事件", dynasty: "朝代", place: "地点", war: "战争", institution: "制度", artifact: "文物", culture: "文化" };
const RELATION_LABELS: Record<HistoryRelationKind, string> = { occurred_in: "发生于", participated_in: "参与", belongs_to: "属于", cause: "前因", consequence: "后果", related_to: "相关", family: "亲属", political_ally: "政治盟友", political_opponent: "政治对手", monarch_minister: "君臣", military_opponent: "军事对手", predecessor: "前任", successor: "继任" };

function year(value: number | null) { if (value === null) return "时间待考"; return value < 0 ? `公元前 ${Math.abs(value)} 年` : `${value} 年`; }
function range(start: number | null, end: number | null) { return end === null || end === start ? year(start) : `${year(start)} · ${year(end)}`; }

type HistoryPeriod = HistoryHome["timeline"][number];

interface EraPreview {
  eyebrow: string;
  tags: string[];
  overview: string;
  facts: Array<{ label: string; value: string; caption: string; icon: typeof Buildings }>;
  highlights: Array<{ eyebrow: string; title: string; description: string }>;
}

const TANG_PREVIEW: EraPreview = {
  eyebrow: "盛世风华", tags: ["繁荣开放", "盛世气象", "兼容并包", "文化鼎盛"],
  overview: "唐朝是中国历史上最为辉煌的朝代之一。政治开明，经济繁荣，文化昌盛，对外交往频繁，丝绸之路畅通，长安成为当时世界上最国际化的大都市。",
  facts: [
    { label: "都城", value: "长安", caption: "今陕西西安", icon: MapPin },
    { label: "国力鼎盛", value: "贞观之治", caption: "开元盛世", icon: Crown },
    { label: "对外交流", value: "丝绸之路", caption: "通达亚欧", icon: Scroll },
    { label: "选官制度", value: "科举制", caption: "逐步完善", icon: Buildings },
  ],
  highlights: [
    { eyebrow: "重要事件", title: "安史之乱", description: "755 年爆发，盛唐由盛转衰的重要转折点。" },
    { eyebrow: "代表人物", title: "李白 · 杜甫", description: "诗仙与诗圣，共同写下唐诗最璀璨的篇章。" },
    { eyebrow: "文化成就", title: "唐诗繁荣", description: "诗歌、书法、绘画、音乐等全面发展。" },
  ],
};

function compactYear(value: number) { return value < 0 ? `前${Math.abs(value)}` : `${value}`; }
function eraRange(period: HistoryPeriod) { return `${compactYear(period.start_year)}—${compactYear(period.end_year)} 年`; }
function eraDuration(period: HistoryPeriod) { return Math.max(1, period.end_year - period.start_year); }

function previewFor(period: HistoryPeriod): EraPreview {
  if (period.id === "tang") return TANG_PREVIEW;
  return {
    eyebrow: "时代纵览", tags: ["制度演进", "社会变迁", "文化传承", "历史脉络"],
    overview: `${period.name}处在中国历史演进的重要阶段。沿着这一时期的制度、人物、事件与文化脉络，可以进一步理解它如何连接前代经验，并影响后世的发展。`,
    facts: [
      { label: "时代范围", value: eraRange(period), caption: "以本地资料库为准", icon: Scroll },
      { label: "延续时间", value: `${eraDuration(period)} 年`, caption: "历史跨度", icon: Crown },
      { label: "探索方式", value: "人物事件", caption: "多维资料关联", icon: MapPin },
      { label: "资料来源", value: "离线史料", caption: "点击查看详情", icon: Buildings },
    ],
    highlights: [
      { eyebrow: "时代起点", title: `走进${period.name}`, description: "从时代概览、关键人物和重要事件开始探索。" },
      { eyebrow: "历史线索", title: "制度与社会", description: "梳理这一阶段延续、变化与影响的线索。" },
      { eyebrow: "继续探索", title: "关联资料", description: "打开详情页，沿着资料库中的关联节点继续阅读。" },
    ],
  };
}

function HistoryTimeline({ periods, selectedId, onSelect, onOpen }: { periods: HistoryHome["timeline"]; selectedId: string; onSelect: (id: string) => void; onOpen: (period: HistoryPeriod) => void }) {
  const selected = periods.find((period) => period.id === selectedId) ?? periods.find((period) => period.id === "tang") ?? periods[0];
  if (!selected) return null;
  const preview = previewFor(selected);
  const activeIndex = Math.max(0, periods.findIndex((period) => period.id === selected.id));
  const chooseOffset = (offset: number) => onSelect(periods[(activeIndex + offset + periods.length) % periods.length].id);
  return <section className="history-timeline history-chronology">
    <div className="history-chronology-stage">
      <section className="history-chronology-drum" aria-label="时代切换器">
        <img src={chronologyDrumImage} alt="中国历史年代卷轴" />
        <div className="history-chronology-drum-copy"><span>当前时代</span><h2>{selected.name}</h2><p>{eraRange(selected)}</p><button type="button" onClick={() => onOpen(selected)}>查看时代详情 <ArrowRight size={15} /></button></div>
      </section>
      <article className="history-era-panel" style={{ backgroundImage: `url(${tangPalaceImage})` }}>
        <div className="history-era-panel-copy"><span className="history-era-kicker">{preview.eyebrow}</span><h2>{selected.name}</h2><p className="history-era-range">{eraRange(selected)} · 共 {eraDuration(selected)} 年</p><div className="history-era-tags">{preview.tags.map((tag) => <span key={tag}>{tag}</span>)}</div><p className="history-era-overview">{preview.overview}</p></div>
        <div className="history-era-facts">{preview.facts.map(({ label, value, caption, icon: Icon }) => <div key={label}><Icon size={23} weight="duotone" /><p><small>{label}</small><b>{value}</b><span>{caption}</span></p></div>)}</div>
        <section className="history-era-highlights"><header><b>精彩速览</b><button type="button" onClick={() => onOpen(selected)}>展开详情 <ArrowRight size={14} /></button></header><div>{preview.highlights.map((highlight, index) => <button type="button" key={highlight.title} className={index === 2 ? "with-ink" : ""} onClick={() => onOpen(selected)}><small>{highlight.eyebrow}</small><b>{highlight.title}</b><span>{highlight.description}</span></button>)}</div></section>
      </article>
    </div>
    <nav className="history-era-rail" aria-label="朝代时间轴"><button type="button" className="history-era-rail-arrow" onClick={() => chooseOffset(-1)} aria-label="上一个时代"><ArrowLeft size={17} /></button><div className="history-era-rail-track">{periods.map((period) => <button type="button" key={period.id} className={period.id === selected.id ? "selected" : ""} onClick={() => onSelect(period.id)} aria-current={period.id === selected.id ? "true" : undefined}><i aria-hidden="true" /><b>{period.name}</b><small>{compactYear(period.start_year)}—{compactYear(period.end_year)}</small></button>)}</div><button type="button" className="history-era-rail-arrow" onClick={() => chooseOffset(1)} aria-label="下一个时代"><ArrowRight size={17} /></button></nav>
    <p className="history-chronology-hint">使用时间轴左右切换时代，选择任一朝代可浏览其重点资料。</p>
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
  const [selectedPeriodId, setSelectedPeriodId] = useState("tang");

  const loadHome = useCallback(async (nextCursor = cursor) => {
    if (!isTauriRuntime()) return;
    setLoading(true);
    try { setHome(await invoke<HistoryHome>("history_home", { cursor: nextCursor })); } catch (error) { setNotice(errorMessage(error)); } finally { setLoading(false); }
  }, [cursor, setNotice]);

  useEffect(() => { if (active && !home) void loadHome(); }, [active, home, loadHome]);
  useEffect(() => {
    if (home && !home.timeline.some((period) => period.id === selectedPeriodId)) setSelectedPeriodId(home.timeline.find((period) => period.id === "tang")?.id ?? home.timeline[0]?.id ?? "");
  }, [home, selectedPeriodId]);

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
  const isHome = Boolean(home) && !detail && !groups.length;
  const searchForm = <form className="history-search" onSubmit={(event) => { event.preventDefault(); void search(); }}><MagnifyingGlass size={20} /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索人物、事件、年代、地点……" /><button type="submit" disabled={loading || !query.trim()}>搜索</button></form>;
  return <div className={"history-page" + (isHome ? " history-home" : "")}>
    {isHome ? <div className="history-home-masthead"><header className="history-heading"><p>HISTORY EXPLORER</p><h1>中国历史时间轴</h1><span>滚动时光之轮，探索中华文明的演进脉络</span></header>{searchForm}</div> : <><header className="history-heading"><p>HISTORY EXPLORER</p><h1>探索中国历史</h1><span>沿着时间、人物、事件与地点继续前行。</span></header>{searchForm}</>}

    {detail ? <article className="history-detail">
      <button className="history-back" onClick={() => setDetail(null)}><CaretLeft size={16} />返回探索</button>
      <header><div><small>{KIND_LABELS[detail.document.node.kind]} · {range(detail.document.node.start_year, detail.document.node.end_year)}</small><h1>{detail.document.node.title}</h1><p>{detail.document.node.summary}</p></div><button className={"history-save" + (favorite ? " saved" : "")} onClick={() => void toggleFavorite()}><BookmarkSimple size={18} weight={favorite ? "fill" : "regular"} />{favorite ? "已收藏" : "收藏"}</button></header>
      <DetailSections detail={detail.document.detail} />
      <section className="history-detail-section"><h2>继续探索</h2><div className="history-relations">{detail.relations.map(({ relation, node }) => <button key={`${relation.from_id}-${relation.to_id}-${relation.kind}`} onClick={() => void openDetail(node.id)}><small>{RELATION_LABELS[relation.kind]}</small><b>{node.title}</b><span>{relation.note}</span><ArrowRight size={15} /></button>)}</div></section>
      <section className="history-detail-section"><h2>资料来源</h2><div className="history-sources">{detail.sources.map((source) => <button key={source.id} onClick={() => { if (isTauriRuntime()) void openUrl(source.url); }}><b>{source.title}</b><span>{source.authority} · {source.source_type}</span><ArrowRight size={14} /></button>)}</div></section>
    </article> : groups.length ? <section className="history-results"><header><p>搜索结果</p><button onClick={() => { setGroups([]); setQuery(""); }}>清除</button></header>{groups.map((group) => <section key={group.kind}><h2>{KIND_LABELS[group.kind]}</h2><div className="history-node-grid">{group.items.map((node) => <NodeChip key={node.id} node={node} onOpen={(id) => void openDetail(id)} />)}</div></section>)}</section> : home ? <>
      <HistoryTimeline periods={home.timeline} selectedId={selectedPeriodId} onSelect={setSelectedPeriodId} onOpen={(period) => void openPeriod(period)} />
      <section className="history-featured"><div className="history-featured-copy"><p><Sparkle size={15} />今日探索</p><small>{KIND_LABELS[home.recommendation.kind]} · {range(home.recommendation.start_year, home.recommendation.end_year)}</small><h2>{home.recommendation.title}</h2><span>{home.recommendation.summary}</span><div>{home.recommendation.tags.slice(0, 4).map((tag) => <i key={tag}>{tag}</i>)}</div><button onClick={() => void openDetail(home.recommendation.id)}>开始探索 <ArrowRight size={16} /></button></div><div className="history-featured-graphic"><span>TIME</span><b>{home.recommendation.start_year}</b><i /></div></section>
      <section className="history-discover"><header><div><p>随机发现</p><h2>换一个视角，继续探索</h2></div><button onClick={() => { const next = cursor + 1; setCursor(next); void loadHome(next); }}>换一批</button></header><div className="history-node-grid">{home.discoveries.map((node) => <NodeChip key={node.id} node={node} onOpen={(id) => void openDetail(id)} />)}</div></section>
      {home.recent.length ? <section className="history-recent"><p>最近浏览</p><div>{home.recent.map((node) => <button key={node.id} onClick={() => void openDetail(node.id)}><MapPin size={14} />{node.title}</button>)}</div></section> : null}
    </> : <div className="history-loading">正在准备离线历史资料…</div>}
  </div>;
}
