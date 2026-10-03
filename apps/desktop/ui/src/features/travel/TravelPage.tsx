import {
  ArrowsClockwise,
  CalendarBlank,
  CarProfile,
  CloudSun,
  Compass,
  ForkKnife,
  MapPin,
  Sparkle,
  Star,
  Tag,
  X,
} from "@phosphor-icons/react";
import { useCallback, useEffect, useRef, useState } from "react";
import type { AppContextPayload } from "../ai/aiTypes";
import type {
  CityGuide,
  GuideSummary,
  TravelDateRange,
  TravelResearchEvent,
} from "../../types";
import { errorMessage, isTauriRuntime } from "../../utils";
import { TravelGuide } from "./TravelGuide";
import { TravelProgress } from "./TravelProgress";
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

interface TravelPageProps {
  active: boolean;
  setNotice: (message: string) => void;
  amapApiKey?: string | null;
  amapSecurityJsCode?: string | null;
}

type ResearchState = "idle" | "running" | "done" | "error";

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
  amapApiKey,
  amapSecurityJsCode,
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
  const [fromCache, setFromCache] = useState(false);
  const pollTimer = useRef<number | null>(null);

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
        must_visits: mustVisits,
        preferences,
        from_cache: fromCache,
      },
    });
  }, [city, state, guide, days, mustVisits, preferences, fromCache, onContextChange]);

  const reloadHistory = useCallback(async () => {
    if (!isTauriRuntime()) return;
    try {
      const list = await travelClient.recentGuides();
      setHistory(list);
    } catch {
      // 历史加载失败保持安静
    }
  }, []);

  useEffect(() => {
    void reloadHistory();
  }, [reloadHistory]);

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

  const openHistory = async (summary: GuideSummary) => {
    if (!isTauriRuntime()) return;
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
    <div className="page-scroll travel-page">
      <header className="travel-hero">
        <div className="travel-hero-badge">
          <Sparkle size={14} weight="fill" />
          <span>沉浸式城市旅行探索与行程规划</span>
        </div>
        <h1>探索下一座城市</h1>
        <p>
          输入目的地、天数与必去景点，自动检索权威信源、整合实时天气、测算景点距离与交通耗时，生成专属深度游路线。
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
                className="travel-submit-btn"
                disabled={state === "running" || !city.trim()}
                onClick={() => void startResearch(false)}
              >
                <Compass size={17} weight="bold" />
                {state === "running" ? "智能研究中…" : "开始探索城市"}
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
            userMustVisits={mustVisits}
            amapApiKey={amapApiKey}
            amapSecurityJsCode={amapSecurityJsCode}
          />
        </>
      ) : null}

      {state === "idle" ? (
        <div className="travel-feature-intro">
          <div className="travel-feature-card">
            <CloudSun className="feature-icon" size={18} />
            <h3>真实天气与穿衣建议</h3>
            <p>获取目的地未来逐日天气、温差与降水预警，提供体贴的穿衣携带指南。</p>
          </div>
          <div className="travel-feature-card">
            <MapPin className="feature-icon" size={18} />
            <h3>景点硬核信息与避坑</h3>
            <p>多源交叉核验门票、开放时间与预约通道，标注多版本冲突事实。</p>
          </div>
          <div className="travel-feature-card">
            <CarProfile className="feature-icon" size={18} />
            <h3>距离测算与交通耗时</h3>
            <p>自动计算各景点间公里数、驾车打车与地铁耗时，合理编排游览节奏。</p>
          </div>
          <div className="travel-feature-card">
            <ForkKnife className="feature-icon" size={18} />
            <h3>地道风味与路线周边</h3>
            <p>精选城市代表性美食，并匹配每日路线上最近的特色餐厅。</p>
          </div>
        </div>
      ) : null}
    </div>
  );
}
