import { invoke } from "@tauri-apps/api/core";
import { ArrowLeft, ArrowRight, BookOpen, CalendarBlank, CaretRight, CheckCircle, Clock, Funnel, MagnifyingGlass, MapPin, Scroll, SealCheck, UsersThree, WarningCircle, X } from "@phosphor-icons/react";
import { useCallback, useEffect, useRef, useState, type KeyboardEvent, type ReactNode, type RefObject } from "react";
import { errorMessage, isTauriRuntime } from "../../utils";
import type { SemanticEventDetail, SemanticEventEvidence, SemanticEventPlace, SemanticEventRelation, SemanticHistoricalText, SemanticHome, SemanticPeriod, SemanticPeriodDetail, SemanticPeriodEvent, SemanticPeriodPerson, SemanticPersonDetail, SemanticRegime, SemanticSearchGroup, SemanticSearchHit, SemanticSource, SemanticStory, SemanticStoryDetail, SemanticWorkDetail } from "./semanticTypes";
import type { HistoryGeoNavigationRequest } from "./types/history";

type DrawerState =
  | { kind: "event"; data: SemanticEventDetail }
  | { kind: "person"; data: SemanticPersonDetail }
  | { kind: "place"; data: SemanticEventPlace; eventName: string }
  | { kind: "text"; data: SemanticHistoricalText; eventName: string }
  | { kind: "work"; data: SemanticWorkDetail }
  | null;

const EMPTY_HOME: SemanticHome = { periods: [], stories: [], stats: null };

function normalizeHome(value: SemanticHome | null | undefined): SemanticHome {
  return {
    periods: Array.isArray(value?.periods) ? value.periods : [],
    stories: Array.isArray(value?.stories) ? value.stories : [],
    stats: value?.stats ?? null,
  };
}

function normalizePeriodDetail(value: SemanticPeriodDetail | null | undefined): SemanticPeriodDetail | null {
  if (!value?.period) return null;
  return {
    ...value,
    regimes: Array.isArray(value.regimes) ? value.regimes : [],
    stories: Array.isArray(value.stories) ? value.stories : [],
    events: Array.isArray(value.events) ? value.events : [],
    people: Array.isArray(value.people) ? value.people : [],
  };
}

function normalizeWorkDetail(value: SemanticWorkDetail | null | undefined): SemanticWorkDetail | null {
  if (!value?.work) return null;
  return {
    ...value,
    texts: Array.isArray(value.texts) ? value.texts : [],
    sources: Array.isArray(value.sources) ? value.sources : [],
  };
}

function yearText(value: number | null | undefined): string { if (value === null || value === undefined) return "年代待考"; return value < 0 ? `前${Math.abs(value)}` : `${value}`; }
function rangeText(start: number | null | undefined, end: number | null | undefined): string { if (start === null || start === undefined) return "年代待考"; if (end === null || end === undefined || start === end) return yearText(start); return `${yearText(start)} — ${yearText(end)}`; }
function kindLabel(kind: SemanticSearchHit["kind"]): string { return { person: "人物", story: "Story", event: "事件", work: "作品" }[kind]; }
function qualityLabel(value: string | null | undefined): string | null { if (!value || value === "verified" || value === "reviewed" || value === "curated") return null; return ({ unverified: "译文未人工复核", needs_review: "待校验", needs_linking: "地点待考", candidate: "候选" } as Record<string, string>)[value] ?? value; }

export function HistoryPage({ active, setNotice, intent }: { active: boolean; setNotice: (message: string) => void; intent?: { id: string; nonce: number } | null; onNavigateToGeography?: (request: HistoryGeoNavigationRequest) => void }) {
  const [home, setHome] = useState<SemanticHome>(EMPTY_HOME);
  const [homeLoaded, setHomeLoaded] = useState(false);
  const [periodDetail, setPeriodDetail] = useState<SemanticPeriodDetail | null>(null);
  const [selectedPeriodId, setSelectedPeriodId] = useState("");
  const [selectedStory, setSelectedStory] = useState<SemanticStoryDetail | null>(null);
  const [drawer, setDrawer] = useState<DrawerState>(null);
  const [loading, setLoading] = useState(true);
  const [drawerLoading, setDrawerLoading] = useState(false);
  const [error, setError] = useState("");
  const [query, setQuery] = useState("");
  const [searchGroups, setSearchGroups] = useState<SemanticSearchGroup[]>([]);
  const [searching, setSearching] = useState(false);
  const chronologyRef = useRef<HTMLDivElement>(null);

  const loadHome = useCallback(async () => {
    if (!isTauriRuntime()) return;
    setLoading(true); setError("");
    try { setHome(normalizeHome(await invoke<SemanticHome | null>("history_semantic_home"))); }
    catch (cause) { const message = errorMessage(cause); setError(message); setNotice(message); }
    finally { setHomeLoaded(true); setLoading(false); }
  }, [setNotice]);
  const loadPeriod = useCallback(async (periodId: string) => {
    if (!isTauriRuntime()) return;
    setPeriodDetail(null);
    try { setPeriodDetail(normalizePeriodDetail(await invoke<SemanticPeriodDetail | null>("history_semantic_period", { periodId }))); }
    catch (cause) { setNotice(errorMessage(cause)); }
  }, [setNotice]);
  const loadStory = useCallback(async (storyId: string) => {
    if (!isTauriRuntime()) return;
    try { const story = await invoke<SemanticStoryDetail | null>("history_semantic_story", { storyId }); if (story) { setSelectedStory(story); setDrawer(null); } }
    catch (cause) { setNotice(errorMessage(cause)); }
  }, [setNotice]);
  const loadEvent = useCallback(async (eventId: string) => {
    if (!isTauriRuntime()) return;
    setDrawerLoading(true);
    try { const event = await invoke<SemanticEventDetail | null>("history_semantic_event", { eventId }); if (event) setDrawer({ kind: "event", data: event }); }
    catch (cause) { setNotice(errorMessage(cause)); }
    finally { setDrawerLoading(false); }
  }, [setNotice]);
  const loadPerson = useCallback(async (personId: string) => {
    if (!isTauriRuntime()) return;
    setDrawerLoading(true);
    try { const person = await invoke<SemanticPersonDetail | null>("history_semantic_person", { personId }); if (person) setDrawer({ kind: "person", data: person }); else setNotice("未找到该人物的详情资料。"); }
    catch (cause) { setNotice(errorMessage(cause)); }
    finally { setDrawerLoading(false); }
  }, [setNotice]);
  const loadWork = useCallback(async (workId: string) => {
    if (!isTauriRuntime()) return;
    setDrawerLoading(true);
    try { const work = await invoke<SemanticWorkDetail | null>("history_semantic_work", { workId }); const detail = normalizeWorkDetail(work); if (detail) setDrawer({ kind: "work", data: detail }); else setNotice("未找到该作品的详情资料。"); }
    catch (cause) { setNotice(errorMessage(cause)); }
    finally { setDrawerLoading(false); }
  }, [setNotice]);

  useEffect(() => { if (active && !homeLoaded) void loadHome(); }, [active, homeLoaded, loadHome]);
  const periods = home.periods;
  const homeStories = home.stories;
  const stats = home.stats;
  const selectedPeriod = periods.find((period) => period.id === selectedPeriodId) ?? periods[0] ?? null;
  const stories = selectedPeriod && periodDetail?.period?.id === selectedPeriod.id
    ? periodDetail.stories
    : homeStories.filter((story) => story.period_id === selectedPeriod?.id);
  useEffect(() => { if (selectedPeriod) void loadPeriod(selectedPeriod.id); }, [selectedPeriod?.id, loadPeriod]);
  useEffect(() => { if (!intent?.id || !active || !homeLoaded) return; if (homeStories.some((story) => story.id === intent.id)) void loadStory(intent.id); else void loadEvent(intent.id); }, [active, homeLoaded, homeStories, intent, loadEvent, loadStory]);
  useEffect(() => {
    const text = query.trim(); if (!text || !isTauriRuntime()) { setSearchGroups([]); setSearching(false); return; }
    let cancelled = false; const timer = window.setTimeout(() => { setSearching(true); void invoke<SemanticSearchGroup[]>("history_semantic_search", { query: text }).then((groups) => { if (!cancelled) setSearchGroups(groups); }).catch((cause) => { if (!cancelled) setNotice(errorMessage(cause)); }).finally(() => { if (!cancelled) setSearching(false); }); }, 180);
    return () => { cancelled = true; window.clearTimeout(timer); };
  }, [query, setNotice]);
  const choosePeriod = useCallback((period: SemanticPeriod) => { setSelectedPeriodId(period.id); setSelectedStory(null); setDrawer(null); }, []);
  const onChronologyKeyDown = useCallback((event: KeyboardEvent<HTMLDivElement>) => { if (!periods.length || (event.key !== "ArrowLeft" && event.key !== "ArrowRight")) return; event.preventDefault(); const current = Math.max(0, periods.findIndex((period) => period.id === selectedPeriod?.id)); const next = event.key === "ArrowRight" ? Math.min(periods.length - 1, current + 1) : Math.max(0, current - 1); choosePeriod(periods[next]); document.getElementById(`history-period-${periods[next].id}`)?.scrollIntoView({ behavior: "smooth", block: "nearest", inline: "center" }); }, [choosePeriod, periods, selectedPeriod?.id]);
  const entityRouteMap: Record<SemanticSearchHit["kind"], (id: string) => void> = { story: (id) => void loadStory(id), event: (id) => void loadEvent(id), person: (id) => void loadPerson(id), work: (id) => void loadWork(id) };
  const openHit = useCallback((hit: SemanticSearchHit) => { setQuery(""); entityRouteMap[hit.kind](hit.id); }, [entityRouteMap]);

  if (!isTauriRuntime()) return <div className="history-v2-empty"><Scroll size={28} /><h1>History Explorer</h1><p>历史探索需要通过桌面应用启动，以访问真实的离线语义资料库。</p></div>;
  if (loading) return <div className="history-v2-state"><div className="history-v2-spinner" /><span>正在载入中国历史时间轴…</span></div>;
  if (error && !homeStories.length && !periods.length) return <div className="history-v2-state"><WarningCircle size={25} /><span>{error}</span><button type="button" onClick={() => void loadHome()}>重试</button></div>;
  return <div className={`history-v2${selectedStory ? " is-reading" : ""}`}>
    <header className="history-v2-header"><div className="history-v2-brand"><span className="history-v2-mark"><Scroll size={17} /></span><div><span>HISTORY / SEMANTIC ATLAS</span><strong>中国历史</strong></div></div><div className="history-v2-search-wrap"><MagnifyingGlass size={17} /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索人物、事件、故事、作品" aria-label="搜索人物、事件、故事、作品" />{query ? <button type="button" onClick={() => setQuery("")} aria-label="清除搜索"><X size={15} /></button> : null}{query ? <SearchPanel groups={searchGroups} searching={searching} onOpen={openHit} /> : null}</div></header>
    {selectedStory ? <StoryReader story={selectedStory} drawer={drawer} drawerLoading={drawerLoading} onBack={() => { setSelectedStory(null); setDrawer(null); }} onOpenEvent={(id) => void loadEvent(id)} onOpenPerson={(id) => void loadPerson(id)} onOpenPlace={(place, eventName) => setDrawer({ kind: "place", data: place, eventName })} onOpenText={(text, eventName) => setDrawer({ kind: "text", data: text, eventName })} onCloseDrawer={() => setDrawer(null)} onBackToEvent={(data) => setDrawer({ kind: "event", data })} /> : <><section className="history-v2-intro"><div><span className="history-v2-kicker">A READING MAP OF CHINA</span><h1>沿着时间，探索<br /><em>历史发生的脉络。</em></h1><p>从时代进入事件与人物，从事件走向史料证据。每一条路径都来自本地语义资料库。</p></div><div className="history-v2-intro-note"><span>CURATED SEMANTIC LAYER</span><strong>{stats?.events ?? homeStories.length}</strong><small>{stats ? "个历史事件全景" : "条可阅读 Story"}</small><i /></div></section><Chronology periods={periods} selectedId={selectedPeriod?.id ?? ""} scrollRef={chronologyRef} onKeyDown={onChronologyKeyDown} onSelect={choosePeriod} />{selectedPeriod ? <PeriodOverview period={selectedPeriod} detail={periodDetail} stories={stories} onOpenStory={(id) => void loadStory(id)} onOpenEvent={(id) => void loadEvent(id)} onOpenPerson={(id) => void loadPerson(id)} /> : <div className="history-v2-state">正在整理时期信息…</div>}{drawer ? <DetailDrawer drawer={drawer} loading={drawerLoading} story={null} onClose={() => setDrawer(null)} onOpenPerson={(id) => void loadPerson(id)} onOpenEvent={(id) => void loadEvent(id)} onOpenPlace={(place, eventName) => setDrawer({ kind: "place", data: place, eventName })} onOpenText={(text, eventName) => setDrawer({ kind: "text", data: text, eventName })} onBackToEvent={() => setDrawer(null)} /> : null}</>}
  </div>;
}

function Chronology({ periods, selectedId, scrollRef, onKeyDown, onSelect }: { periods: SemanticPeriod[]; selectedId: string; scrollRef: RefObject<HTMLDivElement | null>; onKeyDown: (event: KeyboardEvent<HTMLDivElement>) => void; onSelect: (period: SemanticPeriod) => void }) {
  const dragging = useRef(false); const startX = useRef(0); const startScroll = useRef(0); const moved = useRef(false); const pressed = useRef<HTMLButtonElement | null>(null);
  const currentIndex = Math.max(0, periods.findIndex((period) => period.id === selectedId));
  const chooseRelative = (offset: number) => { const next = periods[currentIndex + offset]; if (!next) return; onSelect(next); document.getElementById(`history-period-${next.id}`)?.scrollIntoView({ behavior: "smooth", block: "nearest", inline: "center" }); };
  return <section className="history-v2-chronology"><div className="history-v2-section-label"><span>01 / 时间长河</span><div className="history-v2-chronology-controls"><small>{periods.length ? `${currentIndex + 1} / ${periods.length}` : "暂无时期"}</small><button type="button" onClick={() => chooseRelative(-1)} disabled={currentIndex === 0} aria-label="上一个时期"><ArrowLeft size={14} /></button><button type="button" onClick={() => chooseRelative(1)} disabled={currentIndex >= periods.length - 1} aria-label="下一个时期"><ArrowRight size={14} /></button></div></div><div ref={scrollRef} className="history-v2-chronology-rail" tabIndex={0} onKeyDown={onKeyDown} onPointerDown={(event) => { dragging.current = true; moved.current = false; startX.current = event.clientX; startScroll.current = scrollRef.current?.scrollLeft ?? 0; pressed.current = (event.target as HTMLElement).closest<HTMLButtonElement>("[id^='history-period-']"); event.currentTarget.setPointerCapture(event.pointerId); }} onPointerMove={(event) => { if (dragging.current && scrollRef.current) { if (Math.abs(event.clientX - startX.current) > 5) moved.current = true; if (moved.current) scrollRef.current.scrollLeft = startScroll.current - (event.clientX - startX.current); } }} onPointerUp={() => { dragging.current = false; const target = pressed.current; pressed.current = null; if (target && !moved.current) { const period = periods.find((item) => item.id === target.id.replace("history-period-", "")); if (period) onSelect(period); } }} onPointerCancel={() => { dragging.current = false; pressed.current = null; }} aria-label="中国历史时期时间轴"><div className="history-v2-axis" />{periods.map((period, index) => <button type="button" id={`history-period-${period.id}`} key={period.id} className={`history-v2-period${period.id === selectedId ? " is-selected" : ""}`} onClick={() => onSelect(period)}><span className="history-v2-period-dot" /><small>{String(index + 1).padStart(2, "0")}</small><strong>{period.name_zh_cn}</strong><span>{rangeText(period.start_year, period.end_year)}</span></button>)}</div></section>;
}

function PeriodOverview({ period, detail, stories, onOpenStory, onOpenEvent, onOpenPerson }: { period: SemanticPeriod; detail: SemanticPeriodDetail | null; stories: SemanticStory[]; onOpenStory: (id: string) => void; onOpenEvent: (id: string) => void; onOpenPerson: (id: string) => void }) {
  const regimes = detail?.period?.id === period.id ? detail.regimes : [];
  const periodEvents = detail?.period?.id === period.id ? detail.events : [];
  const periodPeople = detail?.period?.id === period.id ? detail.people : [];
  const available = stories.filter((story) => story.usable !== false);
  const criticalCount = periodEvents.filter((event) => event.importance === "critical").length;
  return <main className="history-v2-period"><div className="history-v2-period-heading"><div><span className="history-v2-kicker">CURRENT PERIOD</span><h2>{period.name_zh_cn}</h2><p>{rangeText(period.start_year, period.end_year)}</p></div><div className="history-v2-period-description"><span>一句话理解这个时代</span><p>{period.description_zh_cn || "这个时期的数据正在整理中。"}</p></div></div><div className="history-v2-period-stats"><span><strong>{periodEvents.length}</strong>个事件</span><span><strong>{periodPeople.length}</strong>位人物</span><span><strong>{regimes.length}</strong>个政权</span>{criticalCount ? <span className="is-critical"><strong>{criticalCount}</strong>个关键事件</span> : null}</div>{regimes.length ? <section className="history-v2-regimes"><span className="history-v2-section-label"><span>01 / 时代与政权</span></span><div className="history-v2-regime-chips">{regimes.map((regime) => <span key={regime.id} className="history-v2-regime-chip"><strong>{regime.name_zh_cn}</strong><small>{rangeText(regime.start_year, regime.end_year)}</small></span>)}</div></section> : null}{periodPeople.length ? <section className="history-v2-people"><div className="history-v2-section-label"><span>02 / 核心人物</span><small>{periodPeople.length} 位活跃人物</small></div><div className="history-v2-people-chips">{periodPeople.map((person) => <button type="button" key={person.person_id} className="history-v2-person-chip" onClick={() => onOpenPerson(person.person_id)}><strong>{person.canonical_name_zh_cn}</strong><small>{person.event_count} 个事件</small></button>)}</div></section> : null}<section className="history-v2-events"><div className="history-v2-section-label"><span>03 / 本期事件时间线</span><small>{periodEvents.length ? `${periodEvents.length} 个事件 · 按时间排序` : "内容正在完善"}</small></div>{periodEvents.length ? <ol className="history-v2-event-timeline">{periodEvents.map((event) => <li key={event.id}><button type="button" className="history-v2-event-row" onClick={() => onOpenEvent(event.id)}><span className="history-v2-event-year">{yearText(event.start_year)}</span><div className="history-v2-event-main"><span className="history-v2-event-name">{event.name_zh_cn}{event.importance === "critical" ? <i className="is-critical">关键</i> : event.importance === "major" ? <i>重要</i> : null}</span><p>{event.summary_zh_cn || event.result_zh_cn || "事件叙述正在整理中。"}</p><div className="history-v2-event-meta"><span><UsersThree size={13} />{event.people_count} 人物</span><span><Scroll size={13} />{event.evidence_count} 证据</span>{event.relation_count ? <span><ArrowRight size={13} />{event.relation_count} 关系</span> : null}</div></div><CaretRight size={15} className="history-v2-event-arrow" /></button></li>)}</ol> : <div className="history-v2-developing"><CalendarBlank size={20} /><div><strong>这个时期暂无结构化的关键事件。</strong><p>时间轴已经开放，事件内容会随着语义资料校验逐步加入。</p></div></div>}</section>{available.length ? <section className="history-v2-stories"><div className="history-v2-section-label"><span>04 / 精选 Story</span><small>{available.length} 条可阅读 Story</small></div><div className="history-v2-story-list">{available.map((story, index) => <StoryCard key={story.id} story={story} index={index} onOpen={() => onOpenStory(story.id)} />)}</div></section> : null}</main>;
}
function StoryCard({ story, index, onOpen }: { story: SemanticStory; index: number; onOpen: () => void }) { const quality = qualityLabel(story.quality_status); return <button type="button" className="history-v2-story-card" onClick={onOpen}><span className="history-v2-story-index">0{index + 1}</span><div className="history-v2-story-card-main"><span className="history-v2-kicker">{story.story_type || "HISTORICAL STORY"}</span><h3>{story.title_zh_cn}</h3><p>{story.summary_zh_cn || "这条历史故事正在整理中。"}</p><div className="history-v2-story-meta"><span><Clock size={14} />{rangeText(story.start_year, story.end_year)}</span><span className="history-v2-quality">{quality ? <><WarningCircle size={14} />{quality}</> : <><SealCheck size={14} />已通过 QA</>}</span></div></div><ArrowRight size={18} className="history-v2-story-arrow" /></button>; }

function StoryReader({ story, drawer, drawerLoading, onBack, onOpenEvent, onOpenPerson, onOpenPlace, onOpenText, onCloseDrawer, onBackToEvent }: { story: SemanticStoryDetail; drawer: DrawerState; drawerLoading: boolean; onBack: () => void; onOpenEvent: (id: string) => void; onOpenPerson: (id: string) => void; onOpenPlace: (place: SemanticEventPlace, eventName: string) => void; onOpenText: (text: SemanticHistoricalText, eventName: string) => void; onCloseDrawer: () => void; onBackToEvent: (data: SemanticEventDetail) => void }) {
  const people = new Set(story.people.map((person) => person.person_id)); const places = new Set(story.places.map((place) => place.place_name || place.place_name_raw || place.event_id));
  return <div className="history-v2-reader"><button type="button" className="history-v2-back" onClick={onBack}><ArrowLeft size={16} />返回时间轴</button><header className="history-v2-story-hero"><div><span className="history-v2-kicker">STORY / {story.story.story_type || "CURATED"}</span><h1>{story.story.title_zh_cn}</h1><p>{story.story.summary_zh_cn}</p></div><div className="history-v2-story-range"><span>{rangeText(story.story.start_year, story.story.end_year)}</span><small>{story.events.length} 个关键事件 · {people.size} 位核心人物</small></div></header>{story.story.background_zh_cn ? <p className="history-v2-story-background">{story.story.background_zh_cn}</p> : null}<div className="history-v2-reader-layout"><main><div className="history-v2-section-label"><span>03 / STORY FLOW</span><small>按语义层 sequence 阅读</small></div><ol className="history-v2-flow">{story.events.map((event, index) => <li key={event.event_id} className={event.event_id === (drawer?.kind === "event" ? drawer.data.event.id : "") ? "is-active" : ""}><div className="history-v2-flow-marker"><span>{String(event.sequence ?? index + 1).padStart(2, "0")}</span><i /></div><button type="button" className="history-v2-flow-event" onClick={() => onOpenEvent(event.event_id)}><div className="history-v2-flow-date">{rangeText(event.start_year, event.end_year)}</div><h3>{event.name_zh_cn}<CaretRight size={16} /></h3><p>{event.summary_zh_cn || event.result_zh_cn || "事件叙述正在整理中。"}</p><small>{event.transition_text_zh_cn || "时间线中的下一个节点"}</small></button></li>)}</ol></main><aside className="history-v2-story-aside"><StatLine icon={<UsersThree size={17} />} label="核心人物" value={`${people.size} 位`} /><StatLine icon={<MapPin size={17} />} label="关联地点" value={`${places.size} 处`} /><StatLine icon={<BookOpen size={17} />} label="史料证据" value={`${Math.max(story.evidences.length, story.historical_texts.length)} 条`} /><SourceSection sources={story.sources} compact /><div className="history-v2-aside-note"><CheckCircle size={15} />{story.story.usable ? "Story 已通过 Semantic QA" : "Story 正在校验"}</div></aside></div>{drawer ? <DetailDrawer drawer={drawer} loading={drawerLoading} story={story} onClose={onCloseDrawer} onOpenPerson={onOpenPerson} onOpenEvent={onOpenEvent} onOpenPlace={onOpenPlace} onOpenText={onOpenText} onBackToEvent={onBackToEvent} /> : null}</div>;
}
function StatLine({ icon, label, value }: { icon: ReactNode; label: string; value: string }) { return <div className="history-v2-stat"><span>{icon}</span><small>{label}</small><strong>{value}</strong></div>; }

function DetailDrawer({ drawer, loading, story, onClose, onOpenPerson, onOpenEvent, onOpenPlace, onOpenText, onBackToEvent }: { drawer: Exclude<DrawerState, null>; loading: boolean; story: SemanticStoryDetail | null; onClose: () => void; onOpenPerson: (id: string) => void; onOpenEvent: (id: string) => void; onOpenPlace: (place: SemanticEventPlace, eventName: string) => void; onOpenText: (text: SemanticHistoricalText, eventName: string) => void; onBackToEvent: (data: SemanticEventDetail) => void }) {
  const title = drawer.kind === "event" ? drawer.data.event.name_zh_cn : drawer.kind === "person" ? drawer.data.person.canonical_name_zh_cn : drawer.kind === "place" ? drawer.data.place_name || drawer.data.place_name_raw || "历史地点" : drawer.kind === "work" ? drawer.data.work.title_zh_cn || drawer.data.work.title : drawer.data.title_zh_cn || "史料阅读";
  return <aside className="history-v2-drawer"><header><div><span className="history-v2-kicker">{drawer.kind === "event" ? "EVENT DETAIL" : drawer.kind === "person" ? "PERSON DETAIL" : drawer.kind === "place" ? "PLACE DETAIL" : drawer.kind === "work" ? "WORK DETAIL" : "HISTORICAL TEXT"}</span><strong>{title}</strong></div><button type="button" onClick={onClose} aria-label="关闭详情"><X size={18} /></button></header>{loading ? <div className="history-v2-drawer-loading"><div className="history-v2-spinner" />正在读取关联资料…</div> : drawer.kind === "event" ? <EventPanel data={drawer.data} onOpenPerson={onOpenPerson} onOpenEvent={onOpenEvent} onOpenPlace={onOpenPlace} onOpenText={onOpenText} /> : drawer.kind === "person" ? <PersonPanel data={drawer.data} onOpenEvent={onOpenEvent} /> : drawer.kind === "place" ? <PlacePanel place={drawer.data} eventName={drawer.eventName} /> : drawer.kind === "work" ? <WorkPanel data={drawer.data} onOpenText={onOpenText} /> : <TextReader text={drawer.data} eventName={drawer.eventName} onBack={() => { const event = story?.events.find((item) => item.name_zh_cn === drawer.eventName); if (event && story) onBackToEvent({ event: { id: event.event_id, name_zh_cn: event.name_zh_cn, start_year: event.start_year, end_year: event.end_year, summary_zh_cn: event.summary_zh_cn, result_zh_cn: event.result_zh_cn, quality_status: event.event_quality_status }, people: story.people.filter((item) => item.event_id === event.event_id), places: story.places.filter((item) => item.event_id === event.event_id), relations: [], historical_texts: story.historical_texts.filter((item) => item.event_id === event.event_id), evidences: story.evidences.filter((item) => item.event_id === event.event_id), sources: story.sources }); else onClose(); }} />}</aside>;
}

/** 将作品下的章节全文（HistoricalTextResult）转成阅读器需要的 scoped text。 */
function toReaderText(text: SemanticWorkDetail["texts"][number]): SemanticHistoricalText {
  return { event_id: "", historical_text_id: text.id, title_zh_cn: text.title_zh_cn, work_title: text.work_title, chapter: text.chapter, original_text: text.original_text, original_simplified: text.original_simplified, translation_zh_cn: text.translation_zh_cn, translation_source: text.translation_source, alignment_quality: text.alignment_quality, source_quality_status: text.quality_status, quality_status: text.quality_status };
}
function WorkPanel({ data, onOpenText }: { data: SemanticWorkDetail; onOpenText: (text: SemanticHistoricalText, eventName: string) => void }) { const work = data.work; const name = work.title_zh_cn || work.title; return <div className="history-v2-drawer-body"><div className="history-v2-drawer-hero"><span>WORK / {work.quality_status || "SEMANTIC"}</span><h2>{name}</h2><p>{work.title_zh_cn ? `原题：${work.title}` : "史籍作品；点击下方章节进入原文与译文阅读。"}</p></div><DrawerSection title={`章节文本 · ${data.texts.length}`} icon={<BookOpen size={16} />}>{data.texts.length ? <div className="history-v2-text-list">{data.texts.map((text) => <button type="button" key={text.id} onClick={() => onOpenText(toReaderText(text), name)}><span>{text.work_title || name}</span><strong>{text.chapter || text.title_zh_cn || "未命名章节"}</strong><small>{qualityLabel(text.quality_status) || "已结构化文本"}<ArrowRight size={14} /></small></button>)}</div> : <EmptyInline text="这部作品暂无已收录的章节全文。" />}</DrawerSection><SourceSection sources={data.sources} /></div>; }

function EventPanel({ data, onOpenPerson, onOpenEvent, onOpenPlace, onOpenText }: { data: SemanticEventDetail; onOpenPerson: (id: string) => void; onOpenEvent: (id: string) => void; onOpenPlace: (place: SemanticEventPlace, eventName: string) => void; onOpenText: (text: SemanticHistoricalText, eventName: string) => void }) {
  const event = data.event; const related = buildRelatedEvents(event.id, data.relations); const narrative: [string, string | null | undefined][] = [["背景", event.background_zh_cn], ["过程", event.process_zh_cn], ["结果", event.result_zh_cn], ["影响", event.impact_zh_cn]];
  return <div className="history-v2-drawer-body"><div className="history-v2-drawer-hero"><span>{rangeText(event.start_year, event.end_year)}</span><h2>{event.name_zh_cn}</h2><p>{event.summary_zh_cn || "事件叙述正在整理中。"}</p></div><DrawerSection title="事件始末" icon={<Clock size={16} />}>{narrative.some(([, text]) => text) ? <div className="history-v2-narrative">{narrative.map(([label, text]) => (text ? <span key={label}><strong>{label}</strong><p>{text}</p></span> : null))}</div> : <EmptyInline text="事件始末的史料叙述正在整理中。" />}</DrawerSection><DrawerSection title="参与人物" icon={<UsersThree size={16} />}>{data.people.length ? <div className="history-v2-entity-list">{data.people.map((person) => <button type="button" key={`${person.event_id}-${person.person_id}`} onClick={() => onOpenPerson(person.person_id)}><span>{person.person_name || "人物待考"}</span><small>{person.role_zh_cn || person.role}</small></button>)}</div> : <EmptyInline text="当前事件未关联人物。" />}</DrawerSection><DrawerSection title="地点" icon={<MapPin size={16} />}>{data.places.length ? <div className="history-v2-place-list">{data.places.map((place, index) => <PlaceRow key={`${place.event_id}-${index}`} place={place} eventName={event.name_zh_cn} onOpen={onOpenPlace} />)}</div> : <EmptyInline text="当前事件未关联地点。" />}</DrawerSection><DrawerSection title="前置与后续事件" icon={<ArrowRight size={16} />}>{related.length ? <div className="history-v2-entity-list">{related.map((item) => <button type="button" key={item.other_id} onClick={() => onOpenEvent(item.other_id)}><span>{item.other_name || "关联事件"}</span><small>{item.tag}{item.description ? ` · ${item.description}` : ""}</small></button>)}</div> : <EmptyInline text="当前事件暂无结构化前后关系。" />}</DrawerSection><DrawerSection title="史料证据" icon={<Scroll size={16} />}>{data.evidences.length ? <div className="history-v2-evidence-list">{data.evidences.map((evidence) => <EvidenceRow key={evidence.id} evidence={evidence} />)}</div> : <EmptyInline text="当前事件暂无章节级史料证据。" />}</DrawerSection><DrawerSection title="史料全文" icon={<BookOpen size={16} />}>{data.historical_texts.length ? <div className="history-v2-text-list">{data.historical_texts.map((text) => <button type="button" key={text.historical_text_id} onClick={() => onOpenText(text, event.name_zh_cn)}><span>{text.work_title || "历史文献"}</span><strong>{text.chapter || text.title_zh_cn || "进入阅读"}</strong><small>{qualityLabel(text.source_quality_status) || "真实关联史料"}<ArrowRight size={14} /></small></button>)}</div> : <EmptyInline text="暂无进入阅读器的全文史料；章节级证据见上。" />}</DrawerSection><SourceSection sources={data.sources} /></div>;
}
function buildRelatedEvents(eventId: string, relations: SemanticEventRelation[]): { other_id: string; other_name: string | null; tag: string; description: string }[] {
  const rows = new Map<string, { other_id: string; other_name: string | null; tag: string; description: string; causal: boolean }>();
  for (const relation of relations) {
    const isSource = relation.source_event_id === eventId;
    if (!isSource && relation.target_event_id !== eventId) continue;
    const other_id = isSource ? relation.target_event_id : relation.source_event_id;
    const other_name = (isSource ? relation.target_event_name : relation.source_event_name) ?? null;
    const type = relation.relation_type;
    const description = relation.description_zh_cn ?? "";
    const causal = type === "leads_to" || type === "causes" || type === "contributes_to";
    let tag: string;
    switch (type) {
      case "precedes": tag = isSource ? "后续" : "前置"; break;
      case "follows": tag = isSource ? "前置" : "后续"; break;
      case "leads_to": tag = isSource ? "后续 · 由它引发" : "前置 · 导因"; break;
      case "causes": tag = isSource ? "后续 · 受其影响" : "前置 · 影响源"; break;
      case "contributes_to": tag = isSource ? "后续 · 由其促成" : "前置 · 促成因素"; break;
      case "part_of": tag = "从属事件"; break;
      default: tag = relationLabel(type); break;
    }
    const existing = rows.get(other_id);
    if (!existing) { rows.set(other_id, { other_id, other_name, tag, description, causal }); continue; }
    if (causal && !existing.causal) existing.tag = tag;
    if (description && !existing.description) existing.description = description;
  }
  const rank = (tag: string) => (tag.startsWith("前置") ? 0 : tag.startsWith("后续") ? 2 : 1);
  return Array.from(rows.values()).map(({ other_id, other_name, tag, description }) => ({ other_id, other_name, tag, description })).sort((a, b) => rank(a.tag) - rank(b.tag));
}
function relationLabel(value: string): string { return ({ precedes: "前置", follows: "后续", leads_to: "推进", causes: "影响", contributes_to: "促成", related_to: "相关" } as Record<string, string>)[value] || value; }
function evidenceRoleLabel(value: string | null | undefined): string { return ({ primary: "主要证据", supporting: "佐证" } as Record<string, string>)[value || ""] ?? (value || "相关文献"); }
function evidenceLinkLabel(value: string | null | undefined): string { return ({ linked: "已关联原文", needs_linking: "待关联原文", pending_knowledge: "待知识库解析" } as Record<string, string>)[value || ""] ?? (value || "原文关联状态待考"); }
function EvidenceRow({ evidence }: { evidence: SemanticEventEvidence }) { return <article className="history-v2-evidence"><strong>{evidence.work || "史籍"}{evidence.term || evidence.chapter_hint ? ` · ${evidence.term || evidence.chapter_hint}` : ""}</strong>{evidence.review_note ? <p>{evidence.review_note}</p> : null}{evidence.chapter_hint && evidence.chapter_hint !== evidence.term ? <small className="history-v2-evidence-chapter">篇章：{evidence.chapter_hint}</small> : null}<div className="history-v2-evidence-meta"><span>{evidenceRoleLabel(evidence.evidence_role)}</span><span>{evidenceLinkLabel(evidence.link_status)}</span>{evidence.quality_status === "reviewed" ? <span className="is-ok">已复核</span> : null}</div></article>; }
function PlaceRow({ place, eventName, onOpen }: { place: SemanticEventPlace; eventName: string; onOpen: (place: SemanticEventPlace, eventName: string) => void }) { const name = place.place_name || place.place_name_raw || "历史地点"; return <button type="button" className={`history-v2-place-row${!place.place_id || place.link_status === "needs_linking" ? " is-pending" : ""}`} onClick={() => onOpen(place, eventName)}><MapPin size={16} /><div><strong>{name}</strong><small>{!place.place_id || place.link_status === "needs_linking" ? "位置待考 / 尚未完成地理关联" : place.modern_name || place.historical_name || (place.latitude !== null && place.latitude !== undefined ? `${place.latitude.toFixed(2)}°, ${place.longitude?.toFixed(2)}°` : "历史地点")}</small></div><CaretRight size={14} /></button>; }
function PlacePanel({ place, eventName }: { place: SemanticEventPlace; eventName: string }) { const name = place.place_name || place.place_name_raw || "历史地点"; const pending = !place.place_id || place.link_status === "needs_linking"; return <div className="history-v2-drawer-body"><div className="history-v2-drawer-hero"><span>{eventName}</span><h2>{name}</h2><p>{pending ? "位置待考 / 尚未完成地理关联" : place.description_zh_cn || "历史地点"}</p></div><div className="history-v2-place-detail"><span>历史名称</span><strong>{place.historical_name || name}</strong>{place.modern_name ? <><span>现代名称</span><strong>{place.modern_name}</strong></> : null}{place.latitude !== null && place.latitude !== undefined && place.longitude !== null && place.longitude !== undefined ? <><span>坐标</span><strong>{place.latitude.toFixed(3)}° N · {place.longitude.toFixed(3)}° E</strong></> : null}</div><div className={`history-v2-quality-note${pending ? "" : " is-ok"}`}><WarningCircle size={15} />{pending ? "此地点没有可靠 canonical Place 关联，不会展示到现代地图。" : "地点已通过时间与名称关联校验。"}</div></div>; }

function PersonPanel({ data, onOpenEvent }: { data: SemanticPersonDetail; onOpenEvent: (id: string) => void }) { const person = data.person; const relatedPeople = new Map<string, { id: string; name: string; labels: Set<string>; records: number }>(); data.relations.filter((relation) => relation.person_a_id !== relation.person_b_id).forEach((relation) => { const isSource = relation.person_a_id === person.id; const id = isSource ? relation.person_b_id : relation.person_a_id; const name = (isSource ? relation.person_b_name : relation.person_a_name) || "人物待考"; const label = relation.relation_name_zh_cn || `CBDB 关系代码 ${relation.relation_type}`; const existing = relatedPeople.get(id) || { id, name, labels: new Set<string>(), records: 0 }; existing.labels.add(label); existing.records += 1; relatedPeople.set(id, existing); }); const people = Array.from(relatedPeople.values()).sort((left, right) => left.name.localeCompare(right.name, "zh-CN")).slice(0, 20); return <div className="history-v2-drawer-body"><div className="history-v2-drawer-hero"><span>{rangeText(person.birth_year, person.death_year)}</span><h2>{person.canonical_name_zh_cn}</h2><p>{person.intro_zh_cn || "人物概览正在整理中。"}</p></div>{person.name_raw && person.name_raw !== person.canonical_name_zh_cn ? <div className="history-v2-alias">原始名称：{person.name_raw}</div> : null}<DrawerSection title="参与 Event" icon={<CalendarBlank size={16} />}>{data.events.length ? <div className="history-v2-entity-list">{data.events.slice(0, 20).map((event) => <button type="button" key={event.event_id} onClick={() => onOpenEvent(event.event_id)}><span>{event.event_name}</span><small>{rangeText(event.start_year, event.end_year)} · {event.role_zh_cn || "参与人物"}</small></button>)}</div> : <EmptyInline text="暂无结构化 Event 参与记录。" />}</DrawerSection><DrawerSection title={`人物关系${relatedPeople.size ? ` · ${relatedPeople.size} 人` : ""}`} icon={<Funnel size={16} />}>{people.length ? <div className="history-v2-relation-groups"><div>{people.map((related) => <div key={related.id}><span>{related.name}</span><em>{Array.from(related.labels).join(" / ")}{related.records > 1 ? ` · 合并 ${related.records} 条来源记录` : ""}</em></div>)}</div></div> : <EmptyInline text="暂无可展示的人物关系。" />}</DrawerSection><SourceSection sources={data.sources} /></div>; }

function TextReader({ text, eventName, onBack }: { text: SemanticHistoricalText; eventName: string; onBack: () => void }) { const [mode, setMode] = useState<"original" | "simplified" | "translation" | "parallel">("parallel"); const quality = qualityLabel(text.source_quality_status); const content = (key: "original_text" | "original_simplified" | "translation_zh_cn") => text[key] || "这层文本暂无内容。"; return <div className="history-v2-drawer-body history-v2-text-reader"><button type="button" className="history-v2-reader-back" onClick={onBack}><ArrowLeft size={15} />返回 {eventName}</button><div className="history-v2-text-heading"><span>{text.work_title || "历史文献"}</span><h2>{text.chapter || text.title_zh_cn || "史料阅读"}</h2></div><div className="history-v2-text-tabs">{([ ["parallel", "对照"], ["original", "原文"], ["simplified", "简体"], ["translation", "译文"]] as const).map(([value, label]) => <button type="button" key={value} className={mode === value ? "is-active" : ""} onClick={() => setMode(value)}>{label}</button>)}</div>{quality ? <div className="history-v2-quality-note"><WarningCircle size={15} />{quality}</div> : null}<div className={`history-v2-text-content is-${mode}`}>{mode === "parallel" ? <><TextBlock label="原文" text={content("original_text")} /><TextBlock label="简体" text={content("original_simplified")} /><TextBlock label="译文" text={content("translation_zh_cn")} /></> : <TextBlock label={mode === "original" ? "原文" : mode === "simplified" ? "简体" : "译文"} text={content(mode === "original" ? "original_text" : mode === "simplified" ? "original_simplified" : "translation_zh_cn")} />}</div><div className="history-v2-text-provenance"><span>来源与数据说明</span><p>原文、简体古文与现代译文分别来自 HistoricalText 的真实字段；关联状态由 Semantic Layer QA 提供。</p>{text.translation_source ? <small>译文来源：{text.translation_source}</small> : null}</div></div>; }
function TextBlock({ label, text }: { label: string; text: string }) { return <section><span>{label}</span><p>{text}</p></section>; }
function DrawerSection({ title, icon, children }: { title: string; icon: ReactNode; children: ReactNode }) { return <section className="history-v2-drawer-section"><h3>{icon}{title}</h3>{children}</section>; }
function EmptyInline({ text }: { text: string }) { return <p className="history-v2-empty-inline">{text}</p>; }
function SourceSection({ sources, compact = false }: { sources: SemanticSource[]; compact?: boolean }) { const [open, setOpen] = useState(false); if (!sources.length) return null; return <section className={`history-v2-sources${compact ? " is-compact" : ""}`}><button type="button" onClick={() => setOpen((value) => !value)}><span><SealCheck size={15} />来源与数据说明</span><small>{open ? "收起" : `${sources.length} 个来源`}</small></button>{open ? <div>{sources.map((source) => <article key={source.id}><strong>{source.dataset || source.source_type || "数据来源"}</strong><span>{source.dataset_version || source.snapshot_version || "版本未提供"}</span><small>{source.license || "License 信息见数据说明"} · {source.quality || source.quality_status || "质量状态未提供"}</small></article>)}</div> : null}</section>; }
function SearchPanel({ groups, searching, onOpen }: { groups: SemanticSearchGroup[]; searching: boolean; onOpen: (hit: SemanticSearchHit) => void }) { return <div className="history-v2-search-panel">{searching ? <p>正在搜索…</p> : groups.length ? groups.map((group) => <section key={group.kind}><h3>{kindLabel(group.kind)}<small>{group.items.length}</small></h3>{group.items.map((hit) => <button type="button" key={`${hit.kind}-${hit.id}`} onClick={() => onOpen(hit)}><span>{hit.title}</span><small>{hit.subtitle || rangeText(hit.start_year, hit.end_year)}</small><em>打开</em></button>)}</section>) : <p>没有匹配资料，换个关键词试试。</p>}</div>; }
