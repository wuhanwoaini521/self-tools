import { invoke } from "@tauri-apps/api/core";
import { ArrowRight, BookmarkSimple, BookOpen, Compass, GitBranch, MagnifyingGlass, Question, X } from "@phosphor-icons/react";
import { useCallback, useEffect, useMemo, useState } from "react";
import type { GeoEntity, GeoEntityDetail, GeoEntityType, GeoMapLine, GeoMapPoint, GeoSearchGroup, GeographyHome } from "../../types";
import { errorMessage, isTauriRuntime } from "../../utils";
import { AmapRegionMap } from "./AmapRegionMap";
import { GeoMap, type GeoMapLayer } from "./GeoMap";
import { Landform3DViewer } from "./Landform3DViewer";
import { LANDFORM_KNOWLEDGE_ENTRIES } from "./landformKnowledgeData";
import { LANDFORM_VIEWERS } from "./landform3dData";

interface GeographyPageProps { active: boolean; setNotice: (notice: string) => void; amapApiKey?: string | null; amapSecurityJsCode?: string | null; intent?: { entityId: string; nonce: number } | null; }

const TYPE_LABELS: Record<GeoEntityType, string> = {
  world: "世界", country: "国家", region: "区域", province: "省级行政区", city: "城市", river: "河流", mountain: "山体", mountain_range: "山脉", plateau: "高原", plain: "平原", basin: "盆地", desert: "沙漠", lake: "湖泊", ocean: "海洋", sea: "海域", island: "岛屿", archipelago: "群岛", climate_zone: "气候区", tectonic_plate: "构造板块",
};

function entity(id: string, entityType: GeoEntityType, name: string, nameEn: string, summary: string, coordinates: [number, number], properties: [string, string, string | null][]): GeoEntity {
  return { id, entity_type: entityType, name, name_en: nameEn, aliases: [], coordinates: { system: "WGS84", latitude: coordinates[0], longitude: coordinates[1] }, geometry: null, parent_id: null, summary, properties: properties.map(([key, value, unit]) => ({ key, label: key, value, unit, source_ids: ["geography-seed"] })), source_ids: ["geography-seed"] };
}

const FALLBACK_ENTITIES: GeoEntity[] = [
  entity("china", "country", "中国", "China", "季风、水系与人口城市分布彼此相连。", [35.86, 104.2], [["面积", "9,596,960", "km²"], ["人口", "1,410,710,000", "人 · 2023"], ["人口密度", "147", "人/km² · 2023"], ["地形", "青藏高原—高原盆地—平原丘陵", null], ["气候", "季风气候为主，南北东西差异显著", null], ["主要城市", "北京、上海、成都、重庆、武汉", null]]),
  entity("japan", "country", "日本", "Japan", "板块边界、山地与沿海平原共同塑造岛国。", [36.2, 138.25], [["面积", "377,975", "km²"], ["人口", "124,516,000", "人 · 2023"], ["人口密度", "330", "人/km² · 2023"], ["地形", "山地为主，平原沿海分布", null], ["气候", "南北跨度大，受季风与海洋影响", null], ["主要城市", "东京、大阪、名古屋、福冈", null]]),
  entity("sichuan", "province", "四川", "Sichuan", "盆地、山地、河流与季风共同解释这里的自然格局。", [30.65, 102.7], [["地形", "四川盆地与周缘山地", null], ["气候", "湿润、多云，盆地内水热条件较好", null], ["主要城市", "成都、绵阳、乐山、宜宾", null]]),
  entity("chengdu", "city", "成都", "Chengdu", "盆地西部的平原城市，农业基础支撑了长期发展。", [30.57, 104.07], [["海拔", "约 500", "m"], ["地形", "成都平原，位于四川盆地西部", null], ["气候", "亚热带湿润季风气候，多云雾", null], ["人口", "21,400,000", "人 · 2023口径"], ["水系", "都江堰—岷江水系", null]]),
  entity("chongqing", "city", "重庆", "Chongqing", "长江与嘉陵江交汇的山地河谷城市。", [29.56, 106.55], [["海拔", "约 260", "m"], ["地形", "山地与河谷", null], ["水系", "长江与嘉陵江交汇", null], ["人口", "32,100,000", "人 · 2023口径"]]),
  entity("yangtze", "river", "长江", "Yangtze River", "连接高原、盆地、平原和海洋的一条探索主线。", [30, 112], [["长度", "约 6,300", "km"], ["源头区域", "青藏高原腹地", null], ["沿线城市", "重庆、武汉、南京、上海", null], ["流经地形", "高原、峡谷、盆地、平原", null]]),
  entity("tibetan-plateau", "plateau", "青藏高原", "Tibetan Plateau", "印度板块与欧亚板块碰撞塑造的高亚洲地形核心。", [33, 88], [["平均海拔", "超过 4,000", "m"], ["地形", "高原与高山系统", null], ["气候", "高寒，影响亚洲季风与水汽输送", null]]),
  entity("sichuan-basin", "basin", "四川盆地", "Sichuan Basin", "周缘山地限制气流交换，水汽与云雾更易积聚。", [30.5, 105.5], [["地形", "盆地，西部连接成都平原", null], ["气候", "湿润、多云雾", null], ["人口关系", "平原与水系支持高密度聚居", null]]),
  entity("himalayas", "mountain_range", "喜马拉雅山脉", "Himalayas", "板块碰撞的地表结果之一，也是亚洲重要气候屏障。", [28, 84], [["最高峰", "珠穆朗玛峰约 8,849", "m"], ["形成过程", "印度板块与欧亚板块碰撞", null], ["气候影响", "改变水汽输送与季风分布", null]]),
  entity("qinling", "mountain_range", "秦岭", "Qinling Mountains", "中国南北气候、植被和水系差异的重要分界。", [33.8, 108], [["气候影响", "中国南北气候分界的重要山脉", null], ["水系", "长江与黄河流域分水背景", null]]),
  entity("shanghai", "city", "上海", "Shanghai", "长江三角洲冲积平原上的河口城市。", [31.23, 121.47], [["地形", "长江三角洲冲积平原", null], ["水系", "长江入海口", null], ["人口", "24,890,000", "人 · 2023口径"]]),
  entity("northeast-china", "region", "中国东北", "Northeast China", "东北平原、黑土地与寒冷冬季共同塑造农业与人口分布。", [45.7, 126.6], [["地形", "东北平原与大小兴安岭", null], ["气候", "温带季风，冬季寒冷", null], ["主要城市", "哈尔滨、长春、沈阳、大连", null]]),
];

const FALLBACK_LINES: GeoMapLine[] = [
  { from_id: "yangtze", to_id: "tibetan-plateau", kind: "SOURCE_OF", from: { system: "WGS84", latitude: 30, longitude: 112 }, to: { system: "WGS84", latitude: 33, longitude: 88 } },
  { from_id: "yangtze", to_id: "chongqing", kind: "FLOWS_THROUGH", from: { system: "WGS84", latitude: 30, longitude: 112 }, to: { system: "WGS84", latitude: 29.56, longitude: 106.55 } },
  { from_id: "yangtze", to_id: "shanghai", kind: "FLOWS_INTO", from: { system: "WGS84", latitude: 30, longitude: 112 }, to: { system: "WGS84", latitude: 31.23, longitude: 121.47 } },
  { from_id: "chengdu", to_id: "sichuan", kind: "LOCATED_IN", from: { system: "WGS84", latitude: 30.57, longitude: 104.07 }, to: { system: "WGS84", latitude: 30.65, longitude: 102.7 } },
  { from_id: "sichuan-basin", to_id: "himalayas", kind: "SURROUNDS", from: { system: "WGS84", latitude: 30.5, longitude: 105.5 }, to: { system: "WGS84", latitude: 28, longitude: 84 } },
];

const FALLBACK_POINTS: GeoMapPoint[] = FALLBACK_ENTITIES.map((item) => ({ entity_id: item.id, name: item.name, entity_type: item.entity_type, coordinate: item.coordinates! }));
const FALLBACK_HOME: GeographyHome = { recommendation: { kind: "question", title: "青藏高原", question: "为什么青藏高原会拥有如此极端的海拔？", entity_id: "tibetan-plateau", tags: ["板块碰撞", "喜马拉雅山", "亚洲季风"] }, featured: FALLBACK_ENTITIES.filter((item) => ["tibetan-plateau", "yangtze", "sichuan-basin", "japan", "chengdu"].includes(item.id)), recent: [], favorite_ids: [], map_points: FALLBACK_POINTS, map_lines: FALLBACK_LINES };

function fallbackGroups(query: string): GeoSearchGroup[] {
  const needle = query.trim().toLowerCase();
  const groups = new Map<GeoEntityType, GeoEntity[]>();
  FALLBACK_ENTITIES.filter((item) => `${item.name} ${item.name_en} ${item.aliases.join(" ")} ${item.summary}`.toLowerCase().includes(needle)).forEach((item) => groups.set(item.entity_type, [...(groups.get(item.entity_type) ?? []), item]));
  return [...groups.entries()].map(([entityType, items]) => ({ entity_type: entityType, items }));
}

function fallbackDetail(id: string, favorite = false): GeoEntityDetail | null {
  const item = FALLBACK_ENTITIES.find((candidate) => candidate.id === id);
  if (!item) return null;
  return { entity: item, favorite, relations: FALLBACK_LINES.filter((line) => line.from_id === id || line.to_id === id).map((line) => { const relatedId = line.from_id === id ? line.to_id : line.from_id; const related = FALLBACK_ENTITIES.find((candidate) => candidate.id === relatedId)!; return { relation: { from_id: line.from_id, to_id: line.to_id, kind: line.kind, note: null, source_ids: ["geography-seed"] }, entity: related }; }), sources: [{ id: "geography-seed", dataset: "Geography Explorer embedded seed", version: "geography-seed-v1", url: "https://github.com/wuhanwoaini521/self-tools", license: "Project license; source links in data-source record", updated_at: "2026-09-01", fields: ["curated summaries", "exploration relations"] }] };
}

function propertyValue(item: GeoEntity, key: string) { return item.properties.find((property) => property.key === key)?.value ?? null; }

const RELATION_LABELS: Record<string, string> = {
  LOCATED_IN: "位于",
  PART_OF: "属于",
  BORDERS: "接壤",
  FLOWS_THROUGH: "流经",
  SOURCE_OF: "发源于",
  FLOWS_INTO: "汇入",
  CONNECTED_TO: "连接",
  NEAR: "邻近",
  CROSSES: "穿过",
  SURROUNDS: "环绕",
  AFFECTS: "影响",
  INFLUENCES: "塑造",
  FORMED_BY: "形成于",
  CAUSES: "导致",
  BELONGS_TO_CLIMATE_ZONE: "属于气候区",
};

function relationLabel(kind: string) { return RELATION_LABELS[kind] ?? kind; }

function parentRelation(detail: GeoEntityDetail) {
  return detail.relations.find((item) =>
    (item.relation.kind === "PART_OF" || item.relation.kind === "LOCATED_IN") &&
    item.relation.from_id === detail.entity.id,
  )?.entity ?? null;
}

function learningCue(item: GeoEntity) {
  const terrain = propertyValue(item, "terrain");
  const climate = propertyValue(item, "climate");
  const river = propertyValue(item, "river_system");
  if (item.entity_type === "city") {
    return {
      remember: `${item.name}可以先从“${terrain ?? "所在地形"}”和“${river ?? "周边水系"}”两个线索理解。`,
      ask: "为什么城市会在这里形成？先看地形能否提供平地，再看水系、交通和人类活动如何叠加。",
    };
  }
  if (item.entity_type === "country" || item.entity_type === "province" || item.entity_type === "region") {
    return {
      remember: `${item.name}的空间格局，可以按“地形 → 气候 → 人口与城市”这条线索阅读。`,
      ask: `先对照${terrain ?? "地形差异"}与${climate ?? "气候差异"}，再观察人口和主要城市为什么集中在那里。`,
    };
  }
  if (item.entity_type === "river") {
    return {
      remember: `${item.name}适合沿着“源头 → 流经地形 → 沿线城市 → 入海方向”来记。`,
      ask: "河流为什么会成为一条地理主线？因为它把不同地形单元、水资源和聚落联系在了一起。",
    };
  }
  return {
    remember: `${item.name}先记住它的${terrain ?? "地形或地质特征"}，再观察它与周围实体的关系。`,
    ask: "它不是孤立的名词；结合气候、水系和相邻地貌，才能解释它为什么呈现现在的样子。",
  };
}

function EntityCard({ item, onOpen }: { item: GeoEntity; onOpen: (id: string) => void }) {
  return <button type="button" className="geo-entity-card" onClick={() => onOpen(item.id)}><span className="geo-card-type">{TYPE_LABELS[item.entity_type]}</span><strong>{item.name}</strong><small>{item.name_en}</small><p>{item.summary}</p><ArrowRight size={15} /></button>;
}

function EntityDetail({ detail, onBack, onFavorite, onOpen, amapApiKey, amapSecurityJsCode }: { detail: GeoEntityDetail; onBack: () => void; onFavorite: () => void; onOpen: (id: string) => void; amapApiKey?: string | null; amapSecurityJsCode?: string | null }) {
  const { entity: item } = detail;
  const parent = parentRelation(detail);
  const cue = learningCue(item);
  const coordinateText = item.coordinates ? `${item.coordinates.latitude.toFixed(2)}° N · ${item.coordinates.longitude.toFixed(2)}° E` : "坐标待补";
  return <section className="geo-detail-panel">
    <header className="geo-detail-head"><button type="button" className="geo-back-button" onClick={onBack}>← 探索首页</button><div className="geo-detail-actions"><button type="button" className={detail.favorite ? "selected" : ""} onClick={onFavorite}><BookmarkSimple size={17} weight={detail.favorite ? "fill" : "regular"} />{detail.favorite ? "已收藏" : "收藏"}</button><span>{TYPE_LABELS[item.entity_type]}</span></div></header>
    <div className="geo-detail-title"><div><span className="geo-eyebrow">{TYPE_LABELS[item.entity_type].toUpperCase()} · {item.coordinates?.system ?? "OFFLINE"}</span><h2>{item.name}</h2><p>{item.name_en}</p></div><div className="geo-coordinate">{coordinateText}</div></div>
    <p className="geo-detail-summary">{item.summary}</p>
    <div className="geo-detail-overview">
      <section className="geo-profile-card">
        <div className="geo-section-label">地理档案 <span>PROFILE</span></div>
        <dl>
          <div><dt>所属层级</dt><dd>{parent ? `${TYPE_LABELS[parent.entity_type]} · ${parent.name}` : item.parent_id ? item.parent_id : "世界 / 独立实体"}</dd></div>
          <div><dt>别名</dt><dd>{item.aliases.length ? item.aliases.join("、") : "暂无记录"}</dd></div>
          <div><dt>坐标</dt><dd>{coordinateText}</dd></div>
          <div><dt>探索关系</dt><dd>{detail.relations.length} 条 · {detail.sources.length} 个来源</dd></div>
        </dl>
      </section>
      <section className="geo-learning-card">
        <div className="geo-section-label">学习线索 <span>LEARNING CUE</span></div>
        <strong>先记住</strong><p>{cue.remember}</p>
        <strong>继续追问</strong><p>{cue.ask}</p>
      </section>
    </div>
    <AmapRegionMap key={item.id} item={item} relations={detail.relations} apiKey={amapApiKey} securityJsCode={amapSecurityJsCode} onSelect={onOpen} />
    <div className="geo-detail-grid"><section><div className="geo-section-label">关键事实</div><div className="geo-facts">{item.properties.map((property) => <div key={property.key}><small>{property.label}</small><strong>{property.value}</strong>{property.unit ? <span>{property.unit}</span> : null}</div>)}</div></section><section className="geo-why-card"><div className="geo-section-label"><Question size={15} /> WHY · 为什么这里是今天的样子</div><p>{item.entity_type === "city" ? `地形、水系与人类活动共同塑造了${item.name}的城市结构。` : item.entity_type === "river" ? `${item.name}把多个地形单元串成一条连续的探索路径。` : `${item.name}不是孤立的事实，它与周围的地形、气候和人类活动共同形成今天的格局。`}</p></section></div>
    <section className="geo-related"><header><div><div className="geo-section-label"><GitBranch size={15} /> RELATIONS</div><h3>沿着关系继续探索</h3></div><span>{detail.relations.length} 条关系</span></header><div className="geo-relation-cards">{detail.relations.map((relation) => <button type="button" key={`${relation.relation.from_id}-${relation.relation.to_id}-${relation.relation.kind}`} onClick={() => onOpen(relation.entity.id)}><span>{relationLabel(relation.relation.kind)}</span><strong>{relation.entity.name}</strong><small>{relation.relation.note ?? "相关地理实体"}</small><ArrowRight size={14} /></button>)}</div></section>
    <section className="geo-sources"><header><span className="geo-section-label">SOURCES</span><span>事实不由 AI 猜测</span></header>{detail.sources.map((source) => <div key={source.id}><strong>{source.dataset}</strong><span>{source.version}</span><span>{source.license}</span><a href={source.url} target="_blank" rel="noreferrer">查看来源 ↗</a></div>)}</section>
  </section>;
}

function KnowledgePage({ onOpenEntity }: { onOpenEntity: (id: string) => void }) {
  const [selectedId, setSelectedId] = useState(LANDFORM_KNOWLEDGE_ENTRIES[0].id);
  const [landformQuery, setLandformQuery] = useState("");
  const selected = LANDFORM_KNOWLEDGE_ENTRIES.find((entry) => entry.id === selectedId) ?? LANDFORM_KNOWLEDGE_ENTRIES[0];
  const visibleEntries = useMemo(() => {
    const query = landformQuery.trim().toLocaleLowerCase();
    if (!query) return LANDFORM_KNOWLEDGE_ENTRIES;
    return LANDFORM_KNOWLEDGE_ENTRIES.filter((entry) => `${entry.title} ${entry.subtitle} ${entry.category} ${entry.keywords.join(" ")}`.toLocaleLowerCase().includes(query));
  }, [landformQuery]);
  const selectedIndex = LANDFORM_KNOWLEDGE_ENTRIES.findIndex((entry) => entry.id === selected.id);
  return <section className="geo-knowledge-page">
    <header className="geo-knowledge-header"><div><span className="geo-eyebrow">GEOGRAPHY KNOWLEDGE · LANDFORMS</span><h2>先认识地貌，再理解地图</h2><p>按主导营力组织主要地貌单元：先观察形态，再理解形成过程和真实案例。</p></div><div className="geo-knowledge-count"><strong>{LANDFORM_KNOWLEDGE_ENTRIES.length}</strong><span>个主要地貌主题</span></div></header>
    <div className="geo-knowledge-layout">
      <nav className="geo-knowledge-index" aria-label="地貌知识目录">
        <div className="geo-knowledge-index-head"><span className="geo-knowledge-index-label">LANDFORM INDEX</span><small>{landformQuery ? `${visibleEntries.length} 个结果` : "按成因浏览"}</small></div>
        <label className="geo-landform-search"><MagnifyingGlass size={14} /><input value={landformQuery} onChange={(event) => setLandformQuery(event.target.value)} placeholder="搜索地貌、成因或关键词" aria-label="搜索地貌" />{landformQuery ? <button type="button" onClick={() => setLandformQuery("")} aria-label="清除地貌搜索"><X size={13} /></button> : null}</label>
        {visibleEntries.length ? visibleEntries.map((entry) => { const index = LANDFORM_KNOWLEDGE_ENTRIES.indexOf(entry); return <button type="button" key={entry.id} className={selected.id === entry.id ? "selected" : ""} onClick={() => setSelectedId(entry.id)}><i>{String(index + 1).padStart(2, "0")}</i><span><strong>{entry.title}</strong><small>{entry.category} · {entry.subtitle}</small></span><ArrowRight size={15} /></button>; }) : <p className="geo-landform-no-results">未找到相关地貌。可尝试“冰川”“海岸”“侵蚀”或“火山”。</p>}
      </nav>
      <article className="geo-knowledge-detail">
        <div className="geo-knowledge-detail-title"><div><span className="geo-card-type">{selected.category}</span><h3>{selected.title}</h3><p>{selected.subtitle}</p></div><span className="geo-knowledge-mark">{String(selectedIndex + 1).padStart(2, "0")} / {String(LANDFORM_KNOWLEDGE_ENTRIES.length).padStart(2, "0")}</span></div>
        <Landform3DViewer title={selected.title} config={LANDFORM_VIEWERS[selected.viewerType]} />
        <div className="geo-knowledge-copy"><section><span>01 · 它是什么</span><p>{selected.intro}</p></section><section><span>02 · 怎么形成</span><p>{selected.formation}</p></section><section><span>03 · 怎么辨认</span><p>{selected.identify}</p></section></div>
        <footer className="geo-knowledge-example"><div><span>典型案例</span><strong>{selected.example}</strong></div><button type="button" onClick={() => onOpenEntity(selected.exampleId)}>打开实体详情 <ArrowRight size={15} /></button></footer>
      </article>
    </div>
  </section>;
}

export function GeographyPage({ active, setNotice, amapApiKey, amapSecurityJsCode, intent }: GeographyPageProps) {
  const [home, setHome] = useState<GeographyHome | null>(null);
  const [query, setQuery] = useState("");
  const [searchGroups, setSearchGroups] = useState<GeoSearchGroup[]>([]);
  const [detail, setDetail] = useState<GeoEntityDetail | null>(null);
  const [layer, setLayer] = useState<GeoMapLayer>("terrain");
  const [view, setView] = useState<"explore" | "knowledge">("explore");

  const loadHome = useCallback(async () => {
    if (!isTauriRuntime()) { setHome(FALLBACK_HOME); return; }
    try { setHome(await invoke<GeographyHome>("geography_home", { cursor: 0 })); } catch (error) { setNotice(errorMessage(error)); }
  }, [setNotice]);
  useEffect(() => { if (active && !home) void loadHome(); }, [active, home, loadHome]);

  const openEntity = useCallback(async (id: string) => {
    if (!isTauriRuntime()) { setDetail(fallbackDetail(id, home?.favorite_ids.includes(id))); return; }
    try { setDetail(await invoke<GeoEntityDetail | null>("geography_detail", { id })); } catch (error) { setNotice(errorMessage(error)); }
  }, [home?.favorite_ids, setNotice]);
  const handleOpenEntity = useCallback((id: string) => { void openEntity(id); }, [openEntity]);

  useEffect(() => {
    if (active && intent?.entityId) void openEntity(intent.entityId);
  }, [active, intent, openEntity]);

  const submitSearch = useCallback(async () => {
    if (!query.trim()) { setSearchGroups([]); return; }
    if (!isTauriRuntime()) { setSearchGroups(fallbackGroups(query)); return; }
    try { setSearchGroups(await invoke<GeoSearchGroup[]>("geography_search", { query, entityType: null, limit: 30 })); } catch (error) { setNotice(errorMessage(error)); }
  }, [query, setNotice]);

  const toggleFavorite = useCallback(async () => {
    if (!detail) return;
    if (!isTauriRuntime()) { setDetail({ ...detail, favorite: !detail.favorite }); setHome((current) => current ? { ...current, favorite_ids: detail.favorite ? current.favorite_ids.filter((id) => id !== detail.entity.id) : [...current.favorite_ids, detail.entity.id] } : current); return; }
    try { const favorite = await invoke<boolean>("geography_toggle_favorite", { id: detail.entity.id }); setDetail({ ...detail, favorite }); setHome((current) => current ? { ...current, favorite_ids: favorite ? [...current.favorite_ids, detail.entity.id] : current.favorite_ids.filter((id) => id !== detail.entity.id) } : current); } catch (error) { setNotice(errorMessage(error)); }
  }, [detail, setNotice]);


  const displayedHome = home ?? FALLBACK_HOME;
  return <div className="page-scroll geography-page">
    <header className="geo-hero"><div><span className="geo-eyebrow">GEOGRAPHY EXPLORER · OFFLINE FIRST</span><h1>探索这个世界，<em>继续追问为什么。</em></h1><p>从一个地点出发，沿着地形、气候、水系、人口与城市之间的关系，读懂它如何成为今天的样子。</p></div><div className="geo-hero-orbit" aria-hidden="true"><span>35.86° N</span><i /><b>104.20° E</b></div></header>
    <nav className="geo-tabs" aria-label="Geography 页面"><button type="button" className={view === "explore" ? "selected" : ""} onClick={() => setView("explore")}><Compass size={16} />Explore</button><button type="button" className={view === "knowledge" ? "selected" : ""} onClick={() => setView("knowledge")}><BookOpen size={16} />知识</button></nav>
    {detail ? <EntityDetail detail={detail} onBack={() => setDetail(null)} onFavorite={() => void toggleFavorite()} onOpen={handleOpenEntity} amapApiKey={amapApiKey} amapSecurityJsCode={amapSecurityJsCode} /> : view === "knowledge" ? <KnowledgePage onOpenEntity={handleOpenEntity} /> : <>
      <div className="geo-search-box"><MagnifyingGlass size={20} /><input value={query} onChange={(event) => setQuery(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") void submitSearch(); }} placeholder="搜索国家、省份、城市、河流、山脉、地貌……" /><button type="button" onClick={() => void submitSearch()}>搜索</button></div>
      {searchGroups.length > 0 ? <section className="geo-search-results"><header><span>SEARCH RESULTS</span><button type="button" onClick={() => setSearchGroups([])}><X size={15} />关闭</button></header>{searchGroups.map((group) => <div key={group.entity_type}><b>{TYPE_LABELS[group.entity_type]}</b>{group.items.map((item) => <button type="button" key={item.id} onClick={() => void openEntity(item.id)}><strong>{item.name}</strong><small>{item.name_en}</small><ArrowRight size={14} /></button>)}</div>)}</section> : null}
      <div className="geo-explore-grid"><section className="geo-discovery-card"><div className="geo-section-label"><Question size={15} /> DAILY DISCOVERY · 今日探索</div><h2>{displayedHome.recommendation.title}</h2><p className="geo-discovery-question">{displayedHome.recommendation.question}</p><div className="geo-tag-row">{displayedHome.recommendation.tags.map((tag) => <span key={tag}>{tag}</span>)}</div><button type="button" className="geo-outline-button" onClick={() => handleOpenEntity(displayedHome.recommendation.entity_id)}>开始探索 <ArrowRight size={16} /></button></section><GeoMap points={displayedHome.map_points} lines={displayedHome.map_lines} layer={layer} onSelect={handleOpenEntity} onLayerChange={setLayer} /></div>
      <section className="geo-feature-section"><header><div><span className="geo-eyebrow">START WITH A PLACE OR A QUESTION</span><h2>继续探索</h2></div><span>精选地理线索</span></header><div className="geo-card-grid">{displayedHome.featured.map((item) => <EntityCard item={item} key={item.id} onOpen={(id) => void openEntity(id)} />)}</div></section>
    </>}
  </div>;
}
