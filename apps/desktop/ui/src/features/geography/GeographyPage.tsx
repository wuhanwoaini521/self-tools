import { invoke } from "@tauri-apps/api/core";
import { ArrowRight, BookmarkSimple, BookOpen, Compass, GitBranch, MagnifyingGlass, Question, X } from "@phosphor-icons/react";
import { lazy, Suspense, useCallback, useEffect, useState } from "react";
import type { GeoEntity, GeoEntityDetail, GeoEntityType, GeoMapLine, GeoMapPoint, GeoSearchGroup, GeographyHome } from "../../types";
import { errorMessage, isTauriRuntime } from "../../utils";
import { AmapRegionMap } from "./AmapRegionMap";
import { GeoMap, type GeoMapLayer } from "./GeoMap";

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

interface GeoKnowledgeEntry {
  id: string;
  category: string;
  title: string;
  subtitle: string;
  intro: string;
  formation: string;
  identify: string;
  example: string;
  exampleId: string;
}

const KNOWLEDGE_ENTRIES: GeoKnowledgeEntry[] = [
  { id: "mountains", category: "构造地貌", title: "山地与山脉", subtitle: "起伏强烈的高地系统", intro: "山地是相对高差大、坡度明显的地表单元；多个山地沿一定方向延伸并相互连接，就形成山脉。", formation: "地壳受挤压发生褶皱、断裂和抬升，是山地形成的常见机制；长期风化、侵蚀又不断雕刻山脊、峡谷和坡面。", identify: "在地图上看等高线，线条密集通常意味着坡度较大；在卫星图上，山地常表现为连续的阴影、山脊和切割深的河谷。", example: "喜马拉雅山脉", exampleId: "himalayas" },
  { id: "plateau", category: "构造地貌", title: "高原", subtitle: "海拔较高的广阔台地", intro: "高原是面积较大、整体海拔较高的地形，表面可以相对平坦，也可能被河流切割成高山峡谷。", formation: "大范围地壳抬升、火山堆积或构造活动会形成高原；抬升后的河流侵蚀，决定了高原边缘和内部的起伏。", identify: "判断高原不能只看一座山的高度，要看一整片区域是否处在较高海拔，并观察边缘是否有陡降或深切河谷。", example: "青藏高原", exampleId: "tibetan-plateau" },
  { id: "basin", category: "构造地貌", title: "盆地", subtitle: "四周较高、中部较低的地形", intro: "盆地像一个巨大的地形容器，四周通常被山地或高地包围，中部相对低平，容易汇集水汽、河流和沉积物。", formation: "地壳沉降、断陷或周缘山地抬升，都可能造成盆地；河流不断搬运沉积物，会进一步填平盆地内部。", identify: "从地形图看，盆地常呈现边缘等高线密集、中心较疏的格局；从区域关系看，要同时寻找周缘高地和内部水系。", example: "四川盆地", exampleId: "sichuan-basin" },
  { id: "plain", category: "流水地貌", title: "平原与冲积平原", subtitle: "地势平坦、沉积物深厚的低地", intro: "平原是起伏小、坡度缓的地貌。河流携带的泥沙在中下游和河口沉积，常形成适合农业、交通和城市发展的冲积平原。", formation: "河流流速降低后，泥沙会在河漫滩、冲积扇、三角洲等位置沉积；海岸线变化也会参与塑造平原。", identify: "平原通常等高线稀疏、河网较密，城市和农田容易连续分布；不要把‘平’误认为没有河流，很多平原正是河流塑造的结果。", example: "东北平原", exampleId: "northeast-china" },
  { id: "river", category: "流水地貌", title: "河流地貌", subtitle: "水流侵蚀、搬运与沉积的连续结果", intro: "河流地貌不是单一形状，而是一套随河流过程变化的地貌组合：上游常见峡谷，中下游常见曲流、河漫滩，入海处可形成三角洲。", formation: "坡降大时，河流以侵蚀为主；坡降变缓后，搬运能力下降，泥沙开始沉积。河流因此把高地、盆地、平原和海洋串成一条空间线索。", identify: "阅读河流要同时看流向、坡度、支流和入海位置；弯曲河道通常意味着侧向侵蚀和沉积正在改变河谷。", example: "长江", exampleId: "yangtze" },
  { id: "island-arc", category: "板块地貌", title: "岛弧与火山地貌", subtitle: "板块俯冲塑造的弧形岛链", intro: "岛弧是沿板块边界呈弧形排列的岛屿或火山岛链，常与深海沟、地震和火山活动共同出现。", formation: "海洋板块俯冲到另一板块之下，部分物质熔融并形成岩浆，长期活动可能构成火山链和岛弧。", identify: "在地图上观察岛链是否呈弧状，并结合海沟、火山和地震带判断其板块背景；日本列岛就是典型案例。", example: "日本列岛", exampleId: "japan" },
];

const KnowledgeTerrainMap = lazy(() => import("./MapLibreMap").then((module) => ({ default: module.MapLibreMap })));

const KNOWLEDGE_CAMERA: Record<string, { center: [number, number]; zoom: number }> = {
  mountains: { center: [84, 29], zoom: 4.8 },
  plateau: { center: [88, 33], zoom: 4.4 },
  basin: { center: [105.5, 30.5], zoom: 5.7 },
  plain: { center: [126.6, 45.7], zoom: 5.1 },
  river: { center: [112, 30], zoom: 4.8 },
  "island-arc": { center: [138.25, 36.2], zoom: 5.1 },
};

function KnowledgeTerrainPreview({ entry, onOpenEntity }: { entry: GeoKnowledgeEntry; onOpenEntity: (id: string) => void }) {
  const example = FALLBACK_ENTITIES.find((item) => item.id === entry.exampleId);
  const camera = KNOWLEDGE_CAMERA[entry.id] ?? { center: [105, 32] as [number, number], zoom: 4.5 };
  const points = example?.coordinates ? [{ entity_id: example.id, name: example.name, entity_type: example.entity_type, coordinate: example.coordinates }] : [];
  return <section className="geo-knowledge-visual" aria-label={`${entry.title}真实地形预览`}>
    <header><div><span>REAL TERRAIN PREVIEW</span><strong>{entry.example}</strong></div><small>Mapterhorn DEM · 可旋转</small></header>
    <div className="geo-knowledge-map">
      <Suspense fallback={<div className="geo-knowledge-map-loading">正在载入真实地形…</div>}>
        <KnowledgeTerrainMap key={entry.id} points={points} lines={[]} layer="terrain" onSelect={onOpenEntity} onError={() => undefined} center={camera.center} zoom={camera.zoom} compact />
      </Suspense>
    </div>
    <p>把视角倾斜后观察高差：山地看山脊，高原看整体抬升，盆地看周缘高地与中心低地。</p>
  </section>;
}

function KnowledgePage({ onOpenEntity }: { onOpenEntity: (id: string) => void }) {
  const [selectedId, setSelectedId] = useState(KNOWLEDGE_ENTRIES[0].id);
  const selected = KNOWLEDGE_ENTRIES.find((entry) => entry.id === selectedId) ?? KNOWLEDGE_ENTRIES[0];
  return <section className="geo-knowledge-page">
    <header className="geo-knowledge-header"><div><span className="geo-eyebrow">GEOGRAPHY KNOWLEDGE · LANDFORMS</span><h2>先认识地貌，再理解地图</h2><p>地貌不是需要孤立背诵的名词。看它怎么形成、如何辨认，再把它和气候、水系、城市联系起来。</p></div><div className="geo-knowledge-count"><strong>{KNOWLEDGE_ENTRIES.length}</strong><span>个基础地貌主题</span></div></header>
    <div className="geo-knowledge-layout">
      <nav className="geo-knowledge-index" aria-label="地貌知识目录">
        <span className="geo-knowledge-index-label">LANDFORM INDEX</span>
        {KNOWLEDGE_ENTRIES.map((entry, index) => <button type="button" key={entry.id} className={selected.id === entry.id ? "selected" : ""} onClick={() => setSelectedId(entry.id)}><i>{String(index + 1).padStart(2, "0")}</i><span><strong>{entry.title}</strong><small>{entry.subtitle}</small></span><ArrowRight size={15} /></button>)}
      </nav>
      <article className="geo-knowledge-detail">
        <div className="geo-knowledge-detail-title"><div><span className="geo-card-type">{selected.category}</span><h3>{selected.title}</h3><p>{selected.subtitle}</p></div><span className="geo-knowledge-mark">{String(KNOWLEDGE_ENTRIES.indexOf(selected) + 1).padStart(2, "0")} / {String(KNOWLEDGE_ENTRIES.length).padStart(2, "0")}</span></div>
        <KnowledgeTerrainPreview entry={selected} onOpenEntity={onOpenEntity} />
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
