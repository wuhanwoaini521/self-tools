import { openUrl } from "@tauri-apps/plugin-opener";
import {
  Airplane,
  ArrowDown,
  ArrowRight,
  ArrowUp,
  ArrowSquareOut,
  Bed,
  CalendarBlank,
  CaretDown,
  Car,
  Clock,
  Cloud,
  CloudRain,
  CloudSun,
  Compass,
  DotsSixVertical,
  Footprints,
  ForkKnife,
  House,
  Info,
  MapPin,
  MapTrifold,
  Plus,
  ShieldCheck,
  ShieldWarning,
  Snowflake,
  Sparkle,
  Star,
  Sun,
  Tag,
  Train,
  Warning,
  WarningCircle,
  X,
} from "@phosphor-icons/react";
import {
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import type {
  Attraction,
  CityGuide,
  Food,
  ItineraryDay,
  ItineraryStop,
  Place,
  SourceLevel,
  TravelDateRange,
  GeoSearchGroup,
  TravelPlan,
  TravelPlanContext,
  TravelPlanPlace,
  TravelSource,
  VerifiedValue,
  WeatherForecast,
} from "../../types";
import { errorMessage, formatDateTime, isTauriRuntime } from "../../utils";
import { geographyClient } from "../geography/geographyClient";
import { explorationClient, type LocationHistoryContext } from "../exploration/explorationClient";
import {
  clearPendingTravelPlanIfUnchanged,
  pendingTravelPlanStorageKey,
  readPendingTravelPlan,
  reconcilePendingTravelPlan,
  sameTravelPlanContent,
  travelPlanWriterId,
  writePendingTravelPlan,
  type PendingTravelPlanDraft,
  type TravelPlanStorage,
} from "./travelPlanPersistence";
import { isTravelPlanConflict, travelClient } from "./travelClient";

function openLink(url: string) {
  if (isTauriRuntime()) void openUrl(url);
  else window.open(url, "_blank");
}

function Section({
  title,
  eyebrow,
  icon,
  children,
  className = "",
}: {
  title: string;
  eyebrow?: string;
  icon?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={`travel-section ${className}`}>
      <header className="travel-section-header">
        <div className="section-title-wrap">
          {icon ? <span className="section-title-icon">{icon}</span> : null}
          <div>
            {eyebrow ? <small>{eyebrow}</small> : null}
            <h2>{title}</h2>
          </div>
        </div>
      </header>
      <div className="travel-section-body">{children}</div>
    </section>
  );
}

function VerifiedBadge({ value }: { value: VerifiedValue }) {
  return (
    <span
      className={`travel-verified ${value.verified ? "" : "pending"}`}
      title={`${value.verified_sources} 个权威来源 · 主来源 ${value.primary_source}`}
    >
      {value.value}
      <i>{value.verified ? `✓ ${value.confidence}` : "待官方确认"}</i>
      {value.has_conflict ? <i className="travel-conflict">⚠ 多版本</i> : null}
    </span>
  );
}

function Stars({ level }: { level: SourceLevel }) {
  const count: Record<SourceLevel, number> = { S: 5, A: 4, B: 3, C: 2 };
  return (
    <span className="travel-stars" aria-label={`${level} 级来源`}>
      {Array.from({ length: 5 }, (_, index) => (
        <Star
          key={index}
          size={11}
          className={index < count[level] ? "filled" : "blank"}
          weight="fill"
        />
      ))}
    </span>
  );
}

function WeatherIcon({ text, size = 22 }: { text: string; size?: number }) {
  if (/雨|雷|阵雨/.test(text)) return <CloudRain size={size} weight="fill" />;
  if (/雪|冰雹/.test(text)) return <Snowflake size={size} weight="fill" />;
  if (/晴/.test(text)) return <Sun size={size} weight="fill" />;
  if (/多云|阴/.test(text)) return <CloudSun size={size} weight="fill" />;
  return <Cloud size={size} weight="fill" />;
}

function weekday(date: string) {
  const value = new Date(`${date}T00:00:00`);
  return Number.isNaN(value.getTime())
    ? date
    : new Intl.DateTimeFormat("zh-CN", { weekday: "short" }).format(value);
}

/** 计算两个经纬度坐标之间的球面距离（公里） */
function haversineDistance(
  coord1?: { latitude: number; longitude: number } | null,
  coord2?: { latitude: number; longitude: number } | null,
): number | null {
  if (!coord1 || !coord2) return null;
  const lat1 = coord1.latitude;
  const lon1 = coord1.longitude;
  const lat2 = coord2.latitude;
  const lon2 = coord2.longitude;
  if (
    !Number.isFinite(lat1) ||
    !Number.isFinite(lon1) ||
    !Number.isFinite(lat2) ||
    !Number.isFinite(lon2)
  )
    return null;

  const R = 6371; // 地球平均半径(km)
  const dLat = ((lat2 - lat1) * Math.PI) / 180;
  const dLon = ((lon2 - lon1) * Math.PI) / 180;
  const a =
    Math.sin(dLat / 2) * Math.sin(dLat / 2) +
    Math.cos((lat1 * Math.PI) / 180) *
      Math.cos((lat2 * Math.PI) / 180) *
      Math.sin(dLon / 2) *
      Math.sin(dLon / 2);
  const c = 2 * Math.atan2(Math.sqrt(a), Math.sqrt(1 - a));
  return Math.round(R * c * 10) / 10;
}

/** 估算交通耗时与推荐出行方式 */
function estimateTransitInfo(
  distKm: number,
  providedTravelTime?: string | null,
): {
  distanceText: string;
  durationText: string;
  modeText: string;
  icon: "car" | "walk" | "train";
} {
  if (providedTravelTime && providedTravelTime.trim()) {
    return {
      distanceText: `${distKm.toFixed(1)} km`,
      durationText: providedTravelTime,
      modeText: providedTravelTime,
      icon: "car",
    };
  }

  const roadKm = Math.max(0.3, Math.round(distKm * 1.3 * 10) / 10);
  if (roadKm <= 1.2) {
    const walkMin = Math.round(roadKm * 14);
    return {
      distanceText: `${roadKm} km`,
      durationText: `步行约 ${walkMin} 分钟`,
      modeText: "适合慢节奏步行游览",
      icon: "walk",
    };
  }
  if (roadKm <= 4.0) {
    const driveMin = Math.max(6, Math.round(roadKm * 2.8 + 3));
    return {
      distanceText: `${roadKm} km`,
      durationText: `打车约 ${driveMin} 分钟`,
      modeText: "推荐打车或骑行直达",
      icon: "car",
    };
  }
  const driveMin = Math.round(roadKm * 2.5 + 5);
  const metroMin = Math.round(roadKm * 3.2 + 8);
  return {
    distanceText: `${roadKm} km`,
    durationText: `驾车约 ${driveMin} 分钟 · 公共交通约 ${metroMin} 分钟`,
    modeText: "推荐地铁或网约车",
    icon: "train",
  };
}

/** 根据天气生成贴心的出行穿衣建议 */
function generateWeatherAdvice(forecast?: WeatherForecast | null): string {
  if (!forecast || forecast.days.length === 0) {
    return "出行前建议留意目的地临近天气预报，备好常用药品与雨具。";
  }
  const first = forecast.days[0];
  const maxTemp = Math.max(...forecast.days.map((d) => Number(d.temp_max) || 20));
  const minTemp = Math.min(...forecast.days.map((d) => Number(d.temp_min) || 12));
  const hasRain = forecast.days.some((d) => /雨|雷/.test(d.text_day));

  let advice = "";
  if (maxTemp >= 30) {
    advice = "白天气温较高，注意防晒补水与防暑降温，建议着清凉透气短袖或防晒服。";
  } else if (maxTemp <= 8) {
    advice = "天气寒冷，建议着厚羽绒服、保暖内衣并佩戴围巾手套，做好防寒保暖。";
  } else if (maxTemp - minTemp >= 10) {
    advice = `早晚温差较大（最低 ${minTemp}°C / 最高 ${maxTemp}°C），建议洋葱式穿衣法，随身备一件轻便外套。`;
  } else {
    advice = `平均气温约 ${Math.round((maxTemp + minTemp) / 2)}°C，气候适宜游览，穿着舒适卫衣或薄夹克即可。`;
  }

  if (hasRain) {
    advice += " 期间部分时段有降水，外出请随身携带雨伞并防滑。";
  }
  return advice;
}

function attractionKey(item: Attraction) {
  return (
    item.poi_id ||
    item.id ||
    item.normalized_name ||
    item.name.trim().toLocaleLowerCase()
  );
}

function attractionLabel(item: Attraction) {
  return (
    item.why_for_this_trip ||
    item.why_go ||
    item.intro ||
    (item.id?.startsWith("custom:")
      ? "你添加的地点；尚无核验资料。"
      : "当前没有可用的地点说明。")
  );
}

function uniqueAttractions(items: Attraction[]) {
  const seen = new Set<string>();
  return items.filter((item) => {
    const key = attractionKey(item).trim().toLocaleLowerCase();
    if (!item.name.trim() || seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

interface SavedTravelPlan {
  title: string;
  departure: string;
  notes: string;
  customPlaces: Record<string, TravelPlanPlace>;
  selectedKeys: string[];
  assignedDays: Record<string, number>;
  mustVisitKeys: string[];
}

function initialMustVisitKeys(catalog: Attraction[], userMustVisits: string[]) {
  return catalog
    .filter((item) =>
      userMustVisits.some(
        (wish) =>
          item.name.includes(wish) ||
          wish.includes(item.name) ||
          (item.intro && item.intro.includes(wish)),
      ),
    )
    .map(attractionKey);
}

function defaultTravelPlan(
  catalog: Attraction[],
  initial: Attraction[],
  dayCount: number,
  userMustVisits: string[],
): SavedTravelPlan {
  return {
    title: "",
    departure: "",
    notes: "",
    customPlaces: {},
    selectedKeys: initial.map(attractionKey),
    assignedDays: Object.fromEntries(
      initial.map((item) => [
        attractionKey(item),
        item.recommended_day && item.recommended_day <= dayCount
          ? item.recommended_day
          : 1,
      ]),
    ),
    mustVisitKeys: initialMustVisitKeys(catalog, userMustVisits),
  };
}

function normalizeSavedTravelPlan(
  plan: Partial<SavedTravelPlan> | null,
  catalog: Attraction[],
  initial: Attraction[],
  dayCount: number,
  userMustVisits: string[],
): SavedTravelPlan {
  const fallback = defaultTravelPlan(catalog, initial, dayCount, userMustVisits);
  if (!plan) return fallback;
  const customPlaces = Object.fromEntries(
    Object.entries(plan.customPlaces ?? {}).filter(
      ([key, place]) =>
        key.startsWith("custom:") &&
        typeof place?.name === "string" &&
        place.name.trim().length > 0,
    ).map(([key, place]) => [key, { name: place.name.trim(), note: place.note ?? "" }]),
  );
  const validKeys = new Set([...catalog.map(attractionKey), ...Object.keys(customPlaces)]);
  const selectedKeys = Array.isArray(plan.selectedKeys)
    ? Array.from(new Set(plan.selectedKeys.filter((key) => validKeys.has(key))))
    : fallback.selectedKeys;
  const assignedDays = Object.fromEntries(
    Object.entries(plan.assignedDays ?? {}).filter(
      ([key, day]) => validKeys.has(key) && Number.isInteger(day) && day >= 1 && day <= dayCount,
    ),
  );
  return {
    title: typeof plan.title === "string" ? plan.title : "",
    departure: typeof plan.departure === "string" ? plan.departure : "",
    notes: typeof plan.notes === "string" ? plan.notes : "",
    customPlaces,
    selectedKeys,
    assignedDays,
    mustVisitKeys: Array.from(
      new Set([
        ...(Array.isArray(plan.mustVisitKeys)
          ? plan.mustVisitKeys.filter((key) => validKeys.has(key))
          : []),
        ...fallback.mustVisitKeys,
      ]),
    ),
  };
}

function readLegacyTravelPlan(key: string): Partial<SavedTravelPlan> | null {
  try {
    const saved = localStorage.getItem(key);
    return saved ? JSON.parse(saved) as Partial<SavedTravelPlan> : null;
  } catch {
    return null;
  }
}

function toTravelPlan(
  context: TravelPlanContext,
  plan: SavedTravelPlan,
  revision: number,
): TravelPlan {
  return {
    context,
    title: plan.title,
    departure: plan.departure,
    notes: plan.notes,
    custom_places: plan.customPlaces,
    selected_keys: plan.selectedKeys,
    assigned_days: plan.assignedDays,
    must_visit_keys: plan.mustVisitKeys,
    revision,
    updated_at: 0,
  };
}

function fromTravelPlan(plan: TravelPlan): Partial<SavedTravelPlan> {
  return {
    title: plan.title,
    departure: plan.departure,
    notes: plan.notes,
    customPlaces: plan.custom_places,
    selectedKeys: plan.selected_keys,
    assignedDays: plan.assigned_days,
    mustVisitKeys: plan.must_visit_keys,
  };
}

function customAttraction(key: string, place: TravelPlanPlace): Attraction {
  return {
    id: key,
    name: place.name,
    normalized_name: place.name,
    poi_id: null,
    intro: place.note || null,
    why_go: null,
    why_for_this_trip: null,
    area: null,
    suggested_duration: null,
    opening_hours: null,
    ticket: null,
    reservation: null,
    tips: [],
    best_for: [],
    recommended_day: null,
    open_status: null,
    confidence: null,
    source_ids: [],
    coordinates: null,
  };
}

function localPlanStorage(): TravelPlanStorage | null {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

interface TravelGuideProps {
  guide: CityGuide;
  fromCache: boolean;
  persistInitialPlan?: boolean;
  userMustVisits?: string[];
  onPlanSaved?: () => void;
  onExploreHistory?: (query: string) => void;
  onOpenHistoryEvent?: (eventId: string) => void;
  onExploreGeography?: (query: string, entityId?: string) => void;
  explorationPlace?: {
    nonce: string;
    entityId: string;
    city: string;
    name: string;
    note: string;
  } | null;
  onPlanContextChange?: (viewState: Record<string, unknown> | null) => void;
}

type TabKey =
  | "itinerary"
  | "map"
  | "attractions"
  | "food"
  | "stay_transport"
  | "alerts_sources";

export function TravelGuide({
  guide,
  fromCache,
  persistInitialPlan = false,
  userMustVisits = [],
  onPlanSaved,
  onExploreHistory,
  onOpenHistoryEvent,
  onExploreGeography,
  explorationPlace,
  onPlanContextChange,
}: TravelGuideProps) {
  const { city, meta } = guide;
  const region = [city.province, city.country].filter(Boolean).join(" · ") || "区域未标注";
  const initialPicks = useMemo(
    () => (guide.top_picks.length > 0 ? guide.top_picks : guide.attractions).slice(0, 8),
    [guide.attractions, guide.top_picks],
  );
  const guideCatalog = useMemo(
    () => uniqueAttractions([...guide.attractions, ...guide.top_picks, ...guide.alternatives]),
    [guide],
  );

  const planContext: TravelPlanContext = {
    city: city.name,
    days: meta.days,
    date_range: meta.date_range,
  };
  const planContextKey = JSON.stringify(planContext);
  const userMustVisitsKey = JSON.stringify(userMustVisits);
  const storageKey = `travel-plan:${city.name}:${meta.days}:${meta.date_range?.start ?? ""}:${meta.date_range?.end ?? ""}`;
  const [plan, setPlan] = useState<SavedTravelPlan>(() =>
    defaultTravelPlan(guideCatalog, initialPicks, meta.days, userMustVisits),
  );
  const catalog = useMemo(
    () => uniqueAttractions([
      ...guideCatalog,
      ...Object.entries(plan.customPlaces).map(([key, place]) => customAttraction(key, place)),
    ]),
    [guideCatalog, plan.customPlaces],
  );
  const [planReady, setPlanReady] = useState(false);
  const [hasBackendPlan, setHasBackendPlan] = useState(false);
  const [planSaving, setPlanSaving] = useState(false);
  const [planRevision, setPlanRevision] = useState(0);
  const [planError, setPlanError] = useState<string | null>(null);
  const [localDraftError, setLocalDraftError] = useState<string | null>(null);
  const [planConflict, setPlanConflict] = useState(false);
  const [conflictRemote, setConflictRemote] = useState<TravelPlan | null>(null);
  const [planLoadAttempt, setPlanLoadAttempt] = useState(0);
  const [planSaveAttempt, setPlanSaveAttempt] = useState(0);
  const [customPlaceName, setCustomPlaceName] = useState("");
  const [customPlaceDay, setCustomPlaceDay] = useState(1);
  const savedPlanRef = useRef<string | null>(null);
  const revisionRef = useRef(0);
  const baseRevisionRef = useRef(0);
  const conflictRef = useRef(false);
  const mountedRef = useRef(true);
  const planRef = useRef(plan);
  const writerId = useMemo(() => travelPlanWriterId(), []);
  const saveQueueRef = useRef<Promise<void>>(Promise.resolve());
  const [activeTab, setActiveTab] = useState<TabKey>("itinerary");
  const [locationContexts, setLocationContexts] = useState<LocationHistoryContext[]>([]);
  const [geographyContext, setGeographyContext] = useState<GeoSearchGroup[]>([]);
  const [historyContextError, setHistoryContextError] = useState<string | null>(null);
  const [geographyContextError, setGeographyContextError] = useState<string | null>(null);
  const [contextLoading, setContextLoading] = useState(false);
  const consumedExplorationNonceRef = useRef<string | null>(null);
  const [selectedDayFilter, setSelectedDayFilter] = useState<number | null>(null);
  /** 收起的日程卡片（默认全部展开）；长行程时用来快速折叠。 */
  const [collapsedDays, setCollapsedDays] = useState<Set<number>>(() => new Set());
  /** 「在地图查看」指定的天数：切到地图 Tab 并预选该天。 */
  const [mapFocusDay, setMapFocusDay] = useState<number | null>(null);
  /** 拖动排序：正在拖的景点 key 与当前悬停的目标 key（只用于视觉反馈）。 */
  const [draggingStopKey, setDraggingStopKey] = useState<string | null>(null);
  const [dragOverStopKey, setDragOverStopKey] = useState<string | null>(null);
  const [attractionSearch, setAttractionSearch] = useState("");
  const [attractionCategory, setAttractionCategory] = useState<string>("all");

  planRef.current = plan;

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    setPlanReady(false);
    setHasBackendPlan(false);
    setPlanSaving(false);
    setPlanError(null);
    setPlanConflict(false);
    setConflictRemote(null);
    conflictRef.current = false;
    void (async () => {
      const remote = await travelClient.getPlan(planContext);
      if (cancelled) return;
      setHasBackendPlan(Boolean(remote));
      const storage = localPlanStorage();
      if (!storage) {
        setLocalDraftError("本机恢复副本不可用；行程仍会尝试保存到数据服务。");
      }
      const pending = storage
        ? readPendingTravelPlan(storage, storageKey, writerId)
        : { ok: true as const, draft: null };
      if (!pending.ok) {
        setPlanError(pending.error);
        return;
      }
      const fallback = defaultTravelPlan(
        guideCatalog,
        initialPicks,
        meta.days,
        userMustVisits,
      );
      const serverPlan = remote
        ? normalizeSavedTravelPlan(
            fromTravelPlan(remote),
            guideCatalog,
            initialPicks,
            meta.days,
            userMustVisits,
          )
        : fallback;
      const removeLegacy = () => {
        try {
          window.localStorage.removeItem(storageKey);
        } catch {
          // 后端已确定来源后，旧缓存清理只是尽力而为。
        }
      };

      if (pending.draft) {
        const reconciliation = reconcilePendingTravelPlan(remote, pending.draft);
        if (reconciliation.kind === "already_saved") {
          if (storage) {
            clearPendingTravelPlanIfUnchanged(
              storage,
              storageKey,
              pending.draft,
              writerId,
            );
          }
          setPlan(serverPlan);
          savedPlanRef.current = JSON.stringify(serverPlan);
          revisionRef.current = remote?.revision ?? 0;
          baseRevisionRef.current = revisionRef.current;
          if (remote) removeLegacy();
        } else {
          const recovered = normalizeSavedTravelPlan(
            fromTravelPlan(pending.draft.plan),
            guideCatalog,
            initialPicks,
            meta.days,
            userMustVisits,
          );
          setPlan(recovered);
          savedPlanRef.current = remote ? JSON.stringify(serverPlan) : null;
          revisionRef.current = remote?.revision ?? 0;
          baseRevisionRef.current =
            reconciliation.kind === "conflict"
              ? pending.draft.base_revision
              : revisionRef.current;
          if (reconciliation.kind === "conflict") {
            conflictRef.current = true;
            setPlanConflict(true);
            setConflictRemote(remote);
          }
        }
        setPlanReady(true);
        return;
      }

      if (remote) {
        setPlan(serverPlan);
        savedPlanRef.current = JSON.stringify(serverPlan);
        revisionRef.current = remote.revision;
        baseRevisionRef.current = remote.revision;
        removeLegacy();
      } else {
        const legacy = readLegacyTravelPlan(storageKey);
        const hydrated = normalizeSavedTravelPlan(
          legacy,
          guideCatalog,
          initialPicks,
          meta.days,
          userMustVisits,
        );
        setPlan(hydrated);
        revisionRef.current = 0;
        baseRevisionRef.current = 0;
        savedPlanRef.current = legacy || persistInitialPlan
          ? null
          : JSON.stringify(hydrated);
      }
      setPlanReady(true);
    })().catch((error: unknown) => {
      if (cancelled) return;
      setPlanError(errorMessage(error));
    });
    return () => {
      cancelled = true;
    };
  }, [
    guideCatalog,
    initialPicks,
    meta.days,
    planContextKey,
    planLoadAttempt,
    persistInitialPlan,
    storageKey,
    userMustVisitsKey,
    writerId,
  ]);

  useLayoutEffect(() => {
    if (!planReady || savedPlanRef.current === JSON.stringify(plan)) return;
    const storage = localPlanStorage();
    if (!storage) {
      setLocalDraftError("本机恢复副本不可用；行程仍会尝试保存到数据服务。");
      return;
    }
    const revision = baseRevisionRef.current;
    const draft: PendingTravelPlanDraft = {
      schema_version: 1,
      base_revision: revision,
      plan: toTravelPlan(planContext, plan, revision),
    };
    if (writePendingTravelPlan(storage, storageKey, draft, writerId)) {
      setLocalDraftError(null);
    } else {
      setLocalDraftError("本机恢复副本写入失败；请保持页面打开直到显示“已保存”。");
    }
  }, [plan, planContextKey, planReady, storageKey, writerId]);

  useEffect(() => {
    const serialized = JSON.stringify(plan);
    if (
      !planReady ||
      planConflict ||
      serialized === savedPlanRef.current
    ) {
      return;
    }
    const timer = window.setTimeout(() => {
      setPlanSaving(true);
      setPlanError(null);
      const snapshot = plan;
      const save = async () => {
        if (
          conflictRef.current ||
          JSON.stringify(planRef.current) !== serialized
        ) {
          if (mountedRef.current) setPlanSaving(false);
          return;
        }
        const storage = localPlanStorage();
        const baseRevision = revisionRef.current;
        const payload = toTravelPlan(planContext, snapshot, baseRevision);
        const sentDraft: PendingTravelPlanDraft = {
          schema_version: 1,
          base_revision: baseRevision,
          plan: payload,
        };
        if (storage && !writePendingTravelPlan(storage, storageKey, sentDraft, writerId)) {
          if (mountedRef.current) {
            setLocalDraftError("本机恢复副本写入失败；请保持页面打开直到显示“已保存”。");
          }
        }

        try {
          let saved: TravelPlan;
          try {
            saved = await travelClient.savePlan(payload);
          } catch (error) {
            if (!isTravelPlanConflict(error)) throw error;
            let latest: TravelPlan | null;
            try {
              latest = await travelClient.getPlan(planContext);
            } catch (reloadError) {
              conflictRef.current = true;
              if (mountedRef.current) {
                setPlanConflict(true);
                setConflictRemote(null);
                setPlanError(`保存版本冲突，且无法载入服务器版本：${errorMessage(reloadError)}`);
              }
              return;
            }
            if (!latest || !sameTravelPlanContent(latest, payload)) {
              conflictRef.current = true;
              if (mountedRef.current) {
                setPlanConflict(true);
                setConflictRemote(latest);
              }
              return;
            }
            saved = latest;
          }

          revisionRef.current = saved.revision;
          setPlanRevision(saved.revision);
          baseRevisionRef.current = saved.revision;
          setHasBackendPlan(true);
          savedPlanRef.current = serialized;
          if (storage) {
            clearPendingTravelPlanIfUnchanged(
              storage,
              storageKey,
              sentDraft,
              writerId,
            );
            try {
              window.localStorage.removeItem(storageKey);
            } catch {
              // The backend copy is authoritative after a successful save.
            }
            const current = planRef.current;
            if (JSON.stringify(current) !== serialized) {
              const currentDraft: PendingTravelPlanDraft = {
                schema_version: 1,
                base_revision: saved.revision,
                plan: toTravelPlan(planContext, current, saved.revision),
              };
              writePendingTravelPlan(storage, storageKey, currentDraft, writerId);
            }
          }
          if (mountedRef.current) {
            setPlanConflict(false);
            setConflictRemote(null);
            setPlanError(null);
          }
          // The parent can remain mounted after this guide is replaced or hidden.
          onPlanSaved?.();
        } catch (error) {
          if (mountedRef.current) setPlanError(errorMessage(error));
        } finally {
          if (mountedRef.current) setPlanSaving(false);
        }
      };
      saveQueueRef.current = saveQueueRef.current.then(save, save);
    }, 350);
    return () => window.clearTimeout(timer);
  }, [onPlanSaved, plan, planContextKey, planConflict, planReady, planSaveAttempt, storageKey, writerId]);

  useEffect(() => {
    let cancelled = false;
    setHistoryContextError(null);
    setGeographyContextError(null);
    setLocationContexts([]);
    setContextLoading(true);
    void geographyClient.search(city.name, null, 10).then(
      async (groups) => {
        if (cancelled) return;
        setGeographyContext(groups);
        const cities = groups
          .flatMap((group) => group.items)
          .filter((item) => item.entity_type === "city")
          .slice(0, 5);
        try {
          const contexts = await Promise.all(
            cities.map((item) => explorationClient.locationHistory(item.id)),
          );
          if (!cancelled) setLocationContexts(contexts.filter((item): item is LocationHistoryContext => Boolean(item)));
        } catch (error) {
          if (!cancelled) setHistoryContextError(errorMessage(error));
        } finally {
          if (!cancelled) setContextLoading(false);
        }
      },
      (error: unknown) => {
        if (!cancelled) {
          setGeographyContextError(errorMessage(error));
          setHistoryContextError(errorMessage(error));
          setContextLoading(false);
        }
      },
    );
    return () => {
      cancelled = true;
    };
  }, [city.name]);

  useEffect(() => {
    if (
      !planReady ||
      !explorationPlace ||
      consumedExplorationNonceRef.current === explorationPlace.nonce ||
      city.name.trim() !== explorationPlace.city.trim()
    ) return;
    consumedExplorationNonceRef.current = explorationPlace.nonce;
    const key = `custom:geography:${explorationPlace.entityId}`;
    setPlan((previous) => {
      const alreadySelected = previous.selectedKeys.includes(key);
      const alreadyStored = Boolean(previous.customPlaces[key]);
      if (alreadySelected && alreadyStored && previous.assignedDays[key]) return previous;
      return {
        ...previous,
        customPlaces: alreadyStored
          ? previous.customPlaces
          : { ...previous.customPlaces, [key]: { name: explorationPlace.name, note: explorationPlace.note } },
        selectedKeys: alreadySelected ? previous.selectedKeys : [...previous.selectedKeys, key],
        assignedDays: { ...previous.assignedDays, [key]: previous.assignedDays[key] ?? 1 },
      };
    });
  }, [city.name, explorationPlace, planReady]);

  const catalogByKey = useMemo(
    () => new Map(catalog.map((item) => [attractionKey(item), item])),
    [catalog],
  );

  const selectedAttractions = useMemo(
    () =>
      plan.selectedKeys
        .map((key) => catalogByKey.get(key))
        .filter((item): item is Attraction => Boolean(item)),
    [catalogByKey, plan.selectedKeys],
  );

  const assignedAttractions = useMemo(
    () =>
      selectedAttractions.map((item) => ({
        ...item,
        recommended_day:
          plan.assignedDays[attractionKey(item)] ?? item.recommended_day ?? 1,
      })),
    [plan.assignedDays, selectedAttractions],
  );

  useEffect(() => {
    if (!planReady) return;
    onPlanContextChange?.({
      departure: plan.departure,
      notes: plan.notes.slice(0, 1200),
      selected_stops: assignedAttractions.slice(0, 50).map((item) => ({
        id: attractionKey(item),
        name: item.name,
        day: item.recommended_day ?? 1,
        note: (item.intro || item.why_for_this_trip || "").slice(0, 240),
      })),
      must_visit_stops: plan.mustVisitKeys.slice(0, 50),
      plan_revision: revisionRef.current,
      plan_persisted: planReady && hasBackendPlan && !planConflict && JSON.stringify(plan) === savedPlanRef.current,
    });
  }, [assignedAttractions, hasBackendPlan, onPlanContextChange, plan, plan.departure, plan.mustVisitKeys, plan.notes, planReady, planConflict, planRevision]);

  const selectedKeys = new Set(plan.selectedKeys);
  const mustVisitSet = new Set(plan.mustVisitKeys);

  const addCustomStop = () => {
    const name = customPlaceName.trim();
    if (!name || !planReady) return;
    const id = globalThis.crypto?.randomUUID?.() ??
      `${Date.now()}-${Math.random().toString(36).slice(2)}`;
    const key = `custom:${id}`;
    const day = customPlaceDay;
    setPlan((previous) => ({
      ...previous,
      customPlaces: {
        ...previous.customPlaces,
        [key]: { name, note: "" },
      },
      selectedKeys: [...previous.selectedKeys, key],
      assignedDays: { ...previous.assignedDays, [key]: day },
    }));
    setCustomPlaceName("");
  };

  /** 拖动排序：把景点移动到目标天的第 targetIndex 位（同天重排 / 跨天移动共用）。 */
  const reorderStop = (key: string, targetDay: number, targetIndex: number) => {
    setPlan((previous) => {
      const dayOf = (selectedKey: string) =>
        previous.assignedDays[selectedKey] ?? catalogByKey.get(selectedKey)?.recommended_day ?? 1;
      const sourceDay = dayOf(key);

      if (sourceDay === targetDay) {
        const dayKeys = previous.selectedKeys.filter((selectedKey) => dayOf(selectedKey) === targetDay);
        const from = dayKeys.indexOf(key);
        if (from < 0) return previous;
        const clamped = Math.max(0, Math.min(dayKeys.length - 1, targetIndex));
        if (clamped === from) return previous;
        const order = dayKeys.slice();
        order.splice(from, 1);
        order.splice(clamped, 0, key);
        let cursor = 0;
        const selectedKeys = previous.selectedKeys.map((selectedKey) => {
          if (dayOf(selectedKey) !== targetDay) return selectedKey;
          return order[cursor++] ?? selectedKey;
        });
        return { ...previous, selectedKeys };
      }

      // 跨天：改 assignedDays，再插到目标天第 targetIndex 位；其它天的顺序保持不变。
      const assignedDays = { ...previous.assignedDays, [key]: targetDay };
      const without = previous.selectedKeys.filter((selectedKey) => selectedKey !== key);
      const dayOfAfter = (selectedKey: string) =>
        assignedDays[selectedKey] ?? catalogByKey.get(selectedKey)?.recommended_day ?? 1;
      const targetKeys = without.filter((selectedKey) => dayOfAfter(selectedKey) === targetDay);
      let insertAt = without.length;
      if (targetKeys.length > 0) {
        const clamped = Math.max(0, Math.min(targetKeys.length, targetIndex));
        insertAt =
          clamped >= targetKeys.length
            ? without.indexOf(targetKeys[targetKeys.length - 1]) + 1
            : without.indexOf(targetKeys[clamped]);
      }
      const selectedKeys = [
        ...without.slice(0, insertAt),
        key,
        ...without.slice(insertAt),
      ];
      return { ...previous, assignedDays, selectedKeys };
    });
  };

  const moveStop = (key: string, day: number, direction: -1 | 1) => {
    setPlan((previous) => {
      const dayKeys = previous.selectedKeys.filter((selectedKey) => {
        const item = catalogByKey.get(selectedKey);
        return (previous.assignedDays[selectedKey] ?? item?.recommended_day ?? 1) === day;
      });
      const index = dayKeys.indexOf(key);
      const neighbor = dayKeys[index + direction];
      if (!neighbor) return previous;
      const selectedKeys = previous.selectedKeys.slice();
      const first = selectedKeys.indexOf(key);
      const second = selectedKeys.indexOf(neighbor);
      [selectedKeys[first], selectedKeys[second]] = [selectedKeys[second], selectedKeys[first]];
      return { ...previous, selectedKeys };
    });
  };

  const loadLatestPlan = async () => {
    try {
      const remote = await travelClient.getPlan(planContext);
      const latest = remote
        ? normalizeSavedTravelPlan(
            fromTravelPlan(remote),
            guideCatalog,
            initialPicks,
            meta.days,
            userMustVisits,
          )
        : defaultTravelPlan(guideCatalog, initialPicks, meta.days, userMustVisits);
      const storage = localPlanStorage();
      if (storage) {
        try {
          storage.removeItem(pendingTravelPlanStorageKey(storageKey, writerId));
          storage.removeItem(storageKey);
        } catch {
          setLocalDraftError("服务器版本已载入，但本机旧草稿未能清除。");
        }
      }
      conflictRef.current = false;
      setPlanConflict(false);
      setConflictRemote(null);
      setPlan(latest);
      setHasBackendPlan(Boolean(remote));
      savedPlanRef.current = JSON.stringify(latest);
      revisionRef.current = remote?.revision ?? 0;
      baseRevisionRef.current = revisionRef.current;
      setPlanError(null);
      setPlanReady(true);
    } catch (error) {
      setPlanError(errorMessage(error));
    }
  };

  const keepLocalPlan = async () => {
    try {
      const remote = await travelClient.getPlan(planContext);
      const revision = remote?.revision ?? 0;
      const remoteState = remote
        ? normalizeSavedTravelPlan(
            fromTravelPlan(remote),
            guideCatalog,
            initialPicks,
            meta.days,
            userMustVisits,
          )
        : defaultTravelPlan(guideCatalog, initialPicks, meta.days, userMustVisits);
      revisionRef.current = revision;
      baseRevisionRef.current = revision;
      setHasBackendPlan(Boolean(remote));
      savedPlanRef.current = JSON.stringify(remoteState);
      const storage = localPlanStorage();
      if (storage) {
        const draft: PendingTravelPlanDraft = {
          schema_version: 1,
          base_revision: revision,
          plan: toTravelPlan(planContext, planRef.current, revision),
        };
        if (!writePendingTravelPlan(storage, storageKey, draft, writerId)) {
          setLocalDraftError("本机恢复副本写入失败；请保持页面打开直到显示“已保存”。");
        }
      } else {
        setLocalDraftError("本机恢复副本不可用；行程仍会尝试保存到数据服务。");
      }
      conflictRef.current = false;
      setPlanConflict(false);
      setConflictRemote(null);
      setPlanError(null);
      setPlanSaveAttempt((attempt) => attempt + 1);
    } catch (error) {
      setPlanError(errorMessage(error));
    }
  };

  const toggleMustVisit = (item: Attraction) => {
    const key = attractionKey(item);
    setPlan((prev) => {
      const nextMust = prev.mustVisitKeys.includes(key)
        ? prev.mustVisitKeys.filter((k) => k !== key)
        : [...prev.mustVisitKeys, key];
      // 如果设为必去且不在已选列表中，自动加入已选
      const nextSelected = prev.selectedKeys.includes(key)
        ? prev.selectedKeys
        : [...prev.selectedKeys, key];
      return {
        ...prev,
        mustVisitKeys: nextMust,
        selectedKeys: nextSelected,
        assignedDays: {
          ...prev.assignedDays,
          [key]: prev.assignedDays[key] ?? item.recommended_day ?? 1,
        },
      };
    });
  };

  // 构造每日行程数据，计算相邻景点间距离与交通
  const calculatedDays = useMemo(() => {
    return Array.from({ length: meta.days }, (_, index) => {
      const dayNum = index + 1;
      const originalDay = guide.itinerary_days.find((d) => d.day === dayNum);

      // 找到这一天的全部已选景点
      const daySpots = assignedAttractions.filter(
        (item) => (plan.assignedDays[attractionKey(item)] ?? item.recommended_day ?? 1) === dayNum,
      );

      // 计算相邻节点间距
      const stopsWithTransit: (ItineraryStop & {
        attractionRef?: Attraction;
        transitToNext?: ReturnType<typeof estimateTransitInfo> | null;
        distanceToNextKm?: number | null;
      })[] = [];

      let totalDistanceDayKm = 0;

      for (let i = 0; i < daySpots.length; i++) {
        const spot = daySpots[i];
        const nextSpot = daySpots[i + 1];
        let distKm: number | null = null;
        let transitInfo = null;

        if (nextSpot && spot.coordinates && nextSpot.coordinates) {
          distKm = haversineDistance(spot.coordinates, nextSpot.coordinates);
          if (distKm !== null) {
            totalDistanceDayKm += distKm * 1.3; // 估算道路修正系数
            transitInfo = estimateTransitInfo(distKm);
          }
        }

        const stopTime =
          i === 0
            ? "09:00"
            : i === 1
              ? "13:30"
              : i === 2
                ? "16:00"
                : "19:00";

        stopsWithTransit.push({
          name: spot.name,
          note: spot.why_for_this_trip || spot.why_go || spot.intro,
          time: stopTime,
          duration: spot.suggested_duration || "时长待定",
          area: spot.area,
          reason: attractionLabel(spot),
          travel_time: transitInfo ? transitInfo.durationText : null,
          attractionRef: spot,
          transitToNext: transitInfo,
          distanceToNextKm: distKm,
        });
      }

      return {
        day: dayNum,
        title: originalDay?.title || `第 ${dayNum} 天`,
        theme:
          originalDay?.theme ||
          daySpots
            .map((s) => s.area)
            .filter(Boolean)
            .slice(0, 2)
            .join(" + ") ||
          "城市特色深度游",
        stops: stopsWithTransit,
        totalDistanceKm: Math.round(totalDistanceDayKm * 10) / 10,
        spotCount: daySpots.length,
      };
    });
  }, [assignedAttractions, guide.itinerary_days, meta.days, plan.assignedDays]);

  const toggleDayCollapsed = (day: number) => {
    setCollapsedDays((current) => {
      const next = new Set(current);
      if (next.has(day)) next.delete(day);
      else next.add(day);
      return next;
    });
  };
  const allDaysCollapsed =
    calculatedDays.length > 0 && calculatedDays.every((day) => collapsedDays.has(day.day));
  const toggleAllDaysCollapsed = () => {
    setCollapsedDays(
      allDaysCollapsed ? new Set() : new Set(calculatedDays.map((day) => day.day)),
    );
  };

  // 全程总公里数估算
  const grandTotalDistanceKm = useMemo(() => {
    return Math.round(
      calculatedDays.reduce((sum, d) => sum + d.totalDistanceKm, 0) * 10,
    ) / 10;
  }, [calculatedDays]);

  // 景点图鉴筛选
  const filteredCatalog = useMemo(() => {
    return catalog.filter((item) => {
      const key = attractionKey(item);
      const isSelected = selectedKeys.has(key);
      const isMust = mustVisitSet.has(key);

      if (attractionCategory === "selected" && !isSelected) return false;
      if (attractionCategory === "must" && !isMust) return false;
      if (attractionCategory === "culture" && !item.best_for.some((b) => /历史|文化|古迹|博物馆/.test(b))) return false;
      if (attractionCategory === "nature" && !item.best_for.some((b) => /自然|山水|公园|湿地|湖/.test(b))) return false;
      if (attractionCategory === "photo" && !item.best_for.some((b) => /摄影|拍照|夜景|地标/.test(b))) return false;

      if (!attractionSearch.trim()) return true;
      const searchTarget = `${item.name} ${item.area ?? ""} ${item.best_for.join(" ")} ${item.intro ?? ""}`.toLocaleLowerCase();
      return searchTarget.includes(attractionSearch.trim().toLocaleLowerCase());
    });
  }, [attractionCategory, attractionSearch, catalog, mustVisitSet, selectedKeys]);

  const mainWarning =
    guide.quick_decisions.main_warning ||
    (guide.warnings[0] ? `${guide.warnings[0].title}：${guide.warnings[0].text}` : null);

  const weatherAdvice = useMemo(
    () => generateWeatherAdvice(guide.weather),
    [guide.weather],
  );
  const planIsDirty = planReady && JSON.stringify(plan) !== savedPlanRef.current;
  const planStatus = !planReady
    ? planError ? "failed" : "loading"
    : planConflict
      ? "conflict"
      : planError
        ? "failed"
        : planSaving
          ? "saving"
          : planIsDirty
            ? "editing"
            : hasBackendPlan
              ? "saved"
              : "draft";
  const planStatusLabel: Record<string, string> = {
    loading: "载入中",
    draft: "Draft · 尚未保存",
    editing: "Editing · 有修改未同步",
    saving: "Saving · 正在写入",
    saved: "Saved · 已保存",
    failed: "Failed · 保存失败",
    conflict: "Conflict · 版本不一致",
  };
  const historyContextItems = locationContexts.flatMap((context) =>
    context.events.map((detail) => ({
      geographyEntityId: context.geography.entity.id,
      event: detail,
    })),
  ).slice(0, 3);
  const geographyContextItems = geographyContext.flatMap((group) => group.items).slice(0, 3);

  return (
    <article className="travel-guide-immersive">
      {/* 沉浸式 Hero Poster 看板 */}
      <header className="travel-hero-poster">
        <div className="poster-backdrop-overlay" />
        <div className="poster-content">
          <div className="poster-kicker">
            {fromCache ? <span className="cache-badge">使用本地缓存攻略</span> : null}
            <span className="time-badge">
              {meta.updated_at > 0
                ? `资料更新于 ${formatDateTime(meta.updated_at)}`
                : "空白手排行程 · 暂无资料快照"}
            </span>
          </div>

          <div className="poster-title-row">
            <div className="poster-city-info">
              <h1>
                {plan.title.trim() || city.name}
                {city.name_en ? <span className="city-en">{city.name_en}</span> : null}
              </h1>
              <p className="city-region">
                <MapPin size={16} weight="fill" />
                {region}
                <span className="trip-duration-pill">
                  <CalendarBlank size={14} />
                  {meta.days} 天 {meta.days > 1 ? `${meta.days - 1} 晚` : "当日往返"}
                </span>
                {meta.date_range ? (
                  <span className="trip-dates-pill">
                    {meta.date_range.start} 至 {meta.date_range.end}
                  </span>
                ) : null}
              </p>
            </div>

            {/* 实时天气与穿衣预警胶囊 */}
            {guide.weather && guide.weather.days.length > 0 ? (
              <div className="poster-weather-widget">
                <div className="weather-primary">
                  <WeatherIcon
                    text={guide.weather.days[0]?.text_day || "天气"}
                    size={32}
                  />
                  <div className="weather-temp-wrap">
                    <strong>{guide.weather.days[0]?.temp_max}°</strong>
                    <span>
                      {guide.weather.days[0]?.text_day} · 最低 {guide.weather.days[0]?.temp_min}°
                    </span>
                  </div>
                </div>
                <div className="weather-advice">
                  <Info size={14} weight="bold" />
                  <span>{weatherAdvice}</span>
                </div>
                <div className="weather-mini-list">
                  {guide.weather.days.slice(0, 5).map((day) => (
                    <div key={day.date} className="weather-mini-day">
                      <small>{weekday(day.date)}</small>
                      <WeatherIcon text={day.text_day} size={15} />
                      <b>{day.temp_max}°</b>
                      <span>{day.temp_min}°</span>
                    </div>
                  ))}
                </div>
              </div>
            ) : null}
          </div>

          {guide.summary ? (
            <p className="poster-city-summary">{guide.summary}</p>
          ) : null}


        </div>
      </header>

      <div className={`travel-plan-status ${planStatus}`} data-state={planStatus} role="status" aria-live="polite">
        <span className="travel-plan-status-copy">
          <strong>{planStatusLabel[planStatus]}</strong>
          {planError ? <span>{planError}</span> : null}
          {!planReady && !planError ? <span>正在载入已保存的行程。</span> : null}
        </span>
        {planError ? (
          <button
            type="button"
            onClick={() => planReady
              ? setPlanSaveAttempt((attempt) => attempt + 1)
              : setPlanLoadAttempt((attempt) => attempt + 1)}
          >
            {planReady ? "重试保存" : "重新载入"}
          </button>
        ) : null}
      </div>
      {localDraftError ? (
        <p className="travel-local-draft-warning" role="alert">{localDraftError}</p>
      ) : null}

      {planConflict ? (
        <section className="travel-conflict-panel" role="alert">
          <div>
            <h2>这份行程已在别处更新</h2>
            <p>
              你的本机修改仍已保留。载入服务器版本会放弃本机草稿；覆盖操作会用本机内容替换服务器版本。
              {conflictRemote ? ` 服务器当前版本：${conflictRemote.revision}。` : " 服务器版本暂时无法读取。"}
            </p>
            {planError ? <p className="travel-conflict-error">{planError}</p> : null}
          </div>
          <div className="travel-conflict-actions">
            <button type="button" className="travel-secondary-action" onClick={() => void loadLatestPlan()}>
              载入服务器版本并放弃本机草稿
            </button>
            <button type="button" className="travel-primary-action" onClick={() => void keepLocalPlan()}>
              保留本机版本并覆盖服务器
            </button>
          </div>
        </section>
      ) : null}

      <section className="travel-plan-notebook" aria-label="编辑行程">
        <div className="travel-plan-fields">
          <label>
            <span>行程名称</span>
            <input
              aria-label="行程名称"
              maxLength={120}
              disabled={!planReady}
              value={plan.title}
              placeholder={`${city.name}行程`}
              onChange={(event) => setPlan((previous) => ({ ...previous, title: event.target.value }))}
            />
          </label>
          <label>
            <span>从哪里出发</span>
            <input
              aria-label="出发地"
              maxLength={120}
              disabled={!planReady}
              value={plan.departure}
              placeholder="例如：杭州东站"
              onChange={(event) => setPlan((previous) => ({ ...previous, departure: event.target.value }))}
            />
          </label>
          <label className="travel-plan-note-field">
            <span>行程备注</span>
            <textarea
              aria-label="行程备注"
              maxLength={4000}
              rows={2}
              disabled={!planReady}
              value={plan.notes}
              placeholder="写下同行人、节奏或需要记住的事"
              onChange={(event) => setPlan((previous) => ({ ...previous, notes: event.target.value }))}
            />
          </label>
        </div>

        <form
          className="travel-add-stop-form"
          onSubmit={(event) => {
            event.preventDefault();
            addCustomStop();
          }}
        >
          <label htmlFor="travel-custom-stop">添加一个地点</label>
          <div className="travel-add-stop-controls">
            <input
              id="travel-custom-stop"
              maxLength={200}
              disabled={!planReady}
              value={customPlaceName}
              onChange={(event) => setCustomPlaceName(event.target.value)}
              placeholder="景点、餐馆、街区都可以"
            />
            <select
              aria-label="新地点安排日期"
              disabled={!planReady}
              value={customPlaceDay}
              onChange={(event) => setCustomPlaceDay(Number(event.target.value))}
            >
              {Array.from({ length: meta.days }, (_, index) => (
                <option key={index + 1} value={index + 1}>Day {index + 1}</option>
              ))}
            </select>
            <button type="submit" disabled={!planReady || !customPlaceName.trim()}>
              <Plus size={16} /> 加入行程
            </button>
          </div>
        </form>
      </section>

      <section className="travel-context-bridge" aria-label="历史与地理资料">
        <div className="travel-context-heading">
          <h2>把城市放进更大的脉络</h2>
          <p>只显示本机资料库中实际匹配的内容；没有匹配时会明确说明。</p>
        </div>
        <div className="travel-context-columns">
          <div className="travel-context-column">
            <div className="travel-context-title-row">
              <h3>历史线索</h3>
              <button type="button" onClick={() => onExploreHistory?.(city.name)}>
                在历史中继续搜索
              </button>
            </div>
            {historyContextError ? <p>{historyContextError}</p> : null}
            {historyContextItems.length > 0 ? (
              <ul>
                {historyContextItems.map((item) => (
                  <li key={`${item.geographyEntityId}-${item.event.event.id}`}>
                    <button type="button" onClick={() => onOpenHistoryEvent?.(item.event.event.id)}>
                      <span>{item.event.event.name_zh_cn}</span>
                      <small>
                        {item.event.event.start_year ?? "年代待考"} · 已审核地点关联
                      </small>
                    </button>
                  </li>
                ))}
              </ul>
            ) : !historyContextError ? (
              <p>
                {contextLoading
                  ? "正在按地理实体的稳定标识查询历史关联…"
                  : locationContexts.some((context) => context.crosswalk)
                    ? "已找到对应历史地点，但当前没有可展示的已审核事件。"
                    : locationContexts.length
                      ? "有地理城市资料；尚无已审核的 History 地点关联，不按名称猜测。"
                      : "地理资料库没有返回可关联的城市实体；历史资料保持未匹配。"}
              </p>
            ) : null}
          </div>
          <div className="travel-context-column">
            <div className="travel-context-title-row">
              <h3>地理资料</h3>
              <button type="button" onClick={() => onExploreGeography?.(city.name)}>
                在地理中搜索
              </button>
            </div>
            {geographyContextError ? <p>{geographyContextError}</p> : null}
            {geographyContextItems.length > 0 ? (
              <ul>
                {geographyContextItems.map((item) => (
                  <li key={item.id}>
                    <button type="button" onClick={() => onExploreGeography?.(city.name, item.id)}>
                      <span>{item.name}</span>
                      <small>{item.entity_type}</small>
                    </button>
                  </li>
                ))}
              </ul>
            ) : !geographyContextError ? (
              <p>地理资料库暂无“{city.name}”的直接匹配项；可以继续浏览已编录的地理背景。</p>
            ) : null}
          </div>
        </div>
        {mainWarning ? <p className="travel-context-warning">{mainWarning}</p> : null}
      </section>

      {/* 沉浸式导航 Tab 切换栏 */}
      <nav className="travel-nav-tabs" role="tablist">
        <button
          type="button"
          role="tab"
          aria-selected={activeTab === "itinerary"}
          className={`nav-tab ${activeTab === "itinerary" ? "active" : ""}`}
          onClick={() => setActiveTab("itinerary")}
        >
          <CalendarBlank size={17} weight={activeTab === "itinerary" ? "fill" : "regular"} />
          <span>智能日程规划</span>
          <span className="tab-badge">{meta.days} 天</span>
        </button>

        <button
          type="button"
          role="tab"
          aria-selected={activeTab === "map"}
          className={`nav-tab ${activeTab === "map" ? "active" : ""}`}
          onClick={() => setActiveTab("map")}
        >
          <MapTrifold size={17} weight={activeTab === "map" ? "fill" : "regular"} />
          <span>路线与全景地图</span>
        </button>

        <button
          type="button"
          role="tab"
          aria-selected={activeTab === "attractions"}
          className={`nav-tab ${activeTab === "attractions" ? "active" : ""}`}
          onClick={() => setActiveTab("attractions")}
        >
          <Compass size={17} weight={activeTab === "attractions" ? "fill" : "regular"} />
          <span>全城景点图鉴</span>
          <span className="tab-badge">{catalog.length}</span>
        </button>

        <button
          type="button"
          role="tab"
          aria-selected={activeTab === "food"}
          className={`nav-tab ${activeTab === "food" ? "active" : ""}`}
          onClick={() => setActiveTab("food")}
        >
          <ForkKnife size={17} weight={activeTab === "food" ? "fill" : "regular"} />
          <span>地道风味美食</span>
          <span className="tab-badge">{guide.foods.length}</span>
        </button>

        <button
          type="button"
          role="tab"
          aria-selected={activeTab === "stay_transport"}
          className={`nav-tab ${activeTab === "stay_transport" ? "active" : ""}`}
          onClick={() => setActiveTab("stay_transport")}
        >
          <Bed size={17} weight={activeTab === "stay_transport" ? "fill" : "regular"} />
          <span>住宿与大交通</span>
        </button>

        <button
          type="button"
          role="tab"
          aria-selected={activeTab === "alerts_sources"}
          className={`nav-tab ${activeTab === "alerts_sources" ? "active" : ""}`}
          onClick={() => setActiveTab("alerts_sources")}
        >
          <ShieldCheck size={17} weight={activeTab === "alerts_sources" ? "fill" : "regular"} />
          <span>避坑锦囊与信源</span>
          <span className="tab-badge">{guide.sources.length}</span>
        </button>
      </nav>

      {/* ================= Tab 1: 智能日程规划 ================= */}
      {activeTab === "itinerary" && (
        <div className="travel-tab-pane tab-itinerary-pane">
          {/* 行程概览：天数 / 地点数 / 必去完成度 / 总里程 */}
          <div className="itinerary-summary" aria-label="行程概览">
            <span className="itinerary-summary-item">
              <b>{meta.days}</b> 天
            </span>
            <span className="itinerary-summary-item">
              <b>{assignedAttractions.length}</b> 个地点
            </span>
            {plan.mustVisitKeys.length ? (
              <span className="itinerary-summary-item">
                必去 <b>{plan.mustVisitKeys.filter((mustKey) => selectedKeys.has(mustKey)).length}</b>
                /{plan.mustVisitKeys.length}
              </span>
            ) : null}
            {grandTotalDistanceKm > 0 ? (
              <span className="itinerary-summary-item">
                约 <b>{grandTotalDistanceKm}</b> km
              </span>
            ) : null}
          </div>

          {/* 天数快速切换器 */}
          <div className="itinerary-days-filter">
            <button
              type="button"
              className={`day-filter-btn ${selectedDayFilter === null ? "active" : ""}`}
              onClick={() => setSelectedDayFilter(null)}
            >
              全部 {meta.days} 天日程
            </button>
            {calculatedDays.map((d) => (
              <button
                key={d.day}
                type="button"
                className={`day-filter-btn ${selectedDayFilter === d.day ? "active" : ""}`}
                onClick={() => setSelectedDayFilter(d.day)}
              >
                Day {d.day}
                <small>{d.theme.slice(0, 6)}</small>
              </button>
            ))}
            {calculatedDays.length > 1 ? (
              <button
                type="button"
                className="day-filter-btn day-collapse-all"
                aria-pressed={allDaysCollapsed}
                onClick={toggleAllDaysCollapsed}
              >
                {allDaysCollapsed ? "展开全部" : "收起全部"}
              </button>
            ) : null}
          </div>

          <div className="itinerary-timeline-days">
            {calculatedDays
              .filter((d) => selectedDayFilter === null || selectedDayFilter === d.day)
              .map((dayData) => (
                <div key={dayData.day} className="day-timeline-card">
                  <header className="day-header-banner">
                    <div className="day-title-left">
                      <span className="day-num-badge">DAY {dayData.day}</span>
                      <div className="day-title-text">
                        <h3>{dayData.title}</h3>
                        <span className="day-theme-tag">{dayData.theme}</span>
                      </div>
                    </div>
                    <div className="day-stats-right">
                      <span className="day-stat-chip">
                        <MapPin size={13} />
                        {dayData.spotCount} 个景点
                      </span>
                      {dayData.totalDistanceKm > 0 ? (
                        <span className="day-stat-chip distance-chip">
                          <Car size={13} />
                          日行程约 {dayData.totalDistanceKm} km
                        </span>
                      ) : null}
                    </div>
                    <div className="day-header-actions">
                      <button
                        type="button"
                        className="day-map-btn"
                        onClick={() => {
                          setMapFocusDay(dayData.day);
                          setActiveTab("map");
                        }}
                      >
                        <MapTrifold size={13} /> 在地图查看
                      </button>
                      <button
                        type="button"
                        className="day-collapse-btn"
                        aria-expanded={!collapsedDays.has(dayData.day)}
                        aria-controls={`day-body-${dayData.day}`}
                        onClick={() => toggleDayCollapsed(dayData.day)}
                      >
                        <CaretDown
                          size={14}
                          className={collapsedDays.has(dayData.day) ? "is-collapsed" : ""}
                        />
                        {collapsedDays.has(dayData.day) ? "展开" : "收起"}
                      </button>
                    </div>
                  </header>

                  <div
                    className="day-body"
                    id={`day-body-${dayData.day}`}
                    hidden={collapsedDays.has(dayData.day)}
                  >
                  {dayData.stops.length > 0 ? (
                    <div className="day-stops-flow">
                      {dayData.stops.map((stop, stopIdx) => {
                        const attraction = stop.attractionRef;
                        const stopKey = attraction ? attractionKey(attraction) : "";
                        const isCustomPlace = Boolean(attraction?.id?.startsWith("custom:"));
                        const isMust = attraction ? mustVisitSet.has(stopKey) : false;

                        return (
                          <div key={`${stop.name}-${stopIdx}`} className="timeline-stop-node">
                            {/* 景点卡片主体 */}
                            <div
                              className={`stop-card${draggingStopKey === stopKey ? " is-dragging" : ""}${dragOverStopKey === stopKey ? " is-drop-target" : ""}`}
                              onDragOver={(event) => {
                                if (!draggingStopKey || !attraction) return;
                                event.preventDefault();
                                event.dataTransfer.dropEffect = "move";
                                if (dragOverStopKey !== stopKey) setDragOverStopKey(stopKey);
                              }}
                              onDragLeave={() => {
                                if (dragOverStopKey === stopKey) setDragOverStopKey(null);
                              }}
                              onDrop={(event) => {
                                event.preventDefault();
                                const sourceKey =
                                  event.dataTransfer.getData("text/plain") || draggingStopKey;
                                if (sourceKey && attraction && sourceKey !== stopKey) {
                                  reorderStop(sourceKey, dayData.day, stopIdx);
                                }
                                setDraggingStopKey(null);
                                setDragOverStopKey(null);
                              }}
                            >
                              <div className="stop-badge-column">
                                {attraction ? (
                                  <span
                                    className="stop-drag-handle"
                                    draggable={planReady}
                                    aria-hidden="true"
                                    title="拖动排序（键盘请用上/下箭头）"
                                    onDragStart={(event) => {
                                      event.dataTransfer.setData("text/plain", stopKey);
                                      event.dataTransfer.effectAllowed = "move";
                                      setDraggingStopKey(stopKey);
                                    }}
                                    onDragEnd={() => {
                                      setDraggingStopKey(null);
                                      setDragOverStopKey(null);
                                    }}
                                  >
                                    <DotsSixVertical size={14} aria-hidden="true" />
                                  </span>
                                ) : null}
                                <span className="stop-order-num">
                                  {String(stopIdx + 1).padStart(2, "0")}
                                </span>
                                <span className="stop-time-label">{stop.time}</span>
                              </div>

                              <div className="stop-main-content">
                                <div className="stop-header-row">
                                  <div className="stop-title-group">
                                    <h4>{stop.name}</h4>
                                    {isMust && (
                                      <span className="must-visit-pill">
                                        <Star size={11} weight="fill" />
                                        必去心愿
                                      </span>
                                    )}
                                    {isCustomPlace ? <span className="travel-custom-place-tag">自定义 · 未核验</span> : null}
                                    {stop.area && (
                                      <span className="stop-area-tag">
                                        <MapPin size={11} />
                                        {stop.area}
                                      </span>
                                    )}
                                    {stop.duration && (
                                      <span className="stop-duration-tag">
                                        <Clock size={11} />
                                        建议 {stop.duration}
                                      </span>
                                    )}
                                  </div>

                                  {/* 景点调整与操作 */}
                                  {attraction && (
                                    <div className="stop-actions">
                                      <button
                                        type="button"
                                        className="btn-reorder-stop"
                                        aria-label={`将${stop.name}上移`}
                                        title="上移"
                                        disabled={!planReady || stopIdx === 0}
                                        onClick={() => moveStop(stopKey, dayData.day, -1)}
                                      >
                                        <ArrowUp size={13} />
                                      </button>
                                      <button
                                        type="button"
                                        className="btn-reorder-stop"
                                        aria-label={`将${stop.name}下移`}
                                        title="下移"
                                        disabled={!planReady || stopIdx === dayData.stops.length - 1}
                                        onClick={() => moveStop(stopKey, dayData.day, 1)}
                                      >
                                        <ArrowDown size={13} />
                                      </button>
                                      <label>
                                        <span>安排在</span>
                                        <select
                                          disabled={!planReady}
                                          value={dayData.day}
                                          onChange={(e) => {
                                            const newDay = Number(e.target.value);
                                            const key = attractionKey(attraction);
                                            setPlan((prev) => ({
                                              ...prev,
                                              assignedDays: {
                                                ...prev.assignedDays,
                                                [key]: newDay,
                                              },
                                            }));
                                          }}
                                        >
                                          {Array.from({ length: meta.days }, (_, dIdx) => (
                                            <option key={dIdx + 1} value={dIdx + 1}>
                                              Day {dIdx + 1}
                                            </option>
                                          ))}
                                        </select>
                                      </label>
                                      <button
                                        type="button"
                                        className="btn-remove-stop"
                                        disabled={!planReady}
                                        title="从行程中移出"
                                        aria-label={`从行程中移出${stop.name}`}
                                        onClick={() => {
                                          setPlan((prev) => ({
                                            ...prev,
                                            selectedKeys: prev.selectedKeys.filter((k) => k !== stopKey),
                                          }));
                                        }}
                                      >
                                        <X size={13} />
                                      </button>
                                    </div>
                                  )}
                                </div>

                                {isCustomPlace && attraction ? (
                                  <textarea
                                    className="travel-custom-place-note"
                                    aria-label={`${stop.name}备注`}
                                    maxLength={4000}
                                    disabled={!planReady}
                                    value={plan.customPlaces[stopKey]?.note ?? ""}
                                    placeholder="为这个地点记一条备注"
                                    onChange={(event) => setPlan((previous) => ({
                                      ...previous,
                                      customPlaces: {
                                        ...previous.customPlaces,
                                        [stopKey]: {
                                          ...previous.customPlaces[stopKey],
                                          note: event.target.value,
                                        },
                                      },
                                    }))}
                                  />
                                ) : stop.reason ? (
                                  <p className="stop-reason">{stop.reason}</p>
                                ) : null}

                                {/* 硬核事实标签：开放时间、门票、预约 */}
                                {attraction &&
                                (attraction.opening_hours ||
                                  attraction.ticket ||
                                  attraction.reservation) ? (
                                  <div className="stop-facts-row">
                                    {attraction.opening_hours && (
                                      <div className="fact-pill">
                                        <Clock size={12} />
                                        <span>开放时间：</span>
                                        <VerifiedBadge value={attraction.opening_hours} />
                                      </div>
                                    )}
                                    {attraction.ticket && (
                                      <div className="fact-pill">
                                        <Tag size={12} />
                                        <span>门票：</span>
                                        <VerifiedBadge value={attraction.ticket} />
                                      </div>
                                    )}
                                    {attraction.reservation && (
                                      <div className="fact-pill">
                                        <ShieldCheck size={12} />
                                        <span>预约：</span>
                                        <VerifiedBadge value={attraction.reservation} />
                                      </div>
                                    )}
                                  </div>
                                ) : null}
                              </div>
                            </div>

                            {/* 【核心亮点】相邻景点间距离与交通耗时指示条 */}
                            {stopIdx < dayData.stops.length - 1 && (
                              <div className="transit-connector-bar">
                                <div className="transit-line" />
                                <div className="transit-badge-wrap">
                                  {stop.transitToNext ? (
                                    <div className="transit-badge">
                                      {stop.transitToNext.icon === "walk" ? (
                                        <Footprints size={14} />
                                      ) : stop.transitToNext.icon === "train" ? (
                                        <Train size={14} />
                                      ) : (
                                        <Car size={14} />
                                      )}
                                      <span className="transit-dist">
                                        距下站 <b>{stop.transitToNext.distanceText}</b>
                                      </span>
                                      <span className="transit-divider">·</span>
                                      <span className="transit-mode">
                                        {stop.transitToNext.durationText}
                                      </span>
                                      <span className="transit-tip">
                                        ({stop.transitToNext.modeText})
                                      </span>
                                    </div>
                                  ) : (
                                    <div className="transit-badge transit-badge-simple">
                                      <ArrowRight size={13} />
                                      <span>前往下一景点</span>
                                    </div>
                                  )}
                                </div>
                              </div>
                            )}
                          </div>
                        );
                      })}
                    </div>
                  ) : (
                    <div className="day-empty-box">
                      <Compass size={24} />
                      <p>这一天还没有地点。可在上方添加自定义地点，或从景点图鉴中选择。</p>
                    </div>
                  )}
                  </div>
                </div>
              ))}
          </div>
        </div>
      )}

      {/* ================= Tab 2: 路线与全景地图 ================= */}
      {activeTab === "map" && (
        <div className="travel-tab-pane tab-map-pane">
          <GuideInteractiveMap
            attractions={assignedAttractions}
            days={calculatedDays}
            focusDay={mapFocusDay}
          />
        </div>
      )}

      {/* ================= Tab 3: 全城景点图鉴 ================= */}
      {activeTab === "attractions" && (
        <div className="travel-tab-pane tab-attractions-pane">
          {/* 筛选与搜索工具栏 */}
          <div className="attractions-toolbar">
            <div className="attraction-category-tabs">
              <button
                type="button"
                className={attractionCategory === "all" ? "active" : ""}
                onClick={() => setAttractionCategory("all")}
              >
                全部 ({catalog.length})
              </button>
              <button
                type="button"
                className={attractionCategory === "must" ? "active" : ""}
                onClick={() => setAttractionCategory("must")}
              >
                <Star size={13} weight="fill" />
                必去心愿 ({plan.mustVisitKeys.length})
              </button>
              <button
                type="button"
                className={attractionCategory === "selected" ? "active" : ""}
                onClick={() => setAttractionCategory("selected")}
              >
                已入行程 ({selectedAttractions.length})
              </button>
              <button
                type="button"
                className={attractionCategory === "culture" ? "active" : ""}
                onClick={() => setAttractionCategory("culture")}
              >
                人文古迹
              </button>
              <button
                type="button"
                className={attractionCategory === "nature" ? "active" : ""}
                onClick={() => setAttractionCategory("nature")}
              >
                自然风光
              </button>
              <button
                type="button"
                className={attractionCategory === "photo" ? "active" : ""}
                onClick={() => setAttractionCategory("photo")}
              >
                拍照打卡
              </button>
            </div>

            <div className="attraction-search-box">
              <input
                type="search"
                value={attractionSearch}
                onChange={(e) => setAttractionSearch(e.target.value)}
                placeholder={`在 ${city.name} 的所有景点与区域中搜索…`}
              />
            </div>
          </div>

          {/* 景点网格卡片列表 */}
          {filteredCatalog.length > 0 ? (
            <div className="attractions-gallery-grid">
              {filteredCatalog.map((item, idx) => {
                const key = attractionKey(item);
                const isSelected = selectedKeys.has(key);
                const isMust = mustVisitSet.has(key);
                const assignedDay = plan.assignedDays[key] ?? item.recommended_day ?? 1;

                return (
                  <article
                    key={key}
                    className={`attraction-gallery-card ${isSelected ? "in-itinerary" : ""} ${isMust ? "is-must-visit" : ""}`}
                  >
                    <div className="card-top-row">
                      <div className="card-index-box">
                        <span>{String(idx + 1).padStart(2, "0")}</span>
                      </div>
                      <div className="card-title-wrap">
                        <h3>{item.name}</h3>
                        {item.area && (
                          <span className="card-area-badge">
                            <MapPin size={12} />
                            {item.area}
                          </span>
                        )}
                      </div>
                      <button
                        type="button"
                        className={`btn-toggle-must ${isMust ? "active" : ""}`}
                        disabled={!planReady}
                        title={isMust ? "取消必去标记" : "设为必去景点"}
                        onClick={() => toggleMustVisit(item)}
                      >
                        <Star size={16} weight={isMust ? "fill" : "bold"} />
                      </button>
                    </div>

                    <p className="card-intro">{attractionLabel(item)}</p>

                    {item.best_for.length > 0 && (
                      <div className="card-tags-row">
                        {item.best_for.slice(0, 3).map((tag) => (
                          <span key={tag} className="meta-tag">
                            {tag}
                          </span>
                        ))}
                        {item.suggested_duration && (
                          <span className="meta-tag duration-tag">
                            <Clock size={11} />
                            {item.suggested_duration}
                          </span>
                        )}
                      </div>
                    )}

                    {/* 硬核事实 */}
                    {(item.opening_hours || item.ticket || item.reservation) && (
                      <div className="card-facts-block">
                        {item.opening_hours && (
                          <div>
                            <small>开放：</small>
                            <VerifiedBadge value={item.opening_hours} />
                          </div>
                        )}
                        {item.ticket && (
                          <div>
                            <small>门票：</small>
                            <VerifiedBadge value={item.ticket} />
                          </div>
                        )}
                        {item.reservation && (
                          <div>
                            <small>预约：</small>
                            <VerifiedBadge value={item.reservation} />
                          </div>
                        )}
                      </div>
                    )}

                    {/* 卡片底部操作按钮 */}
                    <div className="card-footer-actions">
                      {isSelected ? (
                        <div className="in-plan-actions">
                          <label>
                            <span>安排在</span>
                            <select
                              disabled={!planReady}
                              value={assignedDay}
                              onChange={(e) => {
                                const newDay = Number(e.target.value);
                                setPlan((prev) => ({
                                  ...prev,
                                  assignedDays: {
                                    ...prev.assignedDays,
                                    [key]: newDay,
                                  },
                                }));
                              }}
                            >
                              {Array.from({ length: meta.days }, (_, dIdx) => (
                                <option key={dIdx + 1} value={dIdx + 1}>
                                  Day {dIdx + 1}
                                </option>
                              ))}
                            </select>
                          </label>
                          <button
                            type="button"
                            className="btn-secondary-remove"
                            disabled={!planReady}
                            onClick={() => {
                              setPlan((prev) => ({
                                ...prev,
                                selectedKeys: prev.selectedKeys.filter((k) => k !== key),
                              }));
                            }}
                          >
                            移出
                          </button>
                        </div>
                      ) : (
                        <button
                          type="button"
                          className="btn-primary-add"
                          disabled={!planReady}
                          onClick={() => {
                            setPlan((prev) => ({
                              ...prev,
                              selectedKeys: [...prev.selectedKeys, key],
                              assignedDays: {
                                ...prev.assignedDays,
                                [key]: item.recommended_day && item.recommended_day <= meta.days
                                  ? item.recommended_day
                                  : 1,
                              },
                            }));
                          }}
                        >
                          + 加入行程规划
                        </button>
                      )}
                    </div>
                  </article>
                );
              })}
            </div>
          ) : (
            <div className="attractions-empty-state">
              <Compass size={32} />
              <p>{catalog.length === 0
                ? "当前攻略没有可编排的地点。可先在上方手动添加一个。"
                : "没有找到符合条件的地点，可以尝试搜索其他关键词。"}</p>
            </div>
          )}
        </div>
      )}

      {/* ================= Tab 4: 地道风味美食 ================= */}
      {activeTab === "food" && (
        <div className="travel-tab-pane tab-food-pane">
          <div className="food-banner-card">
            <div className="food-banner-icon">
              <ForkKnife size={24} weight="fill" />
            </div>
            <div className="food-banner-text">
              <h3>{city.name}的美食资料</h3>
              <p>{guide.food_summary || "当前攻略未提供可核验的美食资料。"}</p>
            </div>
          </div>

          {/* 代表性美食特产 */}
          {guide.foods.length > 0 ? (
            <Section title="城市代表性美食" eyebrow="SIGNATURE FLAVORS" icon={<Sparkle size={16} />}>
              <div className="foods-grid">
                {guide.foods.map((food, fIdx) => (
                  <div key={`${food.name}-${fIdx}`} className="food-item-card">
                    <div className="food-header">
                      <h4>{food.name}</h4>
                      {food.dish_type && (
                        <span className="dish-type-badge">{food.dish_type}</span>
                      )}
                      {food.area && (
                        <span className="food-area-tag">
                          <MapPin size={11} />
                          {food.area}
                        </span>
                      )}
                    </div>
                    {food.intro && <p className="food-intro">{food.intro}</p>}
                  </div>
                ))}
              </div>
            </Section>
          ) : (
            <p className="travel-empty-data">当前攻略没有可核验的美食资料；你仍可把餐馆作为自定义地点加入行程。</p>
          )}

          {/* 路线沿途精选餐厅 */}
          {guide.restaurants.length > 0 && (
            <Section title="路线沿途口碑餐饮" eyebrow="RESTAURANT PICKS" icon={<MapPin size={16} />}>
              <div className="restaurants-list">
                {guide.restaurants.map((place, pIdx) => (
                  <div key={`${place.name}-${pIdx}`} className="restaurant-card">
                    <div className="restaurant-num">{pIdx + 1}</div>
                    <div className="restaurant-body">
                      <div className="restaurant-head">
                        <h4>{place.name}</h4>
                        {place.route_day ? (
                          <span className="route-day-pill">
                            Day {place.route_day} 沿途
                            {place.distance_to_route ? ` · 距路线 ${place.distance_to_route}` : ""}
                          </span>
                        ) : null}
                        {place.area && (
                          <span className="place-area-pill">{place.area}</span>
                        )}
                      </div>
                      {place.signature_dish && (
                        <div className="restaurant-signature">
                          <small>招牌必点：</small>
                          <b>{place.signature_dish}</b>
                        </div>
                      )}
                      <p className="restaurant-why">
                        {place.why_pick || place.note || "当前资料未说明它与行程路线的关联。"}
                      </p>
                    </div>
                  </div>
                ))}
              </div>
            </Section>
          )}
        </div>
      )}

      {/* ================= Tab 5: 住宿与大交通 ================= */}
      {activeTab === "stay_transport" && (
        <div className="travel-tab-pane tab-stay-transport-pane">
          {/* 住宿区域推荐 */}
          <Section title="推荐下榻商圈" eyebrow="WHERE TO STAY" icon={<House size={16} />}>
            <div className="stay-areas-grid">
              {guide.accommodation_areas.length > 0 ? (
                guide.accommodation_areas.map((area, aIdx) => (
                  <div key={`${area.name}-${aIdx}`} className="stay-area-card">
                    <div className="stay-card-header">
                      <span className="stay-badge">推荐区域 {aIdx + 1}</span>
                      <h4>{area.name}</h4>
                      {area.budget && (
                        <span className="budget-tag">{area.budget}</span>
                      )}
                    </div>
                    {area.note && <p className="stay-note">{area.note}</p>}
                  </div>
                ))
              ) : (
                <p className="travel-empty-data">当前攻略没有提供可核验的住宿区域建议。</p>
              )}
            </div>
          </Section>

          {/* 全城交通指南 */}
          <Section title="抵达与全城交通" eyebrow="GETTING AROUND" icon={<Airplane size={16} />}>
            <div className="transport-guide-grid">
              {guide.transport.airport ? (
                <div className="transport-card">
                  <div className="transport-head"><Airplane size={18} /><h4>机场与大交通</h4></div>
                  <p>{guide.transport.airport}</p>
                </div>
              ) : null}
              {guide.transport.train_station ? (
                <div className="transport-card">
                  <div className="transport-head"><Train size={18} /><h4>火车站 / 高铁抵达</h4></div>
                  <p>{guide.transport.train_station}</p>
                </div>
              ) : null}
              {guide.transport.metro || guide.transport.bus_taxi ? (
                <div className="transport-card">
                  <div className="transport-head"><Car size={18} /><h4>市内交通</h4></div>
                  <p>{guide.transport.metro || guide.transport.bus_taxi}</p>
                </div>
              ) : null}
              {!guide.transport.airport && !guide.transport.train_station && !guide.transport.metro && !guide.transport.bus_taxi ? (
                <p className="travel-empty-data">当前攻略没有提供可核验的抵达或市内交通资料。</p>
              ) : null}
            </div>

            {guide.transport.tips.length > 0 && (
              <div className="transport-tips-card">
                <h4>
                  <Info size={15} />
                  交通出行小贴士
                </h4>
                <ul>
                  {guide.transport.tips.map((tip, tIdx) => (
                    <li key={tIdx}>{tip}</li>
                  ))}
                </ul>
              </div>
            )}
          </Section>
        </div>
      )}

      {/* ================= Tab 6: 避坑锦囊与信源 ================= */}
      {activeTab === "alerts_sources" && (
        <div className="travel-tab-pane tab-alerts-pane">
          {/* 重点预警与注意事项 */}
          <Section title="行前避坑指南与重点提醒" eyebrow="ALERTS & NOTICES" icon={<Warning size={16} />}>
            <div className="warnings-grid">
              {guide.warnings.length > 0 ? (
                guide.warnings.map((w, wIdx) => (
                  <div key={`${w.title}-${wIdx}`} className="warning-card">
                    <div className="warning-head">
                      <WarningCircle size={18} weight="fill" />
                      <h4>{w.title}</h4>
                    </div>
                    <p>{w.text}</p>
                  </div>
                ))
              ) : (
                <div className="warning-card">
                  <div className="warning-head">
                    <ShieldCheck size={18} weight="fill" />
                    <h4>当前没有已收集的行前提醒</h4>
                  </div>
                  <p>这不代表目的地没有风险；出发前请核对景点官方公告与预约规则。</p>
                </div>
              )}
            </div>
          </Section>

          {/* 本地深度游贴士 */}
          {guide.local_tips.length > 0 && (
            <Section title="本地人游览秘诀" eyebrow="LOCAL TIPS" icon={<Sparkle size={16} />}>
              <div className="local-tips-grid">
                {guide.local_tips.map((tip, tIdx) => (
                  <div key={`${tip.title}-${tIdx}`} className="local-tip-card">
                    <h4>{tip.title}</h4>
                    <p>{tip.text}</p>
                  </div>
                ))}
              </div>
            </Section>
          )}

          {/* 权威数据源报告 */}
          {guide.sources.length === 0 ? (
            <p className="travel-empty-data">当前攻略没有可展示的来源记录；请勿把未核验信息当作事实。</p>
          ) : (
            <Section title="事实交叉验证与数据信源" eyebrow="EVIDENCE & CITATIONS" icon={<ShieldCheck size={16} />}>
              <div className="evidence-summary-bar">
                <div className="evidence-item">
                  <span className="label">权威信息源</span>
                  <b>{guide.evidence.source_count} 个</b>
                </div>
                <div className="evidence-item">
                  <span className="label">全文交叉核验</span>
                  <b>{guide.evidence.verified_count} 条</b>
                </div>
                <div className="evidence-item">
                  <span className="label">数据质量评级</span>
                  <b className="quality-tag">{guide.evidence.quality || "高"}</b>
                </div>
              </div>

              <div className="sources-list">
                {guide.sources.map((source, sIdx) => (
                  <div key={`${source.url}-${sIdx}`} className="source-row">
                    <div className="source-info">
                      <b title={source.url}>{source.title}</b>
                      <div className="source-meta">
                        <Stars level={source.level} />
                        <span className={`level-pill level-${source.level.toLowerCase()}`}>
                          {source.level} 级
                        </span>
                        {source.state === "snippet_only" && (
                          <span className="snippet-pill">搜索摘要</span>
                        )}
                        <small>
                          {source.host} · {formatDateTime(source.fetched_at)}
                        </small>
                      </div>
                    </div>
                    <button
                      type="button"
                      className="btn-visit-source"
                      onClick={() => openLink(source.url)}
                    >
                      <ArrowSquareOut size={13} />
                      查看信源
                    </button>
                  </div>
                ))}
              </div>
            </Section>
          )}
        </div>
      )}
    </article>
  );
}

/** 交互式路线地图组件 */
function GuideInteractiveMap({
  attractions,
  days,
  focusDay = null,
}: {
  attractions: Attraction[];
  days: {
    day: number;
    title: string;
    stops: {
      name: string;
      attractionRef?: Attraction;
      distanceToNextKm?: number | null;
      transitToNext?: ReturnType<typeof estimateTransitInfo> | null;
    }[];
  }[];
  /** 从日程卡片「在地图查看」传入的天数；变化时切到该天。 */
  focusDay?: number | null;
}) {
  const [selectedDay, setSelectedDay] = useState<number | null>(days[0]?.day ?? null);
  useEffect(() => {
    if (focusDay == null) return;
    setSelectedDay(focusDay);
  }, [focusDay]);
  const [activeStopIndex, setActiveStopIndex] = useState<number | null>(null);

  // 筛选当前展示的 POI 点位
  const currentStops = useMemo(() => {
    if (selectedDay === null) {
      return days.flatMap((d) => d.stops);
    }
    const targetDay = days.find((d) => d.day === selectedDay);
    return targetDay ? targetDay.stops : [];
  }, [days, selectedDay]);

  const points = useMemo(() => {
    return currentStops
      .map((s) => s.attractionRef)
      .filter((item): item is Attraction => Boolean(item && item.coordinates));
  }, [currentStops]);

  const mapUrl = `https://uri.amap.com/marker?${new URLSearchParams({
    markers: points
      .map(
        (point) =>
          `${point.coordinates!.longitude},${point.coordinates!.latitude},${point.name}`,
      )
      .join("|"),
    src: "self-tools",
    callnative: "0",
  })}`;

  return (
    <Section title="行程路线与节点分布" eyebrow="ROUTE MAP" icon={<MapTrifold size={16} />}>
      <div className="travel-map-dashboard">
        {/* 天数筛选栏 */}
        <div className="travel-map-tabs">
          <button
            type="button"
            className={selectedDay === null ? "active" : ""}
            onClick={() => {
              setSelectedDay(null);
              setActiveStopIndex(null);
            }}
          >
            全部路线节点
          </button>
          {days.map((d) => (
            <button
              key={d.day}
              type="button"
              className={selectedDay === d.day ? "active" : ""}
              onClick={() => {
                setSelectedDay(d.day);
                setActiveStopIndex(null);
              }}
            >
              Day {d.day} 路线
            </button>
          ))}
        </div>

        {/* 地图与路线左右布局 */}
        <div className="travel-map-layout">
          <div className="travel-map-canvas-container">
            <div className="travel-map-unconfigured-box">
              <Compass size={32} />
              <h4>{points.length > 0 ? "交互底图暂不可用" : "还没有可定位的地点"}</h4>
              <p>{points.length > 0
                ? "为避免把地图凭据发送到浏览器，这里不加载第三方底图；下方列出已定位地点。"
                : "当前地点没有可用坐标，因此不会绘制推测路线。你仍可编辑行程顺序和日期。"}</p>
            </div>
          </div>

          {/* 右侧节点距离列表 */}
          <div className="travel-map-nodes-sidebar">
            <h4>已编排行程地点</h4>
            {currentStops.length > 0 ? (
              <div className="route-nodes-list">
                {currentStops.map((stop, idx) => {
                  const isHighlighted = activeStopIndex === idx;
                  return (
                    <div
                      key={`${stop.name}-${idx}`}
                      className={`route-node-item ${isHighlighted ? "active" : ""}`}
                      onClick={() => setActiveStopIndex(idx)}
                    >
                      <div className="node-head">
                        <span className="node-index">{idx + 1}</span>
                        <b>{stop.name}</b>
                        {stop.attractionRef?.area && (
                          <small>{stop.attractionRef.area}</small>
                        )}
                      </div>

                      {/* 距离下一站 */}
                      {stop.distanceToNextKm !== undefined &&
                      stop.distanceToNextKm !== null &&
                      idx < currentStops.length - 1 ? (
                        <div className="node-distance-to-next">
                          <ArrowRight size={11} />
                          <span>距下站 <b>{stop.distanceToNextKm.toFixed(1)} km</b></span>
                          {stop.transitToNext && (
                            <small>· {stop.transitToNext.durationText}</small>
                          )}
                        </div>
                      ) : null}
                    </div>
                  );
                })}
              </div>
            ) : (
              <p className="no-nodes-hint">当前所选日期还没有已编排的地点。</p>
            )}

            {points.length > 0 && (
              <button
                type="button"
                className="travel-link-button"
                onClick={() => openLink(mapUrl)}
              >
                <ArrowSquareOut size={14} />
                在高德地图查看已定位地点
              </button>
            )}
          </div>
        </div>
      </div>
    </Section>
  );
}
