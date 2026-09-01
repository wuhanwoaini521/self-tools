import { invoke } from "@tauri-apps/api/core";
import { ArrowRight, ArrowsLeftRight, BookmarkSimple, Compass, GitBranch, MagnifyingGlass, MapTrifold, Question, X } from "@phosphor-icons/react";
import { useCallback, useEffect, useMemo, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import type { GeoCompareView, GeoEntity, GeoEntityDetail, GeoEntityType, GeoMapLine, GeoMapPoint, GeoSearchGroup, GeographyHome } from "../../types";
import { errorMessage, isTauriRuntime } from "../../utils";
import { GeoMap, type GeoMapLayer } from "./GeoMap";

interface GeographyPageProps { active: boolean; setNotice: (notice: string) => void; amapApiKey?: string | null; amapSecurityJsCode?: string | null; intent?: { entityId: string; nonce: number } | null; }

const TYPE_LABELS: Record<GeoEntityType, string> = {
  world: "世界", country: "国家", region: "区域", province: "省级行政区", city: "城市", river: "河流", mountain: "山体", mountain_range: "山脉", plateau: "高原", plain: "平原", basin: "盆地", desert: "沙漠", lake: "湖泊", ocean: "海洋", sea: "海域", island: "岛屿", archipelago: "群岛", climate_zone: "气候区", tectonic_plate: "构造板块",
};

function entity(id: string, entityType: GeoEntityType, name: string, nameEn: string, summary: string, coordinates: [number, number], properties: [string, string, string | null][]): GeoEntity {
  return { id, entity_type: entityType, name, name_en: nameEn, aliases: [], coordinates: { system: "WGS84", latitude: coordinates[0], longitude: coordinates[1] }, geometry: null, parent_id: null, summary, properties: properties.map(([key, value, unit]) => ({ key, label: key, value, unit, source_ids: ["geography-seed"] })), source_ids: ["geography-seed"] };
}

const FALLBACK_ENTITIES: GeoEntity[] = [
  entity("china", "country", "中国", "China", "季风、水系与人口城市分布彼此相连。", [35.86, 104.2], [["面积", "9,596,960", "km²"], ["人口", "1,410,710,000", "人 · 2023"]]),
  entity("japan", "country", "日本", "Japan", "板块边界、山地与沿海平原共同塑造岛国。", [36.2, 138.25], [["面积", "377,975", "km²"], ["人口", "124,516,000", "人 · 2023"]]),
  entity("sichuan", "province", "四川", "Sichuan", "盆地、山地、河流与季风共同解释这里的自然格局。", [30.65, 102.7], [["地形", "四川盆地与周缘山地", null], ["气候", "湿润、多云", null]]),
  entity("chengdu", "city", "成都", "Chengdu", "盆地西部的平原城市，农业基础支撑了长期发展。", [30.57, 104.07], [["海拔", "约 500", "m"], ["气候", "湿润季风，多云雾", null]]),
  entity("chongqing", "city", "重庆", "Chongqing", "长江与嘉陵江交汇的山地河谷城市。", [29.56, 106.55], [["地形", "山地与河谷", null]]),
  entity("yangtze", "river", "长江", "Yangtze River", "连接高原、盆地、平原和海洋的一条探索主线。", [30, 112], [["长度", "约 6,300", "km"], ["源头", "青藏高原腹地", null]]),
  entity("tibetan-plateau", "plateau", "青藏高原", "Tibetan Plateau", "印度板块与欧亚板块碰撞塑造的高亚洲地形核心。", [33, 88], [["平均海拔", "超过 4,000", "m"], ["气候", "高寒", null]]),
  entity("sichuan-basin", "basin", "四川盆地", "Sichuan Basin", "周缘山地限制气流交换，水汽与云雾更易积聚。", [30.5, 105.5], [["气候", "湿润、多云雾", null], ["人口关系", "平原与水系支持聚居", null]]),
  entity("himalayas", "mountain_range", "喜马拉雅山脉", "Himalayas", "板块碰撞的地表结果之一，也是亚洲重要气候屏障。", [28, 84], [["最高峰", "珠穆朗玛峰约 8,849", "m"], ["形成", "印度板块与欧亚板块碰撞", null]]),
  entity("qinling", "mountain_range", "秦岭", "Qinling Mountains", "中国南北气候、植被和水系差异的重要分界。", [33.8, 108], [["气候影响", "南北气候分界", null]]),
  entity("shanghai", "city", "上海", "Shanghai", "长江三角洲冲积平原上的河口城市。", [31.23, 121.47], [["水系", "长江入海口", null]]),
  entity("northeast-china", "region", "中国东北", "Northeast China", "东北平原、黑土地与寒冷冬季共同塑造农业与人口分布。", [45.7, 126.6], [["气候", "温带季风，冬季寒冷", null]]),
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

interface GeoMedia {
  imageUrl: string;
  sourceUrl: string;
  credit: string;
  terrain: boolean;
}

const WIKIMEDIA_FILE = "https://commons.wikimedia.org/wiki/Special:FilePath/";

const GEO_MEDIA: Record<string, GeoMedia> = {
  world: { imageUrl: `${WIKIMEDIA_FILE}ISS026-E-37436%20-%20View%20of%20Earth.jpg?width=1400`, sourceUrl: "https://commons.wikimedia.org/wiki/File:ISS026-E-37436_-_View_of_Earth.jpg", credit: "NASA/JSC · Wikimedia Commons", terrain: false },
  china: { imageUrl: `${WIKIMEDIA_FILE}China%20from%20space.png?width=1400`, sourceUrl: "https://commons.wikimedia.org/wiki/File:China_from_space.png", credit: "Przemek Pietrak · CC BY 3.0", terrain: false },
  japan: { imageUrl: `${WIKIMEDIA_FILE}Japan%20satellite.jpg?width=1400`, sourceUrl: "https://commons.wikimedia.org/wiki/File:Japan_satellite.jpg", credit: "NASA · Public domain", terrain: false },
  sichuan: { imageUrl: `${WIKIMEDIA_FILE}Sichuan%20Basin%2C%20China%20%28ASTER%29.jpg?width=1400`, sourceUrl: "https://commons.wikimedia.org/wiki/File:Sichuan_Basin,_China_(ASTER).jpg", credit: "NASA/JPL ASTER · Wikimedia Commons", terrain: true },
  chengdu: { imageUrl: `${WIKIMEDIA_FILE}Skyline%20of%20Chengdu.jpg?width=1400`, sourceUrl: "https://commons.wikimedia.org/wiki/File:Skyline_of_Chengdu.jpg", credit: "Xiongyao · CC BY-SA 3.0", terrain: false },
  chongqing: { imageUrl: `${WIKIMEDIA_FILE}SkylineOfChongqing.jpg?width=1400`, sourceUrl: "https://commons.wikimedia.org/wiki/File:SkylineOfChongqing.jpg", credit: "Oliver Ren · Wikimedia Commons", terrain: false },
  shanghai: { imageUrl: `${WIKIMEDIA_FILE}Shanghai%20skyline.jpg?width=1400`, sourceUrl: "https://commons.wikimedia.org/wiki/File:Shanghai_skyline.jpg", credit: "Rose Abrams · CC BY 4.0", terrain: false },
  yangtze: { imageUrl: `${WIKIMEDIA_FILE}Yangtze%20River%20in%20Yunnan%2001.JPG?width=1400`, sourceUrl: "https://commons.wikimedia.org/wiki/File:Yangtze_River_in_Yunnan_01.JPG", credit: "Wikimedia Commons · 见原图许可", terrain: false },
  "tibetan-plateau": { imageUrl: `${WIKIMEDIA_FILE}The%20Tibetan%20Plateau.jpg?width=1400`, sourceUrl: "https://commons.wikimedia.org/wiki/File:The_Tibetan_Plateau.jpg", credit: "ESA Envisat · CC BY-SA 3.0 IGO", terrain: true },
  "sichuan-basin": { imageUrl: `${WIKIMEDIA_FILE}Sichuan%20Basin%2C%20China%20%28ASTER%29.jpg?width=1400`, sourceUrl: "https://commons.wikimedia.org/wiki/File:Sichuan_Basin,_China_(ASTER).jpg", credit: "NASA/JPL ASTER · Wikimedia Commons", terrain: true },
  himalayas: { imageUrl: `${WIKIMEDIA_FILE}Himalayas.jpg?width=1400`, sourceUrl: "https://commons.wikimedia.org/wiki/File:Himalayas.jpg", credit: "NASA · Wikimedia Commons", terrain: true },
  qinling: { imageUrl: `${WIKIMEDIA_FILE}Part%20of%20Qinling%20mountains.jpg?width=1400`, sourceUrl: "https://commons.wikimedia.org/wiki/File:Part_of_Qinling_mountains.jpg", credit: "Charlie439753 · CC BY-SA 4.0", terrain: true },
  "northeast-china": { imageUrl: `${WIKIMEDIA_FILE}Northeast%20China%20Plain%20Deciduous%20Forests%20Ecoregion.png?width=1400`, sourceUrl: "https://commons.wikimedia.org/wiki/File:Northeast_China_Plain_Deciduous_Forests_Ecoregion.png", credit: "Z3lvs · CC BY-SA 4.0", terrain: true },
  "japan-plate": { imageUrl: `${WIKIMEDIA_FILE}The%20Pacific%20Ocean-%20Viewed%20from%20Space.png?width=1400`, sourceUrl: "https://commons.wikimedia.org/wiki/File:The_Pacific_Ocean-_Viewed_from_Space.png", credit: "NASA · Public domain", terrain: true },
};

const GEO_MEDIA_BY_TYPE: Partial<Record<GeoEntityType, GeoMedia>> = {
  world: GEO_MEDIA.world,
  country: GEO_MEDIA.china,
  province: GEO_MEDIA.sichuan,
  region: GEO_MEDIA["northeast-china"],
  city: GEO_MEDIA.chengdu,
  river: GEO_MEDIA.yangtze,
  mountain_range: GEO_MEDIA.himalayas,
  plateau: GEO_MEDIA["tibetan-plateau"],
  basin: GEO_MEDIA["sichuan-basin"],
  tectonic_plate: GEO_MEDIA["japan-plate"],
};

const TERRAIN_TYPES: GeoEntityType[] = ["mountain", "mountain_range", "plateau", "plain", "basin", "desert", "lake", "ocean", "sea", "island", "archipelago", "climate_zone", "tectonic_plate"];

function mediaFor(item: GeoEntity): GeoMedia {
  return GEO_MEDIA[item.id] ?? GEO_MEDIA_BY_TYPE[item.entity_type] ?? {
    imageUrl: `${WIKIMEDIA_FILE}ISS026-E-37436%20-%20View%20of%20Earth.jpg?width=1400`,
    sourceUrl: "https://commons.wikimedia.org/wiki/File:ISS026-E-37436_-_View_of_Earth.jpg",
    credit: "NASA/JSC · Wikimedia Commons",
    terrain: TERRAIN_TYPES.includes(item.entity_type),
  };
}

function terrainVariant(item: GeoEntity) {
  if (item.entity_type === "basin") return "basin";
  if (["mountain", "mountain_range", "tectonic_plate"].includes(item.entity_type)) return "ridge";
  return "plateau";
}

function TerrainPreview({ item }: { item: GeoEntity }) {
  const [rotation, setRotation] = useState({ x: 45, z: -3 });
  const dragRef = useRef<{ pointerId: number; startX: number; startY: number; startXRotation: number; startZRotation: number } | null>(null);
  const variant = terrainVariant(item);

  const startRotate = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    dragRef.current = { pointerId: event.pointerId, startX: event.clientX, startY: event.clientY, startXRotation: rotation.x, startZRotation: rotation.z };
    event.currentTarget.setPointerCapture(event.pointerId);
  };
  const rotate = (event: ReactPointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    setRotation({ x: Math.max(20, Math.min(75, drag.startXRotation - (event.clientY - drag.startY) * .35)), z: drag.startZRotation + (event.clientX - drag.startX) * .35 });
  };
  const endRotate = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (dragRef.current?.pointerId !== event.pointerId) return;
    dragRef.current = null;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
  };

  return <div className="geo-terrain-preview" aria-label={`${item.name} 3D 地形示意`}>
    <div className="geo-terrain-preview-head"><span>3D TERRAIN</span><small>按住拖动旋转</small></div>
    <div className={`geo-terrain-model ${variant}`} onPointerDown={startRotate} onPointerMove={rotate} onPointerUp={endRotate} onPointerCancel={endRotate}>
      <svg viewBox="0 0 320 190" role="img" aria-label={`${item.name} 的等高线与起伏地形示意`} style={{ transform: `rotateX(${rotation.x}deg) rotateZ(${rotation.z}deg) scale(1.08)` }}>
        <defs>
          <linearGradient id="terrain-top" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stopColor="#e5c47a" /><stop offset="1" stopColor="#7b9f72" /></linearGradient>
          <linearGradient id="terrain-side" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stopColor="#9b774d" /><stop offset="1" stopColor="#3b5351" /></linearGradient>
          <radialGradient id="terrain-glow"><stop offset="0" stopColor="#f7e2a6" stopOpacity=".85" /><stop offset="1" stopColor="#7b9f72" stopOpacity="0" /></radialGradient>
        </defs>
        <ellipse cx="158" cy="112" rx="112" ry="38" fill="url(#terrain-glow)" opacity=".6" />
        <g className="geo-terrain-mesh">
          {variant === "ridge" ? <>
            <path className="geo-terrain-side" d="M54 108 L83 83 L112 94 L142 48 L171 82 L205 59 L249 93 L215 147 L118 159 Z" />
            <path className="geo-terrain-top" d="M54 108 L83 83 L112 94 L142 48 L171 82 L205 59 L249 93 L181 126 Z" fill="url(#terrain-top)" />
            <path className="geo-terrain-top geo-terrain-top-2" d="M181 126 L249 93 L215 124 L161 153 Z" fill="#6e936f" />
          </> : variant === "basin" ? <>
            <path className="geo-terrain-side" d="M61 91 L115 54 L247 75 L217 140 L117 159 Z" />
            <path className="geo-terrain-top" d="M61 91 L115 54 L247 75 L181 116 Z" fill="url(#terrain-top)" />
            <path className="geo-terrain-top geo-terrain-top-2" d="M181 116 L247 75 L217 108 L160 142 Z" fill="#6e936f" />
          </> : <>
            <path className="geo-terrain-side" d="M63 100 L118 58 L245 81 L212 143 L117 160 Z" />
            <path className="geo-terrain-top" d="M63 100 L118 58 L245 81 L181 122 Z" fill="url(#terrain-top)" />
            <path className="geo-terrain-top geo-terrain-top-2" d="M181 122 L245 81 L212 112 L161 145 Z" fill="#6e936f" />
          </>}
          <path className="geo-terrain-contour-3d" d="M86 99 C115 85 146 85 178 94 C202 101 221 96 231 87" />
          <path className="geo-terrain-contour-3d" d="M102 111 C128 98 153 99 179 106 C196 111 209 107 219 101" />
          <path className="geo-terrain-contour-3d" d="M118 123 C139 113 157 113 177 118 C188 121 198 119 204 115" />
          <path className="geo-terrain-contour-3d" d="M134 135 C148 128 161 127 176 131" />
          <path className="geo-terrain-river-3d" d="M148 68 C161 80 165 91 159 103 C153 115 160 127 173 139" />
        </g>
      </svg>
    </div>
    <p>拖动旋转模型；地形形态按山脉、盆地、高原做了不同示意。</p>
  </div>;
}

function GeoMediaCard({ item }: { item: GeoEntity }) {
  const media = mediaFor(item);
  const [imageFailed, setImageFailed] = useState(false);
  return <section className={`geo-media-card${media.terrain ? " terrain" : ""}`} aria-label={`${item.name} 配图`}>
    <div className="geo-media-image">
      {imageFailed ? <div className="geo-media-fallback"><span>{TYPE_LABELS[item.entity_type]}</span><strong>{item.name}</strong><small>网络图片暂不可用，仍可查看本地地形示意</small></div> : <img src={media.imageUrl} alt={`${item.name} 图片`} loading="lazy" onError={() => setImageFailed(true)} />}
      <div className="geo-media-caption"><span>{media.credit}</span><a href={media.sourceUrl} target="_blank" rel="noreferrer">查看原图 ↗</a></div>
    </div>
    {media.terrain ? <TerrainPreview item={item} /> : null}
  </section>;
}

function fallbackCompare(left: GeoEntity, right: GeoEntity): GeoCompareView {
  const keys: [string, string, string | null][] = [["面积", "area", ""], ["人口", "population", ""], ["人口密度", "population_density", ""], ["海拔", "elevation", ""], ["地形", "terrain", ""], ["气候", "climate", ""], ["主要城市", "major_cities", ""]];
  return { left, right, metrics: keys.map(([label, key, unit]) => ({ label, key, left: propertyValue(left, label) ?? propertyValue(left, key), right: propertyValue(right, label) ?? propertyValue(right, key), unit })), explanation: `${left.name}与${right.name}的差异，来自地形、纬度和水系条件；先对照事实，再沿关系继续探索。` };
}

function EntityCard({ item, onOpen }: { item: GeoEntity; onOpen: (id: string) => void }) {
  return <button type="button" className="geo-entity-card" onClick={() => onOpen(item.id)}><span className="geo-card-type">{TYPE_LABELS[item.entity_type]}</span><strong>{item.name}</strong><small>{item.name_en}</small><p>{item.summary}</p><ArrowRight size={15} /></button>;
}

function EntityDetail({ detail, onBack, onFavorite, onOpen }: { detail: GeoEntityDetail; onBack: () => void; onFavorite: () => void; onOpen: (id: string) => void }) {
  const { entity: item } = detail;
  return <section className="geo-detail-panel">
    <header className="geo-detail-head"><button type="button" className="geo-back-button" onClick={onBack}>← 探索首页</button><div className="geo-detail-actions"><button type="button" className={detail.favorite ? "selected" : ""} onClick={onFavorite}><BookmarkSimple size={17} weight={detail.favorite ? "fill" : "regular"} />{detail.favorite ? "已收藏" : "收藏"}</button><span>{TYPE_LABELS[item.entity_type]}</span></div></header>
    <div className="geo-detail-title"><div><span className="geo-eyebrow">{TYPE_LABELS[item.entity_type].toUpperCase()} · {item.coordinates?.system ?? "OFFLINE"}</span><h2>{item.name}</h2><p>{item.name_en}</p></div><div className="geo-coordinate">{item.coordinates ? `${item.coordinates.latitude.toFixed(2)}° N · ${item.coordinates.longitude.toFixed(2)}° E` : "坐标待补"}</div></div>
    <p className="geo-detail-summary">{item.summary}</p>
    <GeoMediaCard key={item.id} item={item} />
    <div className="geo-detail-grid"><section><div className="geo-section-label">关键事实</div><div className="geo-facts">{item.properties.map((property) => <div key={property.key}><small>{property.label}</small><strong>{property.value}</strong>{property.unit ? <span>{property.unit}</span> : null}</div>)}</div></section><section className="geo-why-card"><div className="geo-section-label"><Question size={15} /> WHY · 为什么这里是今天的样子</div><p>{item.entity_type === "city" ? `地形、水系与人类活动共同塑造了${item.name}的城市结构。` : item.entity_type === "river" ? `${item.name}把多个地形单元串成一条连续的探索路径。` : `${item.name}不是孤立的事实，它与周围的地形、气候和人类活动共同形成今天的格局。`}</p></section></div>
    <section className="geo-related"><header><div><div className="geo-section-label"><GitBranch size={15} /> RELATIONS</div><h3>沿着关系继续探索</h3></div><span>{detail.relations.length} 条关系</span></header><div className="geo-relation-cards">{detail.relations.map((relation) => <button type="button" key={`${relation.relation.from_id}-${relation.relation.to_id}-${relation.relation.kind}`} onClick={() => onOpen(relation.entity.id)}><span>{relation.relation.kind}</span><strong>{relation.entity.name}</strong><small>{relation.relation.note ?? "相关地理实体"}</small><ArrowRight size={14} /></button>)}</div></section>
    <section className="geo-sources"><header><span className="geo-section-label">SOURCES</span><span>事实不由 AI 猜测</span></header>{detail.sources.map((source) => <div key={source.id}><strong>{source.dataset}</strong><span>{source.version}</span><span>{source.license}</span><a href={source.url} target="_blank" rel="noreferrer">查看来源 ↗</a></div>)}</section>
  </section>;
}

function ComparePanel({ items, onCompare, comparison, leftId, rightId, onLeftChange, onRightChange }: { items: GeoEntity[]; onCompare: () => void; comparison: GeoCompareView | null; leftId: string; rightId: string; onLeftChange: (id: string) => void; onRightChange: (id: string) => void }) {
  return <section className="geo-compare-panel"><header><div><span className="geo-eyebrow">COMPARE / 事实对照</span><h2>比较不是排名，是理解差异</h2><p>从面积和人口开始，再回到地形、气候与水系。</p></div><ArrowsLeftRight size={30} /></header><div className="geo-compare-selectors"><label>左侧实体<select value={leftId} onChange={(event) => onLeftChange(event.target.value)}>{items.map((item) => <option key={item.id} value={item.id}>{item.name} · {TYPE_LABELS[item.entity_type]}</option>)}</select></label><span>VS</span><label>右侧实体<select value={rightId} onChange={(event) => onRightChange(event.target.value)}>{items.map((item) => <option key={item.id} value={item.id}>{item.name} · {TYPE_LABELS[item.entity_type]}</option>)}</select></label><button type="button" className="geo-primary-button" onClick={onCompare}>生成对照</button></div>{comparison ? <div className="geo-comparison-result"><div className="geo-compare-names"><strong>{comparison.left.name}</strong><span>对照</span><strong>{comparison.right.name}</strong></div><div className="geo-metric-list">{comparison.metrics.map((metric) => <div key={metric.key}><span>{metric.label}</span><strong>{metric.left ?? "—"}</strong><i>{metric.unit}</i><strong>{metric.right ?? "—"}</strong></div>)}</div><div className="geo-compare-explanation"><Question size={18} /><p>{comparison.explanation}</p></div></div> : <p className="geo-compare-empty">选择两个同类型实体，生成一张带解释的事实对照卡。</p>}</section>;
}

export function GeographyPage({ active, setNotice, amapApiKey, amapSecurityJsCode, intent }: GeographyPageProps) {
  const [home, setHome] = useState<GeographyHome | null>(null);
  const [query, setQuery] = useState("");
  const [searchGroups, setSearchGroups] = useState<GeoSearchGroup[]>([]);
  const [detail, setDetail] = useState<GeoEntityDetail | null>(null);
  const [layer, setLayer] = useState<GeoMapLayer>("terrain");
  const [view, setView] = useState<"explore" | "compare" | "library">("explore");
  const [leftId, setLeftId] = useState("china");
  const [rightId, setRightId] = useState("japan");
  const [comparison, setComparison] = useState<GeoCompareView | null>(null);

  const loadHome = useCallback(async () => {
    if (!isTauriRuntime()) { setHome(FALLBACK_HOME); return; }
    try { setHome(await invoke<GeographyHome>("geography_home", { cursor: 0 })); } catch (error) { setNotice(errorMessage(error)); }
  }, [setNotice]);
  useEffect(() => { if (active && !home) void loadHome(); }, [active, home, loadHome]);

  const openEntity = useCallback(async (id: string) => {
    if (!isTauriRuntime()) { setDetail(fallbackDetail(id, home?.favorite_ids.includes(id))); return; }
    try { setDetail(await invoke<GeoEntityDetail | null>("geography_detail", { id })); } catch (error) { setNotice(errorMessage(error)); }
  }, [home?.favorite_ids, setNotice]);

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

  const compareItems = useMemo(() => { const all = [...(home?.featured ?? []), ...(home?.recent ?? []), ...FALLBACK_ENTITIES]; return [...new Map(all.map((item) => [item.id, item])).values()]; }, [home]);
  const compare = useCallback(async () => {
    if (!leftId || !rightId || leftId === rightId) { setNotice("请选择两个不同的实体"); return; }
    if (!isTauriRuntime()) { const left = compareItems.find((item) => item.id === leftId); const right = compareItems.find((item) => item.id === rightId); if (left && right && left.entity_type === right.entity_type) setComparison(fallbackCompare(left, right)); else setNotice("V1 比较要求两个同类型实体"); return; }
    try { setComparison(await invoke<GeoCompareView>("geography_compare", { leftId, rightId })); } catch (error) { setNotice(errorMessage(error)); }
  }, [compareItems, leftId, rightId, setNotice]);

  const displayedHome = home ?? FALLBACK_HOME;
  return <div className="page-scroll geography-page">
    <header className="geo-hero"><div><span className="geo-eyebrow">GEOGRAPHY EXPLORER · OFFLINE FIRST</span><h1>探索这个世界，<em>继续追问为什么。</em></h1><p>从一个地点出发，沿着地形、气候、水系、人口与城市之间的关系，读懂它如何成为今天的样子。</p></div><div className="geo-hero-orbit" aria-hidden="true"><span>35.86° N</span><i /><b>104.20° E</b></div></header>
    <nav className="geo-tabs" aria-label="Geography 页面"><button type="button" className={view === "explore" ? "selected" : ""} onClick={() => setView("explore")}><Compass size={16} />Explore</button><button type="button" className={view === "compare" ? "selected" : ""} onClick={() => setView("compare")}><ArrowsLeftRight size={16} />Compare</button><button type="button" className={view === "library" ? "selected" : ""} onClick={() => setView("library")}><BookmarkSimple size={16} />Library <small>{displayedHome.favorite_ids.length}</small></button></nav>
    {detail ? <EntityDetail detail={detail} onBack={() => setDetail(null)} onFavorite={() => void toggleFavorite()} onOpen={(id) => void openEntity(id)} /> : view === "compare" ? <ComparePanel items={compareItems} onCompare={() => void compare()} comparison={comparison} leftId={leftId} rightId={rightId} onLeftChange={setLeftId} onRightChange={setRightId} /> : <>
      <div className="geo-search-box"><MagnifyingGlass size={20} /><input value={query} onChange={(event) => setQuery(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") void submitSearch(); }} placeholder="搜索国家、省份、城市、河流、山脉、地貌……" /><button type="button" onClick={() => void submitSearch()}>搜索</button></div>
      {searchGroups.length > 0 ? <section className="geo-search-results"><header><span>SEARCH RESULTS</span><button type="button" onClick={() => setSearchGroups([])}><X size={15} />关闭</button></header>{searchGroups.map((group) => <div key={group.entity_type}><b>{TYPE_LABELS[group.entity_type]}</b>{group.items.map((item) => <button type="button" key={item.id} onClick={() => void openEntity(item.id)}><strong>{item.name}</strong><small>{item.name_en}</small><ArrowRight size={14} /></button>)}</div>)}</section> : null}
      <div className="geo-explore-grid"><section className="geo-discovery-card"><div className="geo-section-label"><Question size={15} /> DAILY DISCOVERY · 今日探索</div><h2>{displayedHome.recommendation.title}</h2><p className="geo-discovery-question">{displayedHome.recommendation.question}</p><div className="geo-tag-row">{displayedHome.recommendation.tags.map((tag) => <span key={tag}>{tag}</span>)}</div><button type="button" className="geo-outline-button" onClick={() => void openEntity(displayedHome.recommendation.entity_id)}>开始探索 <ArrowRight size={16} /></button></section><GeoMap points={displayedHome.map_points} lines={displayedHome.map_lines} layer={layer} amapApiKey={amapApiKey} amapSecurityJsCode={amapSecurityJsCode} selectedId={null} onSelect={(id) => void openEntity(id)} onLayerChange={setLayer} /></div>
      <section className="geo-feature-section"><header><div><span className="geo-eyebrow">START WITH A PLACE OR A QUESTION</span><h2>继续探索</h2></div><span>精选地理线索</span></header><div className="geo-card-grid">{displayedHome.featured.map((item) => <EntityCard item={item} key={item.id} onOpen={(id) => void openEntity(id)} />)}</div></section>
      {view === "library" ? <section className="geo-library-strip"><MapTrifold size={20} /><div><strong>你的探索记录</strong><p>{displayedHome.recent.length ? displayedHome.recent.map((item) => item.name).join(" · ") : "还没有最近探索；点击地图上的实体开始。"}</p></div></section> : null}
    </>}
  </div>;
}
