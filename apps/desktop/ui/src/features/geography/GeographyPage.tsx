import {
  ArrowClockwise,
  ArrowRight,
  BookmarkSimple,
  BookOpen,
  CaretLeft,
  CaretRight,
  Compass,
  GitBranch,
  MagnetStraight,
  MapPin,
  MapTrifold,
  MagnifyingGlass,
  Question,
  X,
} from "@phosphor-icons/react";
import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type {
  GeoEntity,
  GeoEntityDetail,
  GeoEntityType,
  GeoMapLine,
  GeoMapPoint,
  GeoSearchGroup,
  GeographyHome,
} from "../../types";
import { errorMessage } from "../../utils";
import { geographyClient } from "./geographyClient";
import { explorationClient, type LocationHistoryContext } from "../exploration/explorationClient";
import type { SemanticEventDetail } from "../history/semanticTypes";
import { learningClient } from "../learning/learningClient";
import { SaveToCollection } from "../learning/SaveToCollection";
import type { AppContextPayload } from "../ai/aiTypes";
import { AmapRegionMap } from "./AmapRegionMap";
import { LocationHistoryTimeline } from "./LocationHistoryTimeline";
import { reviewedLocationEvents } from "./geoTimeline";
import { GeoMap, type GeoMapLayer } from "./GeoMap";
import { Landform3DViewer } from "./Landform3DViewer";
import { LANDFORM_KNOWLEDGE_ENTRIES } from "./landformKnowledgeData";
import { LANDFORM_VIEWERS } from "./landform3dData";
import { getRealTerrain } from "./realTerrainData";
import { ViewerErrorBoundary } from "./ViewerErrorBoundary";
import "./geography-explore.css";

const RealTerrainViewer = lazy(() =>
  import("./RealTerrainViewer").then((module) => ({
    default: module.RealTerrainViewer,
  })),
);

type GeographyHistorySelection = {
  eventId: string;
  storyId: string | null;
  year: number | null;
};

interface GeographyPageProps {
  active: boolean;
  setNotice: (notice: string) => void;
  intent?: { entityId?: string; query?: string; nonce: number } | null;
  onAddToTravel?: (entity: GeoEntity, event?: SemanticEventDetail) => void;
  onOpenHistoryEvent?: (eventId: string, location: { id: string; name: string }) => void;
  onOpenHistoryStory?: (storyId: string, location: { id: string; name: string }) => void;
  onReturnToTravel?: () => void;
  onReturnToHistory?: () => void;
}

const TYPE_LABELS: Record<GeoEntityType, string> = {
  world: "世界",
  country: "国家",
  region: "区域",
  province: "省级行政区",
  city: "城市",
  river: "河流",
  mountain: "山体",
  mountain_range: "山脉",
  plateau: "高原",
  plain: "平原",
  basin: "盆地",
  desert: "沙漠",
  lake: "湖泊",
  ocean: "海洋",
  sea: "海域",
  island: "岛屿",
  archipelago: "群岛",
  climate_zone: "气候区",
  tectonic_plate: "构造板块",
};

function entity(
  id: string,
  entityType: GeoEntityType,
  name: string,
  nameEn: string,
  summary: string,
  coordinates: [number, number],
  properties: [string, string, string | null][],
): GeoEntity {
  return {
    id,
    entity_type: entityType,
    name,
    name_en: nameEn,
    aliases: [],
    coordinates: {
      system: "WGS84",
      latitude: coordinates[0],
      longitude: coordinates[1],
    },
    geometry: null,
    parent_id: null,
    summary,
    properties: properties.map(([key, value, unit]) => ({
      key,
      label: key,
      value,
      unit,
      source_ids: ["geography-seed"],
    })),
    source_ids: ["geography-seed"],
  };
}

const FALLBACK_ENTITIES: GeoEntity[] = [
  entity(
    "china",
    "country",
    "中国",
    "China",
    "季风、水系与人口城市分布彼此相连。",
    [35.86, 104.2],
    [
      ["面积", "9,596,960", "km²"],
      ["人口", "1,410,710,000", "人 · 2023"],
      ["人口密度", "147", "人/km² · 2023"],
      ["地形", "青藏高原—高原盆地—平原丘陵", null],
      ["气候", "季风气候为主，南北东西差异显著", null],
      ["主要城市", "北京、上海、成都、重庆、武汉", null],
    ],
  ),
  entity(
    "japan",
    "country",
    "日本",
    "Japan",
    "板块边界、山地与沿海平原共同塑造岛国。",
    [36.2, 138.25],
    [
      ["面积", "377,975", "km²"],
      ["人口", "124,516,000", "人 · 2023"],
      ["人口密度", "330", "人/km² · 2023"],
      ["地形", "山地为主，平原沿海分布", null],
      ["气候", "南北跨度大，受季风与海洋影响", null],
      ["主要城市", "东京、大阪、名古屋、福冈", null],
    ],
  ),
  entity(
    "sichuan",
    "province",
    "四川",
    "Sichuan",
    "盆地、山地、河流与季风共同解释这里的自然格局。",
    [30.65, 102.7],
    [
      ["地形", "四川盆地与周缘山地", null],
      ["气候", "湿润、多云，盆地内水热条件较好", null],
      ["主要城市", "成都、绵阳、乐山、宜宾", null],
    ],
  ),
  entity(
    "chengdu",
    "city",
    "成都",
    "Chengdu",
    "盆地西部的平原城市，农业基础支撑了长期发展。",
    [30.57, 104.07],
    [
      ["海拔", "约 500", "m"],
      ["地形", "成都平原，位于四川盆地西部", null],
      ["气候", "亚热带湿润季风气候，多云雾", null],
      ["人口", "21,400,000", "人 · 2023口径"],
      ["水系", "都江堰—岷江水系", null],
    ],
  ),
  entity(
    "chongqing",
    "city",
    "重庆",
    "Chongqing",
    "长江与嘉陵江交汇的山地河谷城市。",
    [29.56, 106.55],
    [
      ["海拔", "约 260", "m"],
      ["地形", "山地与河谷", null],
      ["水系", "长江与嘉陵江交汇", null],
      ["人口", "32,100,000", "人 · 2023口径"],
    ],
  ),
  entity(
    "yangtze",
    "river",
    "长江",
    "Yangtze River",
    "连接高原、盆地、平原和海洋的一条探索主线。",
    [30, 112],
    [
      ["长度", "约 6,300", "km"],
      ["源头区域", "青藏高原腹地", null],
      ["沿线城市", "重庆、武汉、南京、上海", null],
      ["流经地形", "高原、峡谷、盆地、平原", null],
    ],
  ),
  entity(
    "tibetan-plateau",
    "plateau",
    "青藏高原",
    "Tibetan Plateau",
    "印度板块与欧亚板块碰撞塑造的高亚洲地形核心。",
    [33, 88],
    [
      ["平均海拔", "超过 4,000", "m"],
      ["地形", "高原与高山系统", null],
      ["气候", "高寒，影响亚洲季风与水汽输送", null],
    ],
  ),
  entity(
    "sichuan-basin",
    "basin",
    "四川盆地",
    "Sichuan Basin",
    "周缘山地限制气流交换，水汽与云雾更易积聚。",
    [30.5, 105.5],
    [
      ["地形", "盆地，西部连接成都平原", null],
      ["气候", "湿润、多云雾", null],
      ["人口关系", "平原与水系支持高密度聚居", null],
    ],
  ),
  entity(
    "himalayas",
    "mountain_range",
    "喜马拉雅山脉",
    "Himalayas",
    "板块碰撞的地表结果之一，也是亚洲重要气候屏障。",
    [28, 84],
    [
      ["最高峰", "珠穆朗玛峰约 8,849", "m"],
      ["形成过程", "印度板块与欧亚板块碰撞", null],
      ["气候影响", "改变水汽输送与季风分布", null],
    ],
  ),
  entity(
    "qinling",
    "mountain_range",
    "秦岭",
    "Qinling Mountains",
    "中国南北气候、植被和水系差异的重要分界。",
    [33.8, 108],
    [
      ["气候影响", "中国南北气候分界的重要山脉", null],
      ["水系", "长江与黄河流域分水背景", null],
    ],
  ),
  entity(
    "shanghai",
    "city",
    "上海",
    "Shanghai",
    "长江三角洲冲积平原上的河口城市。",
    [31.23, 121.47],
    [
      ["地形", "长江三角洲冲积平原", null],
      ["水系", "长江入海口", null],
      ["人口", "24,890,000", "人 · 2023口径"],
    ],
  ),
  entity(
    "northeast-china",
    "region",
    "中国东北",
    "Northeast China",
    "东北平原、黑土地与寒冷冬季共同塑造农业与人口分布。",
    [45.7, 126.6],
    [
      ["地形", "东北平原与大小兴安岭", null],
      ["气候", "温带季风，冬季寒冷", null],
      ["主要城市", "哈尔滨、长春、沈阳、大连", null],
    ],
  ),
];

const FALLBACK_LINES: GeoMapLine[] = [
  {
    from_id: "yangtze",
    to_id: "tibetan-plateau",
    kind: "SOURCE_OF",
    from: { system: "WGS84", latitude: 30, longitude: 112 },
    to: { system: "WGS84", latitude: 33, longitude: 88 },
  },
  {
    from_id: "yangtze",
    to_id: "chongqing",
    kind: "FLOWS_THROUGH",
    from: { system: "WGS84", latitude: 30, longitude: 112 },
    to: { system: "WGS84", latitude: 29.56, longitude: 106.55 },
  },
  {
    from_id: "yangtze",
    to_id: "shanghai",
    kind: "FLOWS_INTO",
    from: { system: "WGS84", latitude: 30, longitude: 112 },
    to: { system: "WGS84", latitude: 31.23, longitude: 121.47 },
  },
  {
    from_id: "chengdu",
    to_id: "sichuan",
    kind: "LOCATED_IN",
    from: { system: "WGS84", latitude: 30.57, longitude: 104.07 },
    to: { system: "WGS84", latitude: 30.65, longitude: 102.7 },
  },
  {
    from_id: "sichuan-basin",
    to_id: "himalayas",
    kind: "SURROUNDS",
    from: { system: "WGS84", latitude: 30.5, longitude: 105.5 },
    to: { system: "WGS84", latitude: 28, longitude: 84 },
  },
];

const FALLBACK_POINTS: GeoMapPoint[] = FALLBACK_ENTITIES.map((item) => ({
  entity_id: item.id,
  name: item.name,
  entity_type: item.entity_type,
  coordinate: item.coordinates!,
}));



function propertyValue(item: GeoEntity, key: string) {
  return (
    item.properties.find((property) => property.key === key)?.value ?? null
  );
}

/** 只从实体自身的 properties 读取，用于卡片上的 2–3 条关键属性；不推测缺失字段。 */
function keyFacts(item: GeoEntity, count: number) {
  return item.properties.slice(0, count).map((property) => ({
    key: property.key,
    label: property.label,
    value: property.unit ? `${property.value} ${property.unit}` : property.value,
  }));
}

const SEARCH_SUGGESTIONS = ["成都", "长江", "青藏高原", "四川盆地"];

interface MapPreviewState {
  id: string;
  detail: GeoEntityDetail | null;
  loading: boolean;
  error: string | null;
}

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

function relationLabel(kind: string) {
  return RELATION_LABELS[kind] ?? kind;
}

function historyRelationLabel(kind: string) {
  if (kind === "historical_event_place_anchor") return "历史事件地点锚点";
  if (kind === "modern_urban_area_anchor") return "现代城市范围与历史地点锚点";
  return kind;
}

function parentRelation(detail: GeoEntityDetail) {
  return (
    detail.relations.find(
      (item) =>
        (item.relation.kind === "PART_OF" ||
          item.relation.kind === "LOCATED_IN") &&
        item.relation.from_id === detail.entity.id,
    )?.entity ?? null
  );
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
  if (
    item.entity_type === "country" ||
    item.entity_type === "province" ||
    item.entity_type === "region"
  ) {
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

function EntityCard({
  item,
  favorite = false,
  onOpen,
}: {
  item: GeoEntity;
  favorite?: boolean;
  onOpen: (id: string) => void;
}) {
  return (
    <button
      type="button"
      className="geo-entity-card"
      onClick={() => onOpen(item.id)}
    >
      <span className="geo-card-head">
        <span className="geo-card-type">{TYPE_LABELS[item.entity_type]}</span>
        {favorite ? (
          <span className="geo-card-favorite" title="已收藏">
            <BookmarkSimple size={13} weight="fill" aria-hidden="true" />
            <span className="geo-visually-hidden">已收藏</span>
          </span>
        ) : null}
      </span>
      <strong>{item.name}</strong>
      <small>{item.name_en}</small>
      <p>{item.summary}</p>
      <span className="geo-card-cta">
        查看详情 <ArrowRight size={14} aria-hidden="true" />
      </span>
    </button>
  );
}

/**
 * 探索索引：精选地点 / 收藏 横向卡片轨道。
 * 数据只来自 home.featured 与 home.favorite_ids；收藏里只有 id 的实体不显示名称。
 */
function ExploreIndexRail({
  featured,
  favoriteIds,
  onOpen,
  onLocate,
  selectedId,
}: {
  featured: GeoEntity[];
  favoriteIds: string[];
  onOpen: (id: string) => void;
  onLocate: (id: string) => void;
  selectedId?: string | null;
}) {
  const [tab, setTab] = useState<"featured" | "favorites">("featured");
  const trackRef = useRef<HTMLUListElement>(null);
  const favoriteSet = useMemo(() => new Set(favoriteIds), [favoriteIds]);
  const favoriteItems = featured.filter((item) => favoriteSet.has(item.id));
  const unknownFavorites = Math.max(0, favoriteIds.length - favoriteItems.length);
  const items = tab === "featured" ? featured : favoriteItems;

  // 地图选中实体时，把索引轨道里对应的卡片滚进视野（只横向滚动轨道本身）。
  useEffect(() => {
    if (!selectedId) return;
    const track = trackRef.current;
    const tile = track?.querySelector<HTMLElement>(
      `[data-entity-id="${selectedId.replace(/"/g, '\\"')}"]`,
    );
    if (!track || !tile) return;
    const trackRect = track.getBoundingClientRect();
    const tileRect = tile.getBoundingClientRect();
    if (tileRect.left < trackRect.left || tileRect.right > trackRect.right) {
      const reduceMotion =
        typeof window !== "undefined" &&
        typeof window.matchMedia === "function" &&
        window.matchMedia("(prefers-reduced-motion: reduce)").matches;
      track.scrollBy({
        left: tileRect.left - trackRect.left - 12,
        behavior: reduceMotion ? "auto" : "smooth",
      });
    }
  }, [selectedId]);

  const scrollRail = (direction: 1 | -1) => {
    const track = trackRef.current;
    if (!track) return;
    const reduceMotion =
      typeof window !== "undefined" &&
      typeof window.matchMedia === "function" &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    track.scrollBy({
      left: direction * Math.max(track.clientWidth * 0.72, 240),
      behavior: reduceMotion ? "auto" : "smooth",
    });
  };

  return (
    <section className="geo-index-rail" aria-label="探索索引">
      <header className="geo-index-rail-head">
        <div>
          <span className="geo-eyebrow">EXPLORE INDEX</span>
          <h2>从索引开始</h2>
          <p>
            精选来自首页 featured；收藏只显示已知名称的实体，不推测缺失内容。
            点卡片打开详情，点右上角定位图标在地图上查看。
          </p>
        </div>
        <div className="geo-rail-tabs" aria-label="探索索引分组">
          <button
            type="button"
            className={tab === "featured" ? "selected" : ""}
            aria-pressed={tab === "featured"}
            onClick={() => setTab("featured")}
          >
            精选地点 <span>{featured.length}</span>
          </button>
          <button
            type="button"
            className={tab === "favorites" ? "selected" : ""}
            aria-pressed={tab === "favorites"}
            onClick={() => setTab("favorites")}
          >
            <BookmarkSimple size={13} aria-hidden="true" /> 收藏{" "}
            <span>{favoriteIds.length}</span>
          </button>
        </div>
      </header>
      {items.length ? (
        <div className="geo-rail-window">
          <button
            type="button"
            className="geo-rail-nav prev"
            aria-label="向左滚动探索索引"
            onClick={() => scrollRail(-1)}
          >
            <CaretLeft size={16} aria-hidden="true" />
          </button>
          <ul
            className="geo-rail-track"
            ref={trackRef}
            aria-label={tab === "featured" ? "精选地点" : "已收藏地点"}
          >
            {items.map((item) => (
              <li
                key={item.id}
                data-entity-id={item.id}
                className={`geo-rail-item${selectedId === item.id ? " is-selected" : ""}`}
              >
                <button
                  type="button"
                  className="geo-rail-tile"
                  onClick={() => onOpen(item.id)}
                >
                  <span className="geo-rail-tile-head">
                    <span className="geo-rail-tile-type">
                      {TYPE_LABELS[item.entity_type]}
                    </span>
                    {favoriteSet.has(item.id) ? (
                      <BookmarkSimple
                        size={13}
                        weight="fill"
                        className="geo-rail-tile-favorite"
                        aria-hidden="true"
                      />
                    ) : null}
                  </span>
                  <strong>{item.name}</strong>
                  <small>{item.name_en ?? ""}</small>
                  <dl className="geo-rail-tile-facts">
                    {keyFacts(item, 2).map((fact) => (
                      <div key={fact.key}>
                        <dt>{fact.label}</dt>
                        <dd>{fact.value}</dd>
                      </div>
                    ))}
                  </dl>
                </button>
                <button
                  type="button"
                  className="geo-rail-tile-locate"
                  onClick={() => onLocate(item.id)}
                  aria-label={`在地图上查看 ${item.name}`}
                  title="在地图查看"
                >
                  <MapPin size={14} aria-hidden="true" />
                </button>
              </li>
            ))}
          </ul>
          <button
            type="button"
            className="geo-rail-nav next"
            aria-label="向右滚动探索索引"
            onClick={() => scrollRail(1)}
          >
            <CaretRight size={16} aria-hidden="true" />
          </button>
        </div>
      ) : (
        <div className="geo-rail-empty" role="status">
          {tab === "favorites" && favoriteIds.length > 0 ? (
            <p>
              <strong>
                已收藏的 {favoriteIds.length} 个实体不在本期 featured 列表中。
              </strong>
              这里只显示已知名称的收藏；缺少名称时不会臆造内容，可用上方搜索查看。
            </p>
          ) : (
            <p>
              <strong>还没有收藏地点。</strong>
              打开任意地点的详情页，点「收藏」就会出现在这里。
            </p>
          )}
          <button type="button" onClick={() => setTab("featured")}>
            浏览精选地点 <ArrowRight size={14} aria-hidden="true" />
          </button>
        </div>
      )}
      {tab === "favorites" && items.length > 0 && unknownFavorites > 0 ? (
        <p className="geo-rail-note">
          另有 {unknownFavorites} 个收藏实体不在本期 featured 列表中；缺少名称的收藏不会被臆造。
        </p>
      ) : null}
    </section>
  );
}

function SearchResultsPanel({
  groups,
  onOpen,
  onLocate,
  onClose,
  selectedId,
}: {
  groups: GeoSearchGroup[];
  onOpen: (id: string) => void;
  onLocate: (id: string) => void;
  onClose: () => void;
  selectedId?: string | null;
}) {
  const total = groups.reduce((sum, group) => sum + group.items.length, 0);
  return (
    <section className="geo-search-results" aria-label="搜索结果">
      <header>
        <div>
          <span>SEARCH RESULTS</span>
          <strong>{total} 个地点</strong>
          <small>按类型统计 · 打开后可继续沿关系探索</small>
        </div>
        <button type="button" onClick={onClose}>
          <X size={15} aria-hidden="true" /> 关闭
        </button>
      </header>
      <div className="geo-search-body">
        <aside className="geo-search-group-index" aria-label="结果类型统计">
          <span className="geo-search-group-index-label">按类型</span>
          {groups.map((group) => (
            <div key={group.entity_type}>
              <b>{TYPE_LABELS[group.entity_type]}</b>
              <span>{group.items.length}</span>
            </div>
          ))}
        </aside>
        <div className="geo-search-group-grid">
          {groups.flatMap((group) =>
            group.items.map((item) => (
              <div
                className={`geo-search-item${selectedId === item.id ? " is-selected" : ""}`}
                key={item.id}
              >
                <button
                  type="button"
                  className="geo-search-tile"
                  onClick={() => onOpen(item.id)}
                >
                  <span className="geo-search-tile-type">
                    {TYPE_LABELS[item.entity_type]}
                  </span>
                  <strong>{item.name}</strong>
                  <small>{item.name_en ?? "—"}</small>
                  <p>{item.summary}</p>
                  <span className="geo-search-tile-cta">
                    查看详情 <ArrowRight size={13} aria-hidden="true" />
                  </span>
                </button>
                <button
                  type="button"
                  className="geo-search-tile-locate"
                  onClick={() => onLocate(item.id)}
                  aria-label={`在地图上查看 ${item.name}`}
                  title="在地图查看"
                >
                  <MapPin size={14} aria-hidden="true" />
                </button>
              </div>
            )),
          )}
        </div>
      </div>
    </section>
  );
}

/** 地图上选中实体时的轻量浮动信息卡；不遮挡地图缩放 / 指北控件。 */
function MapEntityPreview({
  preview,
  point,
  favorite,
  onOpen,
  onToggleFavorite,
  onClose,
  onRetry,
}: {
  preview: MapPreviewState;
  point: GeoMapPoint | null;
  favorite: boolean;
  onOpen: () => void;
  onToggleFavorite: () => void;
  onClose: () => void;
  onRetry: () => void;
}) {
  const item = preview.detail?.entity ?? null;
  const name = item?.name ?? point?.name ?? "正在载入地点…";
  const type = item?.entity_type ?? point?.entity_type ?? null;
  const coordinate = item?.coordinates
    ? `${item.coordinates.latitude.toFixed(2)}° N · ${item.coordinates.longitude.toFixed(2)}° E`
    : point
      ? `${point.coordinate.latitude.toFixed(2)}° N · ${point.coordinate.longitude.toFixed(2)}° E`
      : null;
  const facts = item ? keyFacts(item, 3) : [];
  return (
    <section
      className="geo-map-preview"
      aria-label={`选中地点：${name}`}
      aria-live="polite"
    >
      <header className="geo-map-preview-head">
        <span className="geo-map-preview-type">
          {type ? TYPE_LABELS[type] : "地点"}
        </span>
        {favorite ? (
          <span className="geo-map-preview-flag">
            <BookmarkSimple size={12} weight="fill" aria-hidden="true" />
            已收藏
          </span>
        ) : null}
        <button
          type="button"
          className="geo-map-preview-close"
          onClick={onClose}
          aria-label="关闭地点预览"
        >
          <X size={14} aria-hidden="true" />
        </button>
      </header>
      <h3 className="geo-map-preview-name">{name}</h3>
      {item?.name_en ? (
        <p className="geo-map-preview-en">{item.name_en}</p>
      ) : null}
      {coordinate ? (
        <p className="geo-map-preview-coord">
          <MapPin size={12} aria-hidden="true" /> {coordinate}
        </p>
      ) : null}
      {preview.loading ? (
        <p className="geo-map-preview-state" role="status">
          正在读取地点资料…
        </p>
      ) : null}
      {preview.error ? (
        <div className="geo-map-preview-error" role="alert">
          <p>地点资料暂时不可用：{preview.error}</p>
          <button type="button" onClick={onRetry}>
            <ArrowClockwise size={13} aria-hidden="true" /> 重试
          </button>
        </div>
      ) : null}
      {facts.length ? (
        <dl className="geo-map-preview-facts">
          {facts.map((fact) => (
            <div key={fact.key}>
              <dt>{fact.label}</dt>
              <dd>{fact.value}</dd>
            </div>
          ))}
        </dl>
      ) : null}
      <div className="geo-map-preview-actions">
        <button type="button" className="geo-map-preview-open" onClick={onOpen}>
          查看详情 <ArrowRight size={14} aria-hidden="true" />
        </button>
        <button
          type="button"
          className={`geo-map-preview-favorite${favorite ? " selected" : ""}`}
          aria-pressed={favorite}
          onClick={onToggleFavorite}
        >
          <BookmarkSimple
            size={14}
            weight={favorite ? "fill" : "regular"}
            aria-hidden="true"
          />
          {favorite ? "已收藏" : "收藏"}
        </button>
      </div>
    </section>
  );
}

function EntityDetail({
  detail,
  onBack,
  onFavorite,
  onOpen,
  historyContext,
  historyLoading,
  historyError,
  onAddToTravel,
  onOpenHistoryEvent,
  onOpenHistoryStory,
  onHistorySelectionChange,
  onRetryHistory,
  onOpenLandforms,
  onReturnToTravel,
  onReturnToHistory,
}: {
  detail: GeoEntityDetail;
  onBack: () => void;
  onFavorite: () => void;
  onOpen: (id: string) => void;
  historyContext: LocationHistoryContext | null;
  historyLoading: boolean;
  historyError: string | null;
  onAddToTravel?: (entity: GeoEntity, event?: SemanticEventDetail) => void;
  onOpenHistoryEvent?: (eventId: string, location: { id: string; name: string }) => void;
  onOpenHistoryStory?: (storyId: string, location: { id: string; name: string }) => void;
  onHistorySelectionChange?: (entityId: string, selection: GeographyHistorySelection | null) => void;
  onRetryHistory?: () => void;
  onOpenLandforms?: () => void;
  onReturnToTravel?: () => void;
  onReturnToHistory?: () => void;
}) {
  const { entity: item } = detail;
  const parent = parentRelation(detail);
  const crosswalks = historyContext?.crosswalks?.length
    ? historyContext.crosswalks
    : historyContext?.crosswalk
      ? [historyContext.crosswalk]
      : [];
  const verifiedLocationEvents = useMemo(
    () => reviewedLocationEvents(historyContext?.events ?? [], crosswalks),
    [crosswalks, historyContext?.events],
  );
  const handleTimelineSelection = useCallback(
    (selection: GeographyHistorySelection | null) =>
      onHistorySelectionChange?.(item.id, selection),
    [item.id, onHistorySelectionChange],
  );
  const cue = learningCue(item);
  const coordinateText = item.coordinates
    ? `${item.coordinates.latitude.toFixed(2)}° N · ${item.coordinates.longitude.toFixed(2)}° E`
    : "坐标待补";
  return (
    <section className="geo-detail-panel">
      <header className="geo-detail-head">
        <button type="button" className="geo-back-button" onClick={onBack}>
          ← 探索首页
        </button>
        {onReturnToTravel ? (
          <button type="button" className="geo-back-button" onClick={onReturnToTravel}>
            返回当前行程
          </button>
        ) : null}
        {onReturnToHistory ? (
          <button type="button" className="geo-back-button" onClick={onReturnToHistory}>
            返回 History
          </button>
        ) : null}
        <div className="geo-detail-actions">
          <button
            type="button"
            className={detail.favorite ? "selected" : ""}
            onClick={onFavorite}
          >
            <BookmarkSimple
              size={17}
              weight={detail.favorite ? "fill" : "regular"}
            />
            {detail.favorite ? "已收藏" : "收藏"}
          </button>
          <span>{TYPE_LABELS[item.entity_type]}</span>
        </div>
      </header>
      <div className="geo-detail-title">
        <div>
          <span className="geo-eyebrow">
            {TYPE_LABELS[item.entity_type].toUpperCase()} ·{" "}
            {item.coordinates?.system ?? "OFFLINE"}
          </span>
          <h2>{item.name}</h2>
          <p>{item.name_en}</p>
        </div>
        <div className="geo-coordinate">{coordinateText}</div>
      </div>
      <p className="geo-detail-summary">{item.summary}</p>
      <div className="geo-detail-save">
        <SaveToCollection
          module="geography"
          entityType={item.entity_type}
          entityId={item.id}
          title={item.name}
        />
      </div>
      <div className="geo-detail-overview">
        <section className="geo-profile-card">
          <div className="geo-section-label">
            地理档案 <span>PROFILE</span>
          </div>
          <dl>
            <div>
              <dt>所属层级</dt>
              <dd>
                {parent
                  ? `${TYPE_LABELS[parent.entity_type]} · ${parent.name}`
                  : item.parent_id
                    ? item.parent_id
                    : "世界 / 独立实体"}
              </dd>
            </div>
            <div>
              <dt>别名</dt>
              <dd>
                {item.aliases.length ? item.aliases.join("、") : "暂无记录"}
              </dd>
            </div>
            <div>
              <dt>坐标</dt>
              <dd>{coordinateText}</dd>
            </div>
            <div>
              <dt>探索关系</dt>
              <dd>
                {detail.relations.length} 条 · {detail.sources.length} 个来源
              </dd>
            </div>
          </dl>
        </section>
        <section className="geo-learning-card">
          <div className="geo-section-label">
            学习线索 <span>LEARNING CUE</span>
          </div>
          <strong>先记住</strong>
          <p>{cue.remember}</p>
          <strong>继续追问</strong>
          <p>{cue.ask}</p>
        </section>
      </div>
      <AmapRegionMap
        key={item.id}
        item={item}
        relations={detail.relations}
        onSelect={onOpen}
      />
      <button type="button" className="geo-landform-jump" onClick={onOpenLandforms}>
        <span className="geo-landform-jump-mark"><BookOpen size={17} /></span>
        <span>
          <strong>继续探索地貌与地形</strong>
          <small>打开概念地貌模型与可用的真实 DEM；示意模型不代表本地点测绘。</small>
        </span>
        <ArrowRight size={16} />
      </button>
      <div className="geo-detail-grid">
        <section>
          <div className="geo-section-label">关键事实</div>
          <div className="geo-facts">
            {item.properties.map((property) => (
              <div key={property.key}>
                <small>{property.label}</small>
                <strong>{property.value}</strong>
                {property.unit ? <span>{property.unit}</span> : null}
              </div>
            ))}
          </div>
        </section>
        <section className="geo-why-card">
          <div className="geo-section-label">
            <Question size={15} /> WHY · 为什么这里是今天的样子
          </div>
          <p>
            {item.entity_type === "city"
              ? `地形、水系与人类活动共同塑造了${item.name}的城市结构。`
              : item.entity_type === "river"
                ? `${item.name}把多个地形单元串成一条连续的探索路径。`
                : `${item.name}不是孤立的事实，它与周围的地形、气候和人类活动共同形成今天的格局。`}
          </p>
        </section>
      </div>
      <section className="geo-related">
        <header>
          <div>
            <div className="geo-section-label">
              <GitBranch size={15} /> RELATIONS
            </div>
            <h3>沿着关系继续探索</h3>
          </div>
          <span>{detail.relations.length} 条关系</span>
        </header>
        <div className="geo-relation-cards">
          {detail.relations.map((relation) => (
            <button
              type="button"
              key={`${relation.relation.from_id}-${relation.relation.to_id}-${relation.relation.kind}`}
              onClick={() => onOpen(relation.entity.id)}
            >
              <span>{relationLabel(relation.relation.kind)}</span>
              <strong>{relation.entity.name}</strong>
              <small>{relation.relation.note ?? "相关地理实体"}</small>
              <ArrowRight size={14} />
            </button>
          ))}
        </div>
      </section>
      <section className="geo-history-bridge" aria-labelledby="geo-history-title">
        <header>
          <div>
            <span className="geo-section-label">HISTORY CROSSWALK</span>
            <h3 id="geo-history-title">这座城市的历史线索</h3>
          </div>
          {crosswalks.length > 0 ? (
            <span className="geo-history-reviewed">
              稳定标识 · 已审核{crosswalks.length > 1 ? ` · ${crosswalks.length} 条` : ""}
            </span>
          ) : null}
        </header>
        {historyLoading ? <p className="geo-history-empty" role="status">正在通过稳定地点标识查询 History…</p> : null}
        {historyError ? (
          <div className="geo-history-load-error" role="alert">
            <p>History 暂时不可用；这不是“没有历史记录”。{historyError}</p>
            <button type="button" onClick={onRetryHistory}>重新加载关联</button>
          </div>
        ) : null}
        {!historyLoading && !historyError && historyContext && crosswalks.length === 0 ? (
          <p className="geo-history-empty">暂无已审核的 History 地点关联。不会仅凭城市名称拼接历史资料。</p>
        ) : null}
        {!historyLoading && !historyError && historyContext && crosswalks.length > 0 && verifiedLocationEvents.length === 0 ? (
          <p className="geo-history-empty">已找到已审核的 History 地点标识，但没有通过地点事件审核与证据时间范围的事件。</p>
        ) : null}
        {crosswalks.map((link) => (
          <div className="geo-history-link" key={link.id}>
            <p className="geo-history-provenance">
              Geography ID <code>{link.geography_entity_id}</code>
              <span>↔</span>
              History place ID <code>{link.history_place_id}</code>
            </p>
            <p className="geo-history-link-meta">
              {historyRelationLabel(link.relation_type ?? "")} · {link.status === "reviewed" ? "已审核" : link.status}
              {link.evidence_time_scope
                ? ` · 事件证据时间 ${link.evidence_time_scope.start_year}${link.evidence_time_scope.end_year !== link.evidence_time_scope.start_year ? `–${link.evidence_time_scope.end_year}` : ""} 年（${link.evidence_time_scope.precision}）`
                : " · 地点有效年代未标注"}
            </p>
            {link.provenance?.length ? (
              <details className="geo-history-evidence">
                <summary>查看关联依据 · {link.provenance.length} 条来源记录</summary>
                <ul>
                  {link.provenance.map((source) => (
                    <li key={`${source.source_id}:${source.record_id}`}>
                      <span>{source.source_id} / {source.record_id}</span>
                      {source.uri?.startsWith("https://") ? (
                        <a href={source.uri} target="_blank" rel="noreferrer">打开来源 ↗</a>
                      ) : source.uri ? (
                        <small>{source.uri}</small>
                      ) : null}
                      <p>{source.note}</p>
                    </li>
                  ))}
                </ul>
              </details>
            ) : null}
          </div>
        ))}
        {verifiedLocationEvents.length > 0 ? (
          <LocationHistoryTimeline
            key={item.id}
            entity={item}
            context={historyContext}
            crosswalks={crosswalks}
            onSelectionChange={handleTimelineSelection}
            onOpenHistoryEvent={(eventId) => onOpenHistoryEvent?.(eventId, { id: item.id, name: item.name })}
            onOpenHistoryStory={(storyId) => onOpenHistoryStory?.(storyId, { id: item.id, name: item.name })}
            onAddToTravel={item.entity_type === "city" ? onAddToTravel : undefined}
          />
        ) : null}
        {item.entity_type === "city" && verifiedLocationEvents.length === 0 ? (
          <button className="geo-add-to-travel" type="button" onClick={() => onAddToTravel?.(item)}>
            仅将 {item.name} 地点加入行程（不附加未经审核的历史事件） <ArrowRight size={15} />
          </button>
        ) : null}
      </section>
      <section className="geo-sources">
        <header>
          <span className="geo-section-label">SOURCES</span>
          <span>事实不由 AI 猜测</span>
        </header>
        {detail.sources.map((source) => (
          <div key={source.id}>
            <strong>{source.dataset}</strong>
            <span>{source.version}</span>
            <span>{source.license}</span>
            <a href={source.url} target="_blank" rel="noreferrer">
              查看来源 ↗
            </a>
          </div>
        ))}
      </section>
    </section>
  );
}

function KnowledgePage({
  onOpenEntity,
  returnEntity,
  onReturnToExplore,
}: {
  onOpenEntity: (id: string) => void;
  returnEntity?: GeoEntity | null;
  onReturnToExplore?: () => void;
}) {
  const [selectedId, setSelectedId] = useState(
    LANDFORM_KNOWLEDGE_ENTRIES[0].id,
  );
  const [landformQuery, setLandformQuery] = useState("");
  const selected =
    LANDFORM_KNOWLEDGE_ENTRIES.find((entry) => entry.id === selectedId) ??
    LANDFORM_KNOWLEDGE_ENTRIES[0];
  const visibleEntries = useMemo(() => {
    const query = landformQuery.trim().toLocaleLowerCase();
    if (!query) return LANDFORM_KNOWLEDGE_ENTRIES;
    return LANDFORM_KNOWLEDGE_ENTRIES.filter((entry) =>
      `${entry.title} ${entry.subtitle} ${entry.category} ${entry.keywords.join(" ")}`
        .toLocaleLowerCase()
        .includes(query),
    );
  }, [landformQuery]);
  const selectedIndex = LANDFORM_KNOWLEDGE_ENTRIES.findIndex(
    (entry) => entry.id === selected.id,
  );
  const [displayMode, setDisplayMode] = useState<"conceptual" | "real">(
    "conceptual",
  );
  const realRegion = useMemo(() => getRealTerrain(selected.id), [selected.id]);
  return (
    <section className="geo-knowledge-page">
      <header className="geo-knowledge-header">
        <div>
          <span className="geo-eyebrow">GEOGRAPHY KNOWLEDGE · LANDFORMS</span>
          <h2>先认识地貌，再理解地图</h2>
          <p>
            按主导营力组织主要地貌单元。概念模型是教学示意；真实 DEM 只对应标注的示例区域，不代表当前城市测绘。
          </p>
        </div>
        <div className="geo-knowledge-count">
          <strong>{LANDFORM_KNOWLEDGE_ENTRIES.length}</strong>
          <span>个主要地貌主题</span>
        </div>
      </header>
      {returnEntity ? (
        <button type="button" className="geo-knowledge-return" onClick={onReturnToExplore}>
          ← 返回 {returnEntity.name} 地图与地点时间线
        </button>
      ) : null}
      <div className="geo-knowledge-layout">
        <nav className="geo-knowledge-index" aria-label="地貌知识目录">
          <div className="geo-knowledge-index-head">
            <span className="geo-knowledge-index-label">LANDFORM INDEX</span>
            <small>
              {landformQuery ? `${visibleEntries.length} 个结果` : "按成因浏览"}
            </small>
          </div>
          <label className="geo-landform-search">
            <MagnifyingGlass size={14} />
            <input
              value={landformQuery}
              onChange={(event) => setLandformQuery(event.target.value)}
              placeholder="搜索地貌、成因或关键词"
              aria-label="搜索地貌"
            />
            {landformQuery ? (
              <button
                type="button"
                onClick={() => setLandformQuery("")}
                aria-label="清除地貌搜索"
              >
                <X size={13} />
              </button>
            ) : null}
          </label>
          {visibleEntries.length ? (
            visibleEntries.map((entry) => {
              const index = LANDFORM_KNOWLEDGE_ENTRIES.indexOf(entry);
              return (
                <button
                  type="button"
                  key={entry.id}
                  className={selected.id === entry.id ? "selected" : ""}
                  onClick={() => setSelectedId(entry.id)}
                >
                  <i>{String(index + 1).padStart(2, "0")}</i>
                  <span>
                    <strong>{entry.title}</strong>
                    <small>
                      {entry.category} · {entry.subtitle}
                    </small>
                  </span>
                  <ArrowRight size={15} />
                </button>
              );
            })
          ) : (
            <p className="geo-landform-no-results">
              未找到相关地貌。可尝试“冰川”“海岸”“侵蚀”或“火山”。
            </p>
          )}
        </nav>
        <article className="geo-knowledge-detail">
          <div className="geo-knowledge-detail-title">
            <div>
              <span className="geo-card-type">{selected.category}</span>
              <h3>{selected.title}</h3>
              <p>{selected.subtitle}</p>
            </div>
            <span className="geo-knowledge-mark">
              {String(selectedIndex + 1).padStart(2, "0")} /{" "}
              {String(LANDFORM_KNOWLEDGE_ENTRIES.length).padStart(2, "0")}
            </span>
          </div>
          <div
            className="geo-viewer-tabs"
            role="tablist"
            aria-label="地貌呈现方式"
          >
            <button
              type="button"
              role="tab"
              aria-selected={displayMode === "conceptual"}
              className={displayMode === "conceptual" ? "selected" : ""}
              onClick={() => setDisplayMode("conceptual")}
            >
              <MagnetStraight size={15} />
              <span>
                <strong>概念地形</strong>
                <small>Three.js · 程序化模型</small>
              </span>
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={displayMode === "real"}
              className={displayMode === "real" ? "selected" : ""}
              onClick={() => setDisplayMode("real")}
            >
              <MapTrifold size={15} />
              <span>
                <strong>真实地形</strong>
                <small>MapLibre · 真实 DEM · {realRegion.place}</small>
              </span>
            </button>
          </div>
          {displayMode === "conceptual" ? (
            <Landform3DViewer
              title={selected.title}
              config={LANDFORM_VIEWERS[selected.viewerType]}
            />
          ) : (
            <ViewerErrorBoundary
              fallback={
                <div className="realterrain-error" role="alert">
                  <span>真实地形模块暂时无法显示。</span>
                  <p>可切换回「概念地形」继续学习；其他功能不受影响。</p>
                </div>
              }
            >
              <Suspense
                fallback={
                  <div className="realterrain-loading wide">
                    正在载入 MapLibre 真实地形模块…
                  </div>
                }
              >
                <RealTerrainViewer region={realRegion} />
              </Suspense>
            </ViewerErrorBoundary>
          )}
          <div className="geo-knowledge-copy">
            <section>
              <span>01 · 它是什么</span>
              <p>{selected.intro}</p>
            </section>
            <section>
              <span>02 · 怎么形成</span>
              <p>{selected.formation}</p>
            </section>
            <section>
              <span>03 · 怎么辨认</span>
              <p>{selected.identify}</p>
            </section>
          </div>
          <footer className="geo-knowledge-example">
            <div>
              <span>典型案例</span>
              <strong>{selected.example}</strong>
            </div>
            <button
              type="button"
              onClick={() => onOpenEntity(selected.exampleId)}
            >
              打开实体详情 <ArrowRight size={15} />
            </button>
          </footer>
        </article>
      </div>
    </section>
  );
}

export function GeographyPage({
  active,
  setNotice,
  intent,
  onContextChange,
  onAddToTravel,
  onOpenHistoryEvent,
  onOpenHistoryStory,
  onReturnToTravel,
  onReturnToHistory,
}: GeographyPageProps & {
  onContextChange?: (ctx: AppContextPayload | null) => void;
}) {
  const [home, setHome] = useState<GeographyHome | null>(null);
  const [homeError, setHomeError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [searchGroups, setSearchGroups] = useState<GeoSearchGroup[]>([]);
  const [searchSubmitted, setSearchSubmitted] = useState(false);
  const [searchLoading, setSearchLoading] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);
  const searchRequestRef = useRef(0);
  const [detail, setDetail] = useState<GeoEntityDetail | null>(null);
  const [locationHistory, setLocationHistory] = useState<LocationHistoryContext | null>(null);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [historyError, setHistoryError] = useState<string | null>(null);
  const [historyRetry, setHistoryRetry] = useState(0);
  const [selectedHistory, setSelectedHistory] = useState<
    (GeographyHistorySelection & { entityId: string }) | null
  >(null);
  const [layer, setLayer] = useState<GeoMapLayer>("terrain");
  const [view, setView] = useState<"explore" | "knowledge">("explore");
  // 地图上的轻量选择：点击地图实体只打开浮动信息卡，详情仍走原有 openEntity。
  const [mapSelectionId, setMapSelectionId] = useState<string | null>(null);
  /** 从索引/搜索结果「在地图查看」时的观察中心；null 表示使用随机起点。 */
  const [mapFocus, setMapFocus] = useState<{ center: [number, number]; zoom: number } | null>(null);
  const [mapPreview, setMapPreview] = useState<MapPreviewState | null>(null);
  const previewCacheRef = useRef(new Map<string, GeoEntityDetail>());
  const previewRequestRef = useRef(0);

  const loadHome = useCallback(async () => {
    setHomeError(null);
    try {
      setHome(await geographyClient.home(0));
    } catch (error) {
      const message = errorMessage(error);
      setHomeError(message);
      setNotice(message);
    }
  }, [setNotice]);
  useEffect(() => {
    if (active && !home) void loadHome();
  }, [active, home, loadHome]);

  const openEntity = useCallback(
    async (id: string) => {
      setSelectedHistory((current) => current?.entityId === id ? current : null);
      setView("explore");
      // 详情成为主视图时收起地图浮动卡，避免两个信息层同时存在。
      previewRequestRef.current += 1;
      setMapSelectionId(null);
      setMapPreview(null);
      try {
        const res = await geographyClient.detail(id);
        setDetail(res);
        if (res?.entity) {
          void learningClient.recordEvent({
            module: "geography",
            entity_type: res.entity.entity_type ?? "place",
            entity_id: res.entity.id,
            entity_title: res.entity.name,
            action: "study",
          });
        }
      } catch (error) {
        setNotice(errorMessage(error));
      }
    },
    [home?.favorite_ids, setNotice],
  );
  const handleOpenEntity = useCallback(
    (id: string) => {
      void openEntity(id);
    },
    [openEntity],
  );
  const handleHistorySelectionChange = useCallback(
    (entityId: string, selection: GeographyHistorySelection | null) => {
      setSelectedHistory(selection ? { ...selection, entityId } : null);
    },
    [],
  );
  const retryHistory = useCallback(() => setHistoryRetry((count) => count + 1), []);
  const openLandforms = useCallback(() => setView("knowledge"), []);
  const returnToExplore = useCallback(() => setView("explore"), []);

  useEffect(() => {
    const entityId = detail?.entity.id;
    if (!entityId) {
      setLocationHistory(null);
      setHistoryError(null);
      setHistoryLoading(false);
      return;
    }
    let cancelled = false;
    setLocationHistory(null);
    setHistoryError(null);
    setHistoryLoading(true);
    void explorationClient.locationHistory(entityId).then(
      (context) => {
        if (!cancelled) setLocationHistory(context);
      },
      (error: unknown) => {
        if (!cancelled) setHistoryError(errorMessage(error));
      },
    ).finally(() => {
      if (!cancelled) setHistoryLoading(false);
    });
    return () => {
      cancelled = true;
    };
  }, [detail?.entity.id, historyRetry]);

  // V5 AppContext 桥：上报当前地点 / 视图
  useEffect(() => {
    if (!onContextChange || !active) return;
    if (!detail) {
      onContextChange({ module: "geography" });
      return;
    }
    onContextChange({
      module: "geography",
      page: view === "knowledge" ? "knowledge" : "explore",
      entity: {
        kind: "location",
        id: detail.entity.id,
        label: detail.entity.name,
      },
      selection: selectedHistory?.entityId === detail.entity.id
        ? {
            kind: "history-event",
            id: selectedHistory.eventId,
            text: selectedHistory.year
              ? `History event · ${selectedHistory.year}`
              : `History event · ${selectedHistory.eventId}`,
          }
        : null,
      view_state: {
        layer,
        ...(selectedHistory?.entityId === detail.entity.id
          ? {
              history_event_id: selectedHistory.eventId,
              history_story_id: selectedHistory.storyId,
              history_year: selectedHistory.year,
            }
          : {}),
      },
    });
  }, [active, detail, view, layer, selectedHistory, onContextChange]);
  useEffect(() => {
    if (active && intent?.entityId) void openEntity(intent.entityId);
  }, [active, intent, openEntity]);
  useEffect(() => {
    const searchQuery = intent?.query?.trim();
    if (!active || !searchQuery) {
      if (!active) searchRequestRef.current += 1;
      return;
    }
    const requestId = ++searchRequestRef.current;
    setDetail(null);
    setView("explore");
    setQuery(searchQuery);
    setSearchSubmitted(true);
    setSearchLoading(true);
    setSearchError(null);
    void geographyClient
      .search(searchQuery, null, 30)
      .then((groups) => {
        if (searchRequestRef.current === requestId) setSearchGroups(groups);
      })
      .catch((error: unknown) => {
        if (searchRequestRef.current === requestId) setSearchError(errorMessage(error));
      })
      .finally(() => {
        if (searchRequestRef.current === requestId) setSearchLoading(false);
      });
  }, [active, intent?.nonce, intent?.query, setNotice]);

  const runSearch = useCallback(async (text: string) => {
    const trimmed = text.trim();
    const requestId = ++searchRequestRef.current;
    if (!trimmed) {
      setSearchGroups([]);
      setSearchSubmitted(false);
      setSearchError(null);
      setSearchLoading(false);
      return;
    }
    setSearchSubmitted(true);
    setSearchLoading(true);
    setSearchError(null);
    try {
      const groups = await geographyClient.search(trimmed, null, 30);
      if (searchRequestRef.current === requestId) setSearchGroups(groups);
    } catch (error) {
      if (searchRequestRef.current === requestId) setSearchError(errorMessage(error));
    } finally {
      if (searchRequestRef.current === requestId) setSearchLoading(false);
    }
  }, []);

  const submitSearch = useCallback(() => runSearch(query), [query, runSearch]);

  const clearSearch = useCallback(() => {
    searchRequestRef.current += 1;
    setSearchGroups([]);
    setSearchSubmitted(false);
    setSearchLoading(false);
    setSearchError(null);
  }, []);

  const toggleFavorite = useCallback(async () => {
    if (!detail) return;
    try {
      const favorite = await geographyClient.toggleFavorite(detail.entity.id);
      setDetail({ ...detail, favorite });
      setHome((current) =>
        current
          ? {
              ...current,
              favorite_ids: favorite
                ? [...current.favorite_ids, detail.entity.id]
                : current.favorite_ids.filter((id) => id !== detail.entity.id),
            }
          : current,
      );
    } catch (error) {
      setNotice(errorMessage(error));
    }
  }, [detail, setNotice]);

  // ---- 地图选中实体的浮动信息卡 ----
  const loadMapPreview = useCallback(async (id: string) => {
    const cached = previewCacheRef.current.get(id);
    if (cached) {
      setMapPreview({ id, detail: cached, loading: false, error: null });
      return;
    }
    const requestId = ++previewRequestRef.current;
    setMapPreview({ id, detail: null, loading: true, error: null });
    try {
      const res = await geographyClient.detail(id);
      if (previewRequestRef.current !== requestId) return;
      if (res) previewCacheRef.current.set(id, res);
      setMapPreview({
        id,
        detail: res,
        loading: false,
        error: res ? null : "本地资料中没有找到这个地点。",
      });
    } catch (error) {
      if (previewRequestRef.current !== requestId) return;
      setMapPreview({ id, detail: null, loading: false, error: errorMessage(error) });
    }
  }, []);

  const handleMapSelect = useCallback(
    (id: string) => {
      // 第一次点击选中并显示浮动卡；再次点击同一实体时直接进入详情，保留原来的快捷路径。
      if (mapSelectionId === id && mapPreview?.detail) {
        void openEntity(id);
        return;
      }
      setMapSelectionId(id);
      setMapPreview((current) =>
        current?.id === id && !current.error
          ? current
          : { id, detail: null, loading: true, error: null },
      );
      void loadMapPreview(id);
    },
    [loadMapPreview, mapPreview?.detail, mapSelectionId, openEntity],
  );

  const closeMapPreview = useCallback(() => {
    previewRequestRef.current += 1;
    setMapSelectionId(null);
    setMapPreview(null);
  }, []);

  const retryMapPreview = useCallback(() => {
    if (!mapSelectionId) return;
    previewCacheRef.current.delete(mapSelectionId);
    void loadMapPreview(mapSelectionId);
  }, [loadMapPreview, mapSelectionId]);

  const toggleMapPreviewFavorite = useCallback(async () => {
    const id = mapPreview?.id ?? mapSelectionId;
    if (!id) return;
    try {
      const favorite = await geographyClient.toggleFavorite(id);
      setHome((current) =>
        current
          ? {
              ...current,
              favorite_ids: favorite
                ? [...new Set([...current.favorite_ids, id])]
                : current.favorite_ids.filter((favoriteId) => favoriteId !== id),
            }
          : current,
      );
      setMapPreview((current) =>
        current && current.id === id && current.detail
          ? { ...current, detail: { ...current.detail, favorite } }
          : current,
      );
      const cached = previewCacheRef.current.get(id);
      if (cached) previewCacheRef.current.set(id, { ...cached, favorite });
      setDetail((current) =>
        current && current.entity.id === id ? { ...current, favorite } : current,
      );
    } catch (error) {
      setNotice(errorMessage(error));
    }
  }, [mapPreview?.id, mapSelectionId, setNotice]);

  /** 索引/搜索结果 → 地图：选中并平滑定位，滚动到地图卡片；详情仍在信息卡里打开。 */
  const locateOnMap = useCallback(
    (id: string) => {
      if (!home) return;
      const point = home.map_points.find((item) => item.entity_id === id) ?? null;
      const entity = home.featured.find((item) => item.id === id) ?? null;
      const latitude = point?.coordinate.latitude ?? entity?.coordinates?.latitude ?? null;
      const longitude = point?.coordinate.longitude ?? entity?.coordinates?.longitude ?? null;
      if (latitude != null && longitude != null) {
        setMapFocus({ center: [longitude, latitude], zoom: 5 });
      }
      setMapSelectionId(id);
      setMapPreview((current) =>
        current?.id === id && !current.error
          ? current
          : { id, detail: null, loading: true, error: null },
      );
      void loadMapPreview(id);
      const reduceMotion =
        typeof window !== "undefined" &&
        typeof window.matchMedia === "function" &&
        window.matchMedia("(prefers-reduced-motion: reduce)").matches;
      // 只滚动地理页自己的滚动容器，避免把应用外壳（顶栏）一起滚走。
      const pane = document.querySelector<HTMLElement>(".page-pane.geography-pane");
      const card = document.querySelector<HTMLElement>(".geo-map-card");
      if (pane && card) {
        const paneRect = pane.getBoundingClientRect();
        const cardRect = card.getBoundingClientRect();
        if (cardRect.top < paneRect.top || cardRect.bottom > paneRect.bottom) {
          pane.scrollBy({
            top: cardRect.top - paneRect.top - 16,
            behavior: reduceMotion ? "auto" : "smooth",
          });
        }
      }
    },
    [home, loadMapPreview],
  );

  /** 「换一个」：从当前 featured 里随机挑一个不是今日推荐的地点打开详情。 */
  const exploreRandom = useCallback(() => {
    if (!home) return;
    const pool = home.featured.filter(
      (item) => item.id !== home.recommendation?.entity_id,
    );
    const candidates = pool.length ? pool : home.featured;
    if (!candidates.length) return;
    const pick = candidates[Math.floor(Math.random() * candidates.length)];
    void openEntity(pick.id);
  }, [home, openEntity]);

  useEffect(() => {
    if (!mapSelectionId) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") closeMapPreview();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [mapSelectionId, closeMapPreview]);

  // 加载完之前渲染骨架。之前的做法是用一份硬编码的 FALLBACK_HOME 顶着——
  // 那让「真实数据」和「假数据」在界面上无法区分。
  if (!home && homeError) {
    return (
      <div className="page-scroll geography-page">
        <section className="geo-load-error" role="alert">
          <h1>暂时无法载入 Geography</h1>
          <p>地理数据服务没有响应。已保存的本地资料不会被更改；请检查服务后重试。</p>
          <button type="button" onClick={() => void loadHome()}>重新载入</button>
        </section>
      </div>
    );
  }
  if (!home) {
    return (
      <div className="page-scroll geography-page">
        <div className="geo-explore-grid" aria-busy="true">
          <section className="geo-discovery-card">
            <div className="geo-section-label">
              <Question size={15} /> DAILY DISCOVERY · 今日探索
            </div>
            <h2>正在载入今日探索…</h2>
            <p className="geo-discovery-question">正在读取地理知识库。</p>
          </section>
          <div className="geo-discovery-card" />
        </div>
      </div>
    );
  }
  const displayedHome = home;
  const favoriteIdSet = new Set(displayedHome.favorite_ids);
  const previewPoint = mapSelectionId
    ? displayedHome.map_points.find((point) => point.entity_id === mapSelectionId) ?? null
    : null;
  const previewFavorite = mapPreview?.detail
    ? mapPreview.detail.favorite
    : mapSelectionId
      ? favoriteIdSet.has(mapSelectionId)
      : false;
  return (
    <div className="page-scroll geography-page">
      <header className="geo-hero">
        <div>
          <span className="geo-eyebrow">
            GEOGRAPHY EXPLORER · OFFLINE FIRST
          </span>
          <h1>
            探索这个世界，<em>继续追问为什么。</em>
          </h1>
          <p>
            从一个地点出发，沿着地形、气候、水系、人口与城市之间的关系，读懂它如何成为今天的样子。
          </p>
        </div>
        <div className="geo-hero-orbit" aria-hidden="true">
          <span>35.86° N</span>
          <i />
          <b>104.20° E</b>
        </div>
      </header>
      <nav className="geo-tabs" aria-label="Geography 页面">
        <button
          type="button"
          className={view === "explore" ? "selected" : ""}
          onClick={() => setView("explore")}
        >
          <Compass size={16} />
          Explore
        </button>
        <button
          type="button"
          className={view === "knowledge" ? "selected" : ""}
          onClick={() => setView("knowledge")}
        >
          <BookOpen size={16} />
          知识
        </button>
      </nav>
      {view === "knowledge" ? (
        <KnowledgePage
          onOpenEntity={handleOpenEntity}
          returnEntity={detail?.entity ?? null}
          onReturnToExplore={returnToExplore}
        />
      ) : detail ? (
        <EntityDetail
          detail={detail}
          onBack={() => setDetail(null)}
          onFavorite={() => void toggleFavorite()}
          onOpen={handleOpenEntity}
          historyContext={locationHistory}
          historyLoading={historyLoading}
          historyError={historyError}
          onAddToTravel={onAddToTravel}
          onOpenHistoryEvent={onOpenHistoryEvent}
          onOpenHistoryStory={onOpenHistoryStory}
          onHistorySelectionChange={handleHistorySelectionChange}
          onRetryHistory={retryHistory}
          onOpenLandforms={openLandforms}
          onReturnToTravel={onReturnToTravel}
          onReturnToHistory={onReturnToHistory}
        />
      ) : (
        <>
          <div className="geo-search-box">
            <MagnifyingGlass size={20} />
            <input
              value={query}
              onChange={(event) => {
                searchRequestRef.current += 1;
                setQuery(event.target.value);
                setSearchSubmitted(false);
                setSearchLoading(false);
                setSearchError(null);
                setSearchGroups([]);
              }}
              onKeyDown={(event) => {
                if (event.key === "Enter") void submitSearch();
              }}
              placeholder="搜索国家、省份、城市、河流、山脉、地貌……"
            />
            <button type="button" onClick={() => void submitSearch()}>
              搜索
            </button>
          </div>
          {searchLoading ? (
            <div className="geo-search-state geo-search-loading" role="status">
              <span className="geo-search-loading-bar" aria-hidden="true" />
              正在搜索本地 Geography 实体…
            </div>
          ) : null}
          {searchError ? (
            <div className="geo-search-state geo-search-error" role="alert">
              <p>搜索暂时失败：{searchError}</p>
              <button type="button" onClick={() => void runSearch(query)}>
                重新搜索
              </button>
            </div>
          ) : null}
          {searchSubmitted && !searchLoading && !searchError && searchGroups.length === 0 ? (
            <div className="geo-search-empty" role="status">
              <strong>没有找到“{query}”</strong>
              <p>
                搜索只覆盖已载入的本地地理实体，名称相似不会自动建立历史关联。可以换一个更短的关键词，或从下面这些已知地点开始：
              </p>
              <div className="geo-search-suggestions">
                {SEARCH_SUGGESTIONS.map((suggestion) => (
                  <button
                    type="button"
                    key={suggestion}
                    onClick={() => {
                      setQuery(suggestion);
                      void runSearch(suggestion);
                    }}
                  >
                    {suggestion}
                  </button>
                ))}
              </div>
            </div>
          ) : null}
          {searchGroups.length > 0 ? (
            <SearchResultsPanel
              groups={searchGroups}
              onOpen={handleOpenEntity}
              onLocate={locateOnMap}
              onClose={clearSearch}
              selectedId={mapSelectionId}
            />
          ) : null}
          <ExploreIndexRail
            featured={displayedHome.featured}
            favoriteIds={displayedHome.favorite_ids}
            onOpen={handleOpenEntity}
            onLocate={locateOnMap}
            selectedId={mapSelectionId}
          />
          <div className="geo-explore-grid">
            <section className="geo-discovery-card">
              <div className="geo-section-label">
                <Question size={15} /> DAILY DISCOVERY · 今日探索
              </div>
              <h2>{displayedHome.recommendation.title}</h2>
              <p className="geo-discovery-question">
                {displayedHome.recommendation.question}
              </p>
              <div className="geo-tag-row">
                {displayedHome.recommendation.tags.map((tag) => (
                  <span key={tag}>{tag}</span>
                ))}
              </div>
              <div className="geo-discovery-actions">
                <button
                  type="button"
                  className="geo-outline-button"
                  onClick={() =>
                    handleOpenEntity(displayedHome.recommendation.entity_id)
                  }
                >
                  开始探索 <ArrowRight size={16} />
                </button>
                <button
                  type="button"
                  className="geo-outline-button geo-shuffle-button"
                  onClick={exploreRandom}
                  title="从精选地点里随机换一个"
                >
                  <ArrowClockwise size={15} aria-hidden="true" /> 换一个
                </button>
              </div>
            </section>
            <GeoMap
              points={displayedHome.map_points}
              lines={displayedHome.map_lines}
              layer={layer}
              active={active}
              onSelect={handleMapSelect}
              onLayerChange={setLayer}
              selectedId={mapSelectionId}
              focus={mapFocus}
              overlay={
                mapPreview ? (
                  <MapEntityPreview
                    preview={mapPreview}
                    point={previewPoint}
                    favorite={previewFavorite}
                    onOpen={() => {
                      if (mapSelectionId) void openEntity(mapSelectionId);
                    }}
                    onToggleFavorite={() => void toggleMapPreviewFavorite()}
                    onClose={closeMapPreview}
                    onRetry={retryMapPreview}
                  />
                ) : null
              }
            />
          </div>
          <section className="geo-feature-section">
            <header>
              <div>
                <span className="geo-eyebrow">RECENTLY EXPLORED</span>
                <h2>最近查看</h2>
              </div>
              <span>{displayedHome.recent.length} 个地点</span>
            </header>
            {displayedHome.recent.length ? (
              <div className="geo-card-grid">
                {displayedHome.recent.map((item) => (
                  <EntityCard
                    item={item}
                    key={item.id}
                    favorite={favoriteIdSet.has(item.id)}
                    onOpen={handleOpenEntity}
                  />
                ))}
              </div>
            ) : (
              <p className="geo-feature-empty">
                还没有最近查看的地点。打开任意实体后，这里会记录你刚看过的内容。
              </p>
            )}
          </section>
        </>
      )}
    </div>
  );
}
