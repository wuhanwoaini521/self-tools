import {
  ArrowRight,
  ArrowsClockwise,
  CalendarBlank,
  Compass,
  MapPin,
  Sparkle,
  Star,
  Tag,
  X,
} from "@phosphor-icons/react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { AppContextPayload } from "../ai/aiTypes";
import type {
  CityGuide,
  GuideSummary,
  TravelDateRange,
  TravelPlan,
  TravelPlanContext,
  TravelResearchEvent,
} from "../../types";
import { errorMessage } from "../../utils";
import { TravelGuide } from "./TravelGuide";
import { TravelProgress } from "./TravelProgress";
import {
  clearPendingTravelPlanIfUnchanged,
  listPendingTravelPlans,
  travelPlanWriterId,
  writePendingTravelPlan,
  type PendingTravelPlanEntry,
} from "./travelPlanPersistence";
import { travelClient } from "./travelClient";
import { learningClient } from "../learning/learningClient";

/**
 * Travel 模块（沉浸式升级版）：
 * 交互：输入城市 + 天数/日期 + 必去景点 + 旅行偏好 → 自动抓取与交叉验证 → 沉浸式多维度城市攻略。
 */

export const TRAVEL_PREFERENCES = [
  "历史人文",
  "地道美食",
  "绝美自然",
  "城市漫步",
  "拍照打卡",
  "亲子家庭",
  "深度文化",
  "避开拥挤",
  "夜生活",
];

export const POPULAR_CITIES = [
  { name: "杭州", must: ["西湖", "灵隐寺", "西溪湿地"] },
  { name: "成都", must: ["大熊猫繁育基地", "锦里", "宽窄巷子", "都江堰"] },
  { name: "西安", must: ["秦始皇帝陵博物院", "大雁塔", "西安城墙", "陕西历史博物馆"] },
  { name: "厦门", must: ["鼓浪屿", "环岛路", "南普陀寺", "沙坡尾"] },
  { name: "大理", must: ["洱海", "大理古城", "苍山", "喜洲古镇"] },
  { name: "重庆", must: ["洪崖洞", "解放碑", "磁器口", "长江索道"] },
  { name: "青岛", must: ["栈桥", "八大关", "青岛啤酒博物馆", "五四广场"] },
  { name: "南京", must: ["中山陵", "夫子庙", "玄武湖", "总统府"] },
];

/**
 * 起步行程：第一次打开、还没有任何行程时给出的三个可点击起点。
 * 只填表单（城市 / 天数 / 偏好），不发请求、不写数据；用户仍然可以改成自己的安排。
 * 参考 TREK 的 day-plan 起点与 AdventureLog 的 trip 模板做法。
 */
export const STARTER_TRIPS: ReadonlyArray<{
  city: string;
  kicker: string;
  days: number;
  preferences: string[];
}> = [
  { city: "杭州", kicker: "第一次去也不慌", days: 3, preferences: ["历史人文", "绝美自然"] },
  { city: "成都", kicker: "慢慢吃慢慢逛", days: 4, preferences: ["地道美食", "城市漫步"] },
  { city: "西安", kicker: "两天看一座古都", days: 2, preferences: ["历史人文", "深度文化"] },
];

interface TravelPageProps {
  active: boolean;
  setNotice: (message: string) => void;
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
  planIntent?: { context: TravelPlanContext; nonce: number } | null;
}

type ResearchState = "idle" | "running" | "done" | "error";

function emptyGuideForManualPlan(
  cityName: string,
  dayCount: number,
  dateRange: TravelDateRange | null,
): CityGuide {
  return {
    city: { name: cityName, name_en: null, province: null, country: null },
    summary: "",
    highlights: [],
    best_time: null,
    weather: null,
    districts: [],
    attractions: [],
    foods: [],
    restaurants: [],
    transport: {
      overview: null,
      airport: null,
      train_station: null,
      metro: null,
      bus_taxi: null,
      tips: [],
    },
    accommodation_areas: [],
    itineraries: { one_day: null, two_days: null, three_days: null },
    local_tips: [],
    warnings: [],
    quick_decisions: {
      best_area_to_stay: null,
      signature_food: null,
      trip_style: null,
      must_visit: [],
      main_warning: null,
    },
    top_picks: [],
    alternatives: [],
    itinerary_days: Array.from({ length: dayCount }, (_, index) => ({
      day: index + 1,
      title: `第 ${index + 1} 天`,
      theme: null,
      stops: [],
    })),
    food_summary: null,
    stay_areas: [],
    transport_summary: null,
    evidence: {
      source_count: 0,
      verified_count: 0,
      snippet_only_count: 0,
      conflict_count: 0,
      quality: "",
    },
    sources: [],
    meta: {
      generated_at: 0,
      updated_at: 0,
      days: dayCount,
      date_range: dateRange,
      llm_used: false,
      notes: ["空白手排行程；尚未收集城市资料。"],
    },
  };
}

function inclusiveDays(start: string, end: string): number | null {
  if (!start || !end || start > end) return null;
  const startTime = Date.parse(`${start}T00:00:00Z`);
  const endTime = Date.parse(`${end}T00:00:00Z`);
  if (!Number.isFinite(startTime) || !Number.isFinite(endTime)) return null;
  return Math.floor((endTime - startTime) / 86_400_000) + 1;
}

export function TravelPage({
  active,
  setNotice,
  onContextChange,
  onExploreHistory,
  onOpenHistoryEvent,
  onExploreGeography,
  explorationPlace,
  planIntent,
}: TravelPageProps & {
  onContextChange?: (ctx: AppContextPayload | null) => void;
}) {
  const [city, setCity] = useState("");
  const [days, setDays] = useState(3);
  const [tripStart, setTripStart] = useState("");
  const [tripEnd, setTripEnd] = useState("");
  const [mustVisits, setMustVisits] = useState<string[]>([]);
  const [mustVisitInput, setMustVisitInput] = useState("");
  const [preferences, setPreferences] = useState<string[]>([]);
  const [state, setState] = useState<ResearchState>("idle");
  const [events, setEvents] = useState<TravelResearchEvent[]>([]);
  const [guide, setGuide] = useState<CityGuide | null>(null);
  const [error, setError] = useState("");
  const [history, setHistory] = useState<GuideSummary[]>([]);
  const [savedPlans, setSavedPlans] = useState<TravelPlan[]>([]);
  const [pendingDrafts, setPendingDrafts] = useState<PendingTravelPlanEntry[]>([]);
  const [fromCache, setFromCache] = useState(false);
  const [persistInitialPlan, setPersistInitialPlan] = useState(false);
  const [pendingExplorationPlace, setPendingExplorationPlace] = useState<TravelPageProps["explorationPlace"]>(null);
  const [activePlanContext, setActivePlanContext] = useState<Record<string, unknown> | null>(null);
  const handledExplorationPlaceRef = useRef<string | null>(null);
  const handledPlanIntentRef = useRef<number | null>(null);
  const pollTimer = useRef<number | null>(null);

  /** 行程速览：只在填了城市后出现，用一句话说明「将按什么条件整理」。 */
  const planSummary = useMemo(() => {
    const name = city.trim();
    if (!name) return null;
    const chips: string[] = [`${days} 天${days > 1 ? ` ${days - 1} 晚` : "往返"}`];
    if (tripStart && tripEnd) chips.push(`${tripStart} 至 ${tripEnd}`);
    else if (tripStart) chips.push(`从 ${tripStart} 出发`);
    if (mustVisits.length) chips.push(`必去 ${mustVisits.length} 处`);
    if (preferences.length) {
      chips.push(
        preferences.length > 2
          ? `${preferences.slice(0, 2).join("、")} 等 ${preferences.length} 项偏好`
          : preferences.join("、"),
      );
    }
    return { name, chips };
  }, [city, days, tripStart, tripEnd, mustVisits.length, preferences]);

  const applyStarter = useCallback((starter: (typeof STARTER_TRIPS)[number]) => {
    setCity(starter.city);
    setDays(starter.days);
    setPreferences(starter.preferences);
    setState("idle");
  }, []);

  // V5 AppContext 桥：上报当前目的地 / 天数 / 偏好
  useEffect(() => {
    if (!onContextChange) return;
    if (!city.trim()) {
      onContextChange({ module: "travel" });
      return;
    }
    const name = city.trim();
    onContextChange({
      module: "travel",
      page: state === "done" && guide ? "guide" : "planner",
      entity: { kind: "destination", id: name, label: name },
      view_state: {
        days,
        date_range: tripStart && tripEnd ? { start: tripStart, end: tripEnd } : null,
        must_visits: mustVisits,
        preferences,
        from_cache: fromCache,
        ...(activePlanContext ?? {}),
      },
    });
  }, [city, state, guide, days, tripStart, tripEnd, mustVisits, preferences, fromCache, activePlanContext, onContextChange]);

  const reloadHistory = useCallback(async () => {
    try {
      setHistory(await travelClient.recentGuides());
    } catch {
      // Recent plans remain available even if guide history is temporarily unavailable.
    }
  }, []);
  const reloadPlans = useCallback(async () => {
    try {
      setSavedPlans(await travelClient.recentPlans(20));
    } catch {
      // Keep the last loaded list visible during a transient service failure.
    }
  }, []);
  const reloadPendingDrafts = useCallback(() => {
    try {
      setPendingDrafts(listPendingTravelPlans(window.localStorage));
    } catch {
      setPendingDrafts([]);
    }
  }, []);
  const handlePlanSaved = useCallback(() => {
    void reloadPlans();
    reloadPendingDrafts();
  }, [reloadPendingDrafts, reloadPlans]);

  useEffect(() => {
    setActivePlanContext(null);
  }, [guide?.city.name, guide?.meta.days, guide?.meta.updated_at, guide?.meta.date_range?.start, guide?.meta.date_range?.end]);

  useEffect(() => {
    if (!active) return;
    void reloadHistory();
    void reloadPlans();
    reloadPendingDrafts();
  }, [active, reloadHistory, reloadPendingDrafts, reloadPlans]);

  useEffect(() => {
    if (!explorationPlace || handledExplorationPlaceRef.current === explorationPlace.nonce) return;
    handledExplorationPlaceRef.current = explorationPlace.nonce;
    const targetCity = explorationPlace.city.trim();
    const currentCity = guide?.city.name.trim() || city.trim();
    if (currentCity && currentCity !== targetCity) {
      setNotice(`当前行程是「${currentCity}」，没有更改它。请先打开「${targetCity}」行程，再从 Geography 加入地点。`);
      return;
    }
    setPendingExplorationPlace(explorationPlace);
    if (guide || state === "running") return;

    const dateRange = tripStart && tripEnd ? { start: tripStart, end: tripEnd } : null;
    const rangeDays = dateRange ? inclusiveDays(dateRange.start, dateRange.end) : null;
    const dayCount = rangeDays ?? days;
    setCity(targetCity);
    setGuide(emptyGuideForManualPlan(targetCity, dayCount, dateRange));
    setEvents([]);
    setError("");
    setFromCache(false);
    setPersistInitialPlan(true);
    setState("done");
  }, [city, days, explorationPlace, guide, setNotice, state, tripEnd, tripStart]);

  // 离开页面时停止轮询
  useEffect(
    () => () => {
      if (pollTimer.current !== null) window.clearInterval(pollTimer.current);
    },
    [],
  );

  const stopPolling = useCallback(() => {
    if (pollTimer.current !== null) {
      window.clearInterval(pollTimer.current);
      pollTimer.current = null;
    }
  }, []);

  const togglePreference = (preference: string) => {
    setPreferences((previous) =>
      previous.includes(preference)
        ? previous.filter((item) => item !== preference)
        : [...previous, preference],
    );
  };

  const addMustVisit = (spotName: string) => {
    const trimmed = spotName.trim();
    if (!trimmed || mustVisits.includes(trimmed)) return;
    setMustVisits((prev) => [...prev, trimmed]);
  };

  const removeMustVisit = (spotName: string) => {
    setMustVisits((prev) => prev.filter((item) => item !== spotName));
  };

  const handleMustVisitKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter" || e.key === "," || e.key === "，") {
      e.preventDefault();
      if (mustVisitInput.trim()) {
        const parts = mustVisitInput.split(/[,，、]/);
        for (const part of parts) {
          if (part.trim()) addMustVisit(part.trim());
        }
        setMustVisitInput("");
      }
    }
  };

  const selectQuickCity = (cityItem: { name: string; must: string[] }) => {
    setCity(cityItem.name);
    // 预填部分经典必去
    setMustVisits(cityItem.must.slice(0, 3));
  };

  const startResearch = async (force: boolean) => {
    setPersistInitialPlan(false);
    const cityName = city.trim();
    if (!cityName) {
      setNotice("请输入要探索的城市，例如「杭州」");
      return;
    }
    if ((tripStart && !tripEnd) || (!tripStart && tripEnd)) {
      setNotice("请选择完整的起始和结束日期。");
      return;
    }
    const rangeDays = inclusiveDays(tripStart, tripEnd);
    if ((tripStart || tripEnd) && (!rangeDays || rangeDays > 7)) {
      setNotice("日期范围需有效，且暂时最多支持 7 天行程。");
      return;
    }
    const dateRange: TravelDateRange | null = rangeDays
      ? { start: tripStart, end: tripEnd }
      : null;
    const requestedDays = rangeDays ?? days;

    // 智能组装自然语言提示
    let nlQuery = cityName;
    const pendingInput = mustVisitInput.trim();
    const finalMustVisits = [...mustVisits];
    if (pendingInput) {
      const parts = pendingInput.split(/[,，、]/);
      for (const part of parts) {
        if (part.trim() && !finalMustVisits.includes(part.trim())) {
          finalMustVisits.push(part.trim());
        }
      }
    }

    if (finalMustVisits.length > 0) {
      nlQuery += ` 必去景点：${finalMustVisits.join("、")}`;
    }
    if (preferences.length > 0) {
      nlQuery += ` 偏好：${preferences.join("、")}`;
    }
    if (requestedDays) {
      nlQuery += ` ${requestedDays}天行程`;
    }

    stopPolling();
    setState("running");
    setGuide(null);
    setEvents([]);
    setFromCache(false);
    setError("");

    try {
      const request = {
        city: cityName,
        natural_language: nlQuery,
        today: (() => {
          const now = new Date();
          const pad = (value: number) => String(value).padStart(2, "0");
          return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
        })(),
        days: requestedDays,
        month: dateRange ? Number(dateRange.start.slice(5, 7)) : null,
        date_range: dateRange,
        preferences,
        force,
      };

      const id = await travelClient.researchStart(request);
      // 轮询进度直至完成
      const poll = async () => {
        try {
          const snapshot = await travelClient.researchProgress(id);
          if (!snapshot) {
            stopPolling();
            setState("error");
            setError("研究会话丢失，请重试。");
            return;
          }
          setEvents(snapshot.events);
          setFromCache(snapshot.from_cache);
          if (snapshot.done) {
            stopPolling();
            if (snapshot.guide) {
              setGuide(snapshot.guide);
              setState("done");
              void learningClient.recordEvent({
                module: "travel",
                entity_type: "guide",
                entity_id: cityName,
                entity_title: `${cityName} ${requestedDays}日游`,
                action: "study",
              });
            } else {
              setState("error");
              setError(snapshot.error ?? "研究失败，请查看设置后重试。");
            }
            void reloadHistory();
            void reloadPlans();
          }
        } catch (pollError) {
          stopPolling();
          setState("error");
          setError(errorMessage(pollError));
        }
      };
      pollTimer.current = window.setInterval(() => void poll(), 600);
      await poll();
    } catch (researchError) {
      stopPolling();
      setState("error");
      setError(errorMessage(researchError));
    }
  };

  const startBlankPlan = () => {
    const cityName = city.trim();
    if (!cityName) {
      setNotice("先填写目的地，再创建空白行程。");
      return;
    }
    if ((tripStart && !tripEnd) || (!tripStart && tripEnd)) {
      setNotice("请选择完整的起始和结束日期。");
      return;
    }
    const rangeDays = inclusiveDays(tripStart, tripEnd);
    if ((tripStart || tripEnd) && (!rangeDays || rangeDays > 7)) {
      setNotice("日期范围需有效，且暂时最多支持 7 天行程。");
      return;
    }
    const dateRange = rangeDays ? { start: tripStart, end: tripEnd } : null;
    const dayCount = rangeDays ?? days;
    setPersistInitialPlan(true);
    setGuide(emptyGuideForManualPlan(cityName, dayCount, dateRange));
    setEvents([]);
    setError("");
    setFromCache(false);
    setState("done");
  };

  const openSavedPlan = useCallback(async (plan: TravelPlan) => {
    const { context } = plan;
    let loaded: CityGuide | null = null;
    try {
      if (!context.date_range) {
        loaded = await travelClient.loadGuide(
          context.city,
          context.days,
          context.date_range,
        );
      }
    } catch {
      // A saved itinerary is independently useful when its research guide is unavailable.
    }
    setCity(context.city);
    setDays(context.days);
    setTripStart(context.date_range?.start ?? "");
    setTripEnd(context.date_range?.end ?? "");
    setMustVisits([]);
    setPersistInitialPlan(false);
    setGuide(loaded ?? emptyGuideForManualPlan(context.city, context.days, context.date_range));
    setEvents([]);
    setError("");
    setFromCache(false);
    setState("done");
  }, []);

  useEffect(() => {
    if (!active || !planIntent || handledPlanIntentRef.current === planIntent.nonce) return;
    handledPlanIntentRef.current = planIntent.nonce;
    const intent = planIntent;
    void travelClient.getPlan(intent.context).then(
      (plan) => {
        if (handledPlanIntentRef.current !== intent.nonce) return;
        if (!plan) {
          setNotice(`没有找到「${intent.context.city}」的这份已保存行程。`);
          return;
        }
        void openSavedPlan(plan);
      },
      (cause: unknown) => {
        if (handledPlanIntentRef.current === intent.nonce) setNotice(errorMessage(cause));
      },
    );
  }, [active, openSavedPlan, planIntent, setNotice]);

  const resumePendingPlan = (entry: PendingTravelPlanEntry) => {
    const currentWriter = travelPlanWriterId();
    if (entry.writerId !== currentWriter) {
      try {
        const storage = window.localStorage;
        if (!writePendingTravelPlan(storage, entry.contextKey, entry.draft, currentWriter)) {
          setNotice("无法恢复这份本机草稿；原有草稿仍保留。");
          return;
        }
        clearPendingTravelPlanIfUnchanged(
          storage,
          entry.contextKey,
          entry.draft,
          entry.writerId,
        );
      } catch {
        setNotice("无法恢复这份本机草稿；原有草稿仍保留。");
        return;
      }
    }
    const context = entry.draft.plan.context;
    setCity(context.city);
    setDays(context.days);
    setTripStart(context.date_range?.start ?? "");
    setTripEnd(context.date_range?.end ?? "");
    setMustVisits([]);
    setPersistInitialPlan(false);
    setGuide(emptyGuideForManualPlan(context.city, context.days, context.date_range));
    setEvents([]);
    setError("");
    setFromCache(false);
    setState("done");
    reloadPendingDrafts();
  };

  const openHistory = async (summary: GuideSummary) => {
    try {
      const loaded = await travelClient.loadGuide(
        summary.city,
        summary.days,
        summary.date_range,
      );
      if (loaded) {
        setCity(summary.city);
        setDays(summary.days);
        setTripStart(loaded.meta.date_range?.start ?? "");
        setTripEnd(loaded.meta.date_range?.end ?? "");
        setGuide(loaded);
        setEvents([]);
        setState("done");
        setFromCache(false);
        void learningClient.recordEvent({
          module: "travel",
          entity_type: "guide",
          entity_id: summary.city,
          entity_title: `${summary.city} ${summary.days}日游`,
          action: "study",
        });
      } else setNotice("本地没有找到该攻略，可能已被清除。");
    } catch (openError) {
      setNotice(errorMessage(openError));
    }
  };

  // 当前匹配城市的必去推荐
  const currentCityPopular = POPULAR_CITIES.find(
    (c) => c.name === city.trim() || city.trim().includes(c.name),
  );

  return (
    <div className="page-scroll travel-page page-shell">
      <header className="travel-hero">
        <div className="travel-hero-badge">
          <Sparkle size={14} weight="fill" />
          <span>沉浸式城市旅行探索与行程规划</span>
        </div>
        <h1>探索下一座城市</h1>
        <p>
          先查看已有城市资料，再按自己的节奏安排地点、日期与备注。模型、天气或地图资料不可用时，仍可手动编排和保存行程；缺失的信息会明确留空。
        </p>

        {/* 热门城市快捷推荐 */}
        <div className="travel-popular-cities">
          <span className="travel-popular-label">灵感目的地：</span>
          <div className="travel-popular-tags">
            {POPULAR_CITIES.map((item) => (
              <button
                key={item.name}
                type="button"
                className={`travel-popular-tag ${city.trim() === item.name ? "active" : ""}`}
                onClick={() => selectQuickCity(item)}
                disabled={state === "running"}
              >
                {item.name}
              </button>
            ))}
          </div>
        </div>
      </header>

      <section className="travel-search-container">
        {/* 第一行：城市目的地输入 */}
        <div className="travel-search-main">
          <div className="travel-input-group city-input-group">
            <label htmlFor="travel-city">
              <MapPin size={15} weight="bold" />
              <span>目的地城市</span>
            </label>
            <div className="travel-search-row">
              <input
                id="travel-city"
                value={city}
                onChange={(event) => setCity(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") void startResearch(false);
                }}
                placeholder="输入想去的城市，如：杭州、成都、西安、大理…"
                disabled={state === "running"}
              />
              <button
                type="button"
                className="travel-submit-btn"
                disabled={state === "running" || !city.trim()}
                onClick={() => void startResearch(false)}
              >
                <Compass size={17} weight="bold" />
                {state === "running" ? "研究中…" : "查看城市资料"}
              </button>
              <button
                type="button"
                className="travel-create-manual"
                disabled={state === "running" || !city.trim()}
                onClick={startBlankPlan}
              >
                创建空白行程
              </button>
              {state === "done" && guide ? (
                <button
                  className="travel-research-again"
                  title="跳过 24 小时缓存，重新搜索全部权威来源"
                  onClick={() => void startResearch(true)}
                >
                  <ArrowsClockwise size={15} />
                  重新研究
                </button>
              ) : null}
            </div>
          </div>
        </div>

        {/* 第二行：必去景点输入器 */}
        <div className="travel-must-visit-section">
          <label htmlFor="travel-must-input">
            <Star size={15} weight="fill" className="star-icon" />
            <span>必去景点 / 心愿地标</span>
            <small>（输入景点名称后按回车添加，优先编排进每日行程）</small>
          </label>
          <div className="travel-must-input-wrapper">
            <div className="travel-must-tags">
              {mustVisits.map((spot) => (
                <span key={spot} className="travel-must-chip">
                  <Star size={12} weight="fill" />
                  {spot}
                  <button
                    type="button"
                    aria-label={`移除必去景点 ${spot}`}
                    onClick={() => removeMustVisit(spot)}
                    disabled={state === "running"}
                  >
                    <X size={12} />
                  </button>
                </span>
              ))}
              <input
                id="travel-must-input"
                value={mustVisitInput}
                onChange={(e) => setMustVisitInput(e.target.value)}
                onKeyDown={handleMustVisitKeyDown}
                onBlur={() => {
                  if (mustVisitInput.trim()) {
                    addMustVisit(mustVisitInput.trim());
                    setMustVisitInput("");
                  }
                }}
                placeholder={
                  mustVisits.length === 0
                    ? "输入必去景点（如：西湖、灵隐寺），按回车添加…"
                    : "+ 继续添加必去景点"
                }
                disabled={state === "running"}
              />
            </div>
          </div>

          {/* 针对当前城市的推荐必去景点 */}
          {currentCityPopular && (
            <div className="travel-quick-spots">
              <span className="quick-spots-label">常见热门：</span>
              {currentCityPopular.must.map((spot) => (
                <button
                  key={spot}
                  type="button"
                  className={`travel-quick-spot-tag ${mustVisits.includes(spot) ? "selected" : ""}`}
                  onClick={() => {
                    if (mustVisits.includes(spot)) {
                      removeMustVisit(spot);
                    } else {
                      addMustVisit(spot);
                    }
                  }}
                  disabled={state === "running"}
                >
                  {mustVisits.includes(spot) ? "✓ " : "+ "}
                  {spot}
                </button>
              ))}
            </div>
          )}
        </div>

        {/* 第三行：行程天数、具体日期与出行偏好 */}
        <div className="travel-search-meta">
          <div className="travel-meta-item">
            <label htmlFor="travel-days">
              <CalendarBlank size={14} />
              <span>游玩天数</span>
            </label>
            <select
              id="travel-days"
              value={days}
              disabled={state === "running" || Boolean(tripStart && tripEnd)}
              onChange={(event) => setDays(Number(event.target.value))}
            >
              {[1, 2, 3, 4, 5, 6, 7].map((value) => (
                <option key={value} value={value}>
                  {value} 天 {value > 1 ? `${value - 1} 晚` : "往返"}
                </option>
              ))}
            </select>
          </div>

          <div className="travel-meta-item travel-date-item">
            <label>具体出行日期（可选）</label>
            <div
              className="travel-date-range"
              role="group"
              aria-label="行程日期范围"
            >
              <input
                type="date"
                aria-label="起始日期"
                value={tripStart}
                max={tripEnd || undefined}
                disabled={state === "running"}
                onChange={(event) => {
                  const value = event.target.value;
                  setTripStart(value);
                  const inferred = inclusiveDays(value, tripEnd);
                  if (inferred && inferred <= 7) setDays(inferred);
                }}
              />
              <span>至</span>
              <input
                type="date"
                aria-label="结束日期"
                value={tripEnd}
                min={tripStart || undefined}
                disabled={state === "running"}
                onChange={(event) => {
                  const value = event.target.value;
                  setTripEnd(value);
                  const inferred = inclusiveDays(tripStart, value);
                  if (inferred && inferred <= 7) setDays(inferred);
                }}
              />
            </div>
          </div>

          <div className="travel-meta-item travel-prefs-container">
            <label>
              <Tag size={14} />
              <span>旅行偏好风格</span>
            </label>
            <div className="travel-prefs">
              {TRAVEL_PREFERENCES.map((preference) => (
                <button
                  key={preference}
                  type="button"
                  className={preferences.includes(preference) ? "selected" : ""}
                  disabled={state === "running"}
                  onClick={() => togglePreference(preference)}
                >
                  {preference}
                </button>
              ))}
            </div>
          </div>
        </div>
      </section>

      {planSummary ? (
        <section className="travel-plan-summary" aria-label="行程速览">
          <div className="travel-plan-summary-main">
            <span className="travel-plan-summary-kicker">行程速览</span>
            <strong>{planSummary.name}</strong>
            <ul className="travel-plan-summary-chips">
              {planSummary.chips.map((chip) => (
                <li key={chip}>{chip}</li>
              ))}
            </ul>
          </div>
          <p className="travel-plan-summary-hint">
            点「查看城市资料」会按这些条件整理攻略；也可以直接创建空白行程手动编排。
          </p>
        </section>
      ) : null}

      {state === "idle" && !city.trim() && savedPlans.length === 0 && pendingDrafts.length === 0 && history.length === 0 ? (
        <section className="travel-starters" aria-label="起步行程">
          <header className="travel-starters-head">
            <h2>从一条现成的起步行程开始</h2>
            <p>点选后自动填好城市、天数与偏好，再按自己的节奏改。</p>
          </header>
          <div className="travel-starter-grid">
            {STARTER_TRIPS.map((starter) => (
              <button
                key={starter.city}
                type="button"
                className="travel-starter"
                onClick={() => applyStarter(starter)}
              >
                <span className="travel-starter-kicker">{starter.kicker}</span>
                <strong>{starter.city}</strong>
                <span className="travel-starter-meta">
                  {starter.days} 天 · {starter.preferences.join(" · ")}
                </span>
                <span className="travel-starter-action">
                  填入表单 <ArrowRight size={14} />
                </span>
              </button>
            ))}
          </div>
        </section>
      ) : null}

      {savedPlans.length > 0 || pendingDrafts.length > 0 ? (
        <section className="travel-saved-plan-rail" aria-label="最近行程和待同步草稿">
          <h2>最近行程</h2>
          {savedPlans.length > 0 ? (
            <ul className="travel-saved-plan-list">
              {savedPlans.slice(0, 8).map((plan) => (
                <li key={`${plan.context.city}-${plan.context.days}-${plan.context.date_range?.start ?? "any"}`}>
                  <button type="button" onClick={() => void openSavedPlan(plan)}>
                    <span className="saved-plan-title">{plan.title.trim() || `${plan.context.city}行程`}</span>
                    <small>
                      {plan.context.city} · {plan.context.date_range
                        ? `${plan.context.date_range.start} 至 ${plan.context.date_range.end}`
                        : `${plan.context.days} 天`}
                    </small>
                  </button>
                </li>
              ))}
            </ul>
          ) : null}
          {pendingDrafts.length > 0 ? (
            <div className="travel-pending-draft-list">
              {pendingDrafts.map((entry) => (
                <button
                  type="button"
                  key={`${entry.writerId}-${entry.contextKey}`}
                  onClick={() => resumePendingPlan(entry)}
                >
                  <span>{entry.draft.plan.title.trim() || `${entry.draft.plan.context.city}行程`}</span>
                  <small>有尚未同步的修改 · 点击恢复</small>
                </button>
              ))}
            </div>
          ) : null}
        </section>
      ) : null}

      {history.length > 0 ? (
        <section className="travel-history">
          <h2>
            <MapPin size={16} />
            <span>历史攻略库</span>
          </h2>
          <div className="travel-history-list">
            {history.slice(0, 6).map((summary) => (
              <button
                key={`${summary.city}-${summary.days}-${summary.date_range?.start ?? "any"}`}
                onClick={() => void openHistory(summary)}
              >
                <div className="history-icon-box">
                  <MapPin size={15} />
                </div>
                <div className="history-text">
                  <b>{summary.city}</b>
                  <small>
                    {summary.date_range
                      ? `${summary.date_range.start} ~ ${summary.date_range.end}`
                      : `${summary.days} 天行程`}
                  </small>
                </div>
              </button>
            ))}
          </div>
        </section>
      ) : null}

      {state === "running" ? (
        <section className="travel-research">
          <TravelProgress events={events} />
        </section>
      ) : null}

      {state === "error" ? (
        <section className="travel-error">
          <p>⚠ {error}</p>
          <button onClick={() => setState("idle")}>返回修改</button>
        </section>
      ) : null}

      {state === "done" && guide ? (
        <>
          {fromCache ? (
            <div className="travel-cache-note">
              <span>⚡ 已命中本地缓存（24 小时内生成），如需更新最新天气与开放状态，请点击「重新研究」。</span>
            </div>
          ) : null}
          <TravelGuide
            key={`${guide.city.name}:${guide.meta.days}:${guide.meta.updated_at}:${guide.meta.date_range?.start ?? ""}:${guide.meta.date_range?.end ?? ""}`}
            guide={guide}
            fromCache={fromCache}
            persistInitialPlan={persistInitialPlan}
            userMustVisits={mustVisits}
            onExploreHistory={onExploreHistory}
            onOpenHistoryEvent={onOpenHistoryEvent}
            onExploreGeography={onExploreGeography}
            explorationPlace={pendingExplorationPlace}
            onPlanContextChange={setActivePlanContext}
            onPlanSaved={handlePlanSaved}
          />
        </>
      ) : null}

      {state === "idle" ? (
        <section className="travel-first-run-note">
          <h2>从一张可编辑的行程开始</h2>
          <p>添加自定义地点、安排天数、调整顺序并写下备注。行程写入本机数据服务后，可从最近攻略中重新打开。</p>
          <span>无需配置 AI · 资料按实际来源显示 · 保存状态始终可见</span>
        </section>
      ) : null}
    </div>
  );
}
