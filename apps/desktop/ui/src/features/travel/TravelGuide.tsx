import { openUrl } from "@tauri-apps/plugin-opener";
import {
  Airplane,
  ArrowRight,
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
  Footprints,
  ForkKnife,
  House,
  Info,
  MapPin,
  MapTrifold,
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
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type {
  Attraction,
  CityGuide,
  Food,
  ItineraryDay,
  ItineraryStop,
  Place,
  SourceLevel,
  TravelDateRange,
  TravelSource,
  VerifiedValue,
  WeatherForecast,
} from "../../types";
import { formatDateTime, isTauriRuntime } from "../../utils";
import {
  loadAmap,
  type AmapMapInstance,
  type AmapNamespace,
  type AmapOverlay,
} from "../geography/AmapMap";

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
    "已从城市景点数据中找到，可加入行程后安排日期。"
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
  selectedKeys: string[];
  assignedDays: Record<string, number>;
  mustVisitKeys: string[];
}

function readTravelPlan(
  key: string,
  catalog: Attraction[],
  initial: Attraction[],
  dayCount: number,
  userMustVisits: string[],
): SavedTravelPlan {
  const initialMustKeys = catalog
    .filter((c) =>
      userMustVisits.some(
        (m) =>
          c.name.includes(m) ||
          m.includes(c.name) ||
          (c.intro && c.intro.includes(m)),
      ),
    )
    .map(attractionKey);

  try {
    const saved = localStorage.getItem(key);
    if (saved) {
      const plan = JSON.parse(saved) as SavedTravelPlan;
      const validKeys = new Set(catalog.map(attractionKey));
      return {
        selectedKeys: plan.selectedKeys.filter((itemKey) => validKeys.has(itemKey)),
        assignedDays: Object.fromEntries(
          Object.entries(plan.assignedDays).filter(
            ([itemKey, day]) =>
              validKeys.has(itemKey) && day >= 1 && day <= dayCount,
          ),
        ),
        mustVisitKeys: Array.from(
          new Set([
            ...(plan.mustVisitKeys || []).filter((k) => validKeys.has(k)),
            ...initialMustKeys,
          ]),
        ),
      };
    }
  } catch {
    // 本地存储异常保持静默
  }
  return {
    selectedKeys: initial.map(attractionKey),
    assignedDays: Object.fromEntries(
      initial.map((item) => [
        attractionKey(item),
        item.recommended_day && item.recommended_day <= dayCount
          ? item.recommended_day
          : 1,
      ]),
    ),
    mustVisitKeys: initialMustKeys,
  };
}

interface TravelGuideProps {
  guide: CityGuide;
  fromCache: boolean;
  userMustVisits?: string[];
  amapApiKey?: string | null;
  amapSecurityJsCode?: string | null;
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
  userMustVisits = [],
  amapApiKey,
  amapSecurityJsCode,
}: TravelGuideProps) {
  const { city, meta } = guide;
  const region = [city.province, city.country].filter(Boolean).join(" · ") || "中国";
  const initialPicks = (
    guide.top_picks.length > 0 ? guide.top_picks : guide.attractions
  ).slice(0, 8);
  const catalog = useMemo(
    () => uniqueAttractions([...guide.attractions, ...guide.top_picks, ...guide.alternatives]),
    [guide],
  );

  const storageKey = `travel-plan:${city.name}:${meta.days}:${meta.date_range?.start ?? ""}:${meta.date_range?.end ?? ""}`;
  const [plan, setPlan] = useState<SavedTravelPlan>(() =>
    readTravelPlan(storageKey, catalog, initialPicks, meta.days, userMustVisits),
  );
  const [activeTab, setActiveTab] = useState<TabKey>("itinerary");
  const [selectedDayFilter, setSelectedDayFilter] = useState<number | null>(null);
  const [attractionSearch, setAttractionSearch] = useState("");
  const [attractionCategory, setAttractionCategory] = useState<string>("all");

  useEffect(() => {
    try {
      localStorage.setItem(storageKey, JSON.stringify(plan));
    } catch {
      // 存储异常
    }
  }, [plan, storageKey]);

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

  const selectedKeys = new Set(plan.selectedKeys);
  const mustVisitSet = new Set(plan.mustVisitKeys);

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
          duration: spot.suggested_duration || "约 2 小时",
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

  return (
    <article className="travel-guide-immersive">
      {/* 沉浸式 Hero Poster 看板 */}
      <header className="travel-hero-poster">
        <div className="poster-backdrop-overlay" />
        <div className="poster-content">
          <div className="poster-kicker">
            <span className="edition-badge">
              <Sparkle size={13} weight="fill" />
              CITY EXPLORER EDITION
            </span>
            {fromCache ? <span className="cache-badge">已命中 24h 本地快照</span> : null}
            <span className="time-badge">
              更新于 {formatDateTime(meta.updated_at)}
            </span>
          </div>

          <div className="poster-title-row">
            <div className="poster-city-info">
              <h1>
                {city.name}
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
                    text={guide.weather.days[0]?.text_day || "晴"}
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

          {/* 城市全景数据指标条 */}
          <div className="poster-stats-strip">
            <div className="stat-pill">
              <span className="stat-num">{selectedAttractions.length}</span>
              <span className="stat-label">已编排景点</span>
            </div>
            <div className="stat-pill">
              <span className="stat-num">{catalog.length}</span>
              <span className="stat-label">全城发现地标</span>
            </div>
            <div className="stat-pill">
              <span className="stat-num">{guide.foods.length}</span>
              <span className="stat-label">地道特色风味</span>
            </div>
            <div className="stat-pill">
              <span className="stat-num">{grandTotalDistanceKm > 0 ? `${grandTotalDistanceKm} km` : "市区集约"}</span>
              <span className="stat-label">规划路线总里程</span>
            </div>
            <div className="stat-pill">
              <span className="stat-num">{guide.accommodation_areas.length || 2}</span>
              <span className="stat-label">推荐住宿圈</span>
            </div>
          </div>
        </div>
      </header>

      {/* 核心结论速览卡片 */}
      <section className="travel-quick-decisions-grid">
        <div className="decision-card stay-card">
          <div className="decision-icon">
            <House size={18} weight="fill" />
          </div>
          <div className="decision-body">
            <small>建议住哪</small>
            <b>
              {guide.quick_decisions.best_area_to_stay ||
                guide.accommodation_areas[0]?.name ||
                "近核心地铁站与商圈"}
            </b>
            <p>
              {guide.accommodation_areas[0]?.note ||
                "出行便利，近主要换乘枢纽与餐饮集聚区。"}
            </p>
          </div>
        </div>

        <div className="decision-card food-card">
          <div className="decision-icon">
            <ForkKnife size={18} weight="fill" />
          </div>
          <div className="decision-body">
            <small>必吃美食</small>
            <b>
              {guide.quick_decisions.signature_food ||
                guide.foods[0]?.name ||
                "本地传统特色菜肴"}
            </b>
            <p>
              {guide.food_summary ||
                guide.foods[0]?.intro ||
                "寻味老字号与地道街头小馆。"}
            </p>
          </div>
        </div>

        <div className="decision-card must-card">
          <div className="decision-icon">
            <Star size={18} weight="fill" />
          </div>
          <div className="decision-body">
            <small>核心地标</small>
            <b>
              {plan.mustVisitKeys.length > 0
                ? plan.mustVisitKeys
                    .map((k) => catalogByKey.get(k)?.name)
                    .filter(Boolean)
                    .slice(0, 3)
                    .join(" · ")
                : guide.quick_decisions.must_visit.slice(0, 3).join(" · ") ||
                  selectedAttractions.slice(0, 3).map((a) => a.name).join(" · ")}
            </b>
            <p>本趟行程的核心灵魂与打卡必到之地。</p>
          </div>
        </div>

        {mainWarning ? (
          <div className="decision-card alert-card">
            <div className="decision-icon">
              <ShieldWarning size={18} weight="fill" />
            </div>
            <div className="decision-body">
              <small>行前避坑提醒</small>
              <b>{mainWarning}</b>
              <p>门票预约、错峰出行与官方规则留意。</p>
            </div>
          </div>
        ) : null}
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
                  </header>

                  {dayData.stops.length > 0 ? (
                    <div className="day-stops-flow">
                      {dayData.stops.map((stop, stopIdx) => {
                        const attraction = stop.attractionRef;
                        const isMust = attraction ? mustVisitSet.has(attractionKey(attraction)) : false;

                        return (
                          <div key={`${stop.name}-${stopIdx}`} className="timeline-stop-node">
                            {/* 景点卡片主体 */}
                            <div className="stop-card">
                              <div className="stop-badge-column">
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
                                      <label>
                                        <span>安排在</span>
                                        <select
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
                                        title="从行程中移出"
                                        onClick={() => {
                                          const key = attractionKey(attraction);
                                          setPlan((prev) => ({
                                            ...prev,
                                            selectedKeys: prev.selectedKeys.filter((k) => k !== key),
                                          }));
                                        }}
                                      >
                                        <X size={13} />
                                      </button>
                                    </div>
                                  )}
                                </div>

                                {stop.reason && (
                                  <p className="stop-reason">{stop.reason}</p>
                                )}

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
                      <p>这一天暂无安排景点，可以从「全城景点图鉴」中添加喜欢的地标。</p>
                    </div>
                  )}
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
            apiKey={amapApiKey}
            securityJsCode={amapSecurityJsCode}
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
              <p>没有找到符合条件的景点，可以尝试搜索其他关键词。</p>
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
              <h3>{city.name} 风味寻味地图</h3>
              <p>
                {guide.food_summary ||
                  "先认准本地最具代表性的经典吃法，再结合当天行程路线，就近打卡口碑餐厅与街头小馆。"}
              </p>
            </div>
          </div>

          {/* 代表性美食特产 */}
          {guide.foods.length > 0 && (
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
                        {place.why_pick || place.note || "适合作为当天游览路线上的就餐补给点。"}
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
                    <div className="stay-features">
                      <span>✓ 交通便利</span>
                      <span>✓ 餐饮集中</span>
                      <span>✓ 靠近核心游览区</span>
                    </div>
                  </div>
                ))
              ) : (
                <div className="stay-area-card">
                  <div className="stay-card-header">
                    <h4>市区中心 / 地铁枢纽商圈</h4>
                    <span className="budget-tag">中端 / 舒适</span>
                  </div>
                  <p className="stay-note">
                    建议选择靠近城市核心地铁交汇站（如 1、2 号线换乘枢纽），往返景点与火车站最省时。
                  </p>
                </div>
              )}
            </div>
          </Section>

          {/* 全城交通指南 */}
          <Section title="抵达与全城交通" eyebrow="GETTING AROUND" icon={<Airplane size={16} />}>
            <div className="transport-guide-grid">
              <div className="transport-card">
                <div className="transport-head">
                  <Airplane size={18} />
                  <h4>机场与大交通</h4>
                </div>
                <p>
                  {guide.transport.airport ||
                    "各大枢纽机场均配备机场快轨或大巴专线直达市中心核心区域。"}
                </p>
              </div>

              <div className="transport-card">
                <div className="transport-head">
                  <Train size={18} />
                  <h4>火车站 / 高铁抵达</h4>
                </div>
                <p>
                  {guide.transport.train_station ||
                    "高铁枢纽通常无缝接驳城市地铁主力线路，出站刷码乘车最便捷。"}
                </p>
              </div>

              <div className="transport-card">
                <div className="transport-head">
                  <Car size={18} />
                  <h4>市内地铁与公交打车</h4>
                </div>
                <p>
                  {guide.transport.metro ||
                    guide.transport.bus_taxi ||
                    "推荐使用支付宝/微信乘车码乘坐地铁与公交；主要景点间打车起步价经济实惠。"}
                </p>
              </div>
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
                    <h4>暂无重大突发警示</h4>
                  </div>
                  <p>主要景点运营平稳，建议出发前通过官方微信小程序确认实时开放与预约排队情况。</p>
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
          {guide.sources.length > 0 && (
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
  apiKey,
  securityJsCode,
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
  apiKey?: string | null;
  securityJsCode?: string | null;
}) {
  const [selectedDay, setSelectedDay] = useState<number | null>(days[0]?.day ?? null);
  const [activeStopIndex, setActiveStopIndex] = useState<number | null>(null);
  const [mapError, setMapError] = useState<string | null>(null);
  const mapContainer = useRef<HTMLDivElement>(null);

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

  const hasMapConfig = Boolean(apiKey?.trim() && securityJsCode?.trim());
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

  useEffect(() => {
    const container = mapContainer.current;
    if (!container || !hasMapConfig || points.length === 0) return;
    let cancelled = false;
    let map: AmapMapInstance | null = null;
    setMapError(null);

    void loadAmap(apiKey!, securityJsCode!)
      .then((amap: AmapNamespace) => {
        if (cancelled) return;
        const first = points[0].coordinates!;
        map = new amap.Map(container, {
          center: [first.longitude, first.latitude],
          zoom: 12,
          viewMode: "2D",
          resizeEnable: true,
          mapStyle: "amap://styles/normal",
        });

        const markers: AmapOverlay[] = points.map((point, index) => {
          const marker = new amap.Marker({
            position: [point.coordinates!.longitude, point.coordinates!.latitude],
            title: point.name,
            content: `<div class="travel-amap-marker ${activeStopIndex === index ? "active" : ""}">${index + 1}</div>`,
            offset: [-14, -14],
          });
          return marker;
        });

        map.add(markers);
        map.setFitView(markers, false, [50, 50, 50, 50], 14);
      })
      .catch((error: unknown) => {
        if (!cancelled)
          setMapError(error instanceof Error ? error.message : "高德地图加载失败");
      });

    return () => {
      cancelled = true;
      map?.destroy();
    };
  }, [activeStopIndex, apiKey, hasMapConfig, points, securityJsCode]);

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
            {hasMapConfig ? (
              <div
                ref={mapContainer}
                className="travel-map-canvas"
                role="img"
                aria-label="行程 POI 高德地图"
              />
            ) : (
              <div className="travel-map-unconfigured-box">
                <Compass size={32} />
                <h4>配置高德地图 API 后可显示真实矢量底图与缩放</h4>
                <p>可在「设置 → Geography」中填入高德 Web Key 与安全密钥。</p>
              </div>
            )}
            {mapError && (
              <div className="travel-map-status" role="alert">
                地图加载提示：{mapError}
              </div>
            )}
          </div>

          {/* 右侧节点距离列表 */}
          <div className="travel-map-nodes-sidebar">
            <h4>路线节点与距离测算</h4>
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
              <p className="no-nodes-hint">当前所选日期无节点点位。</p>
            )}

            {points.length > 0 && (
              <button
                type="button"
                className="travel-link-button"
                onClick={() => openLink(mapUrl)}
              >
                <ArrowSquareOut size={14} />
                在高德地图中打开全景路线
              </button>
            )}
          </div>
        </div>
      </div>
    </Section>
  );
}
