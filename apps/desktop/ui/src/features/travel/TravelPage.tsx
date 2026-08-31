import { invoke } from "@tauri-apps/api/core";
import { ArrowsClockwise, Compass, MapPin, Sparkle } from "@phosphor-icons/react";
import { useCallback, useEffect, useRef, useState } from "react";
import type { CityGuide, GuideSummary, TravelResearchEvent, TravelResearchSnapshot } from "../../types";
import { errorMessage, isTauriRuntime } from "../../utils";
import { TravelGuide } from "./TravelGuide";
import { TravelProgress } from "./TravelProgress";

/**
 * Travel 模块（需求 #十四）：研究一个城市。
 * 交互：输入城市/天数/偏好 → 后台研究（会话 + 进度轮询）→ 结构化攻略 + Sources。
 * 数据全部来自 Rust 命令层；未配置 LLM / API Key 时模块仍可用（降级为来源列表）。
 */

export const TRAVEL_PREFERENCES = ["历史", "美食", "自然", "散步", "摄影", "亲子", "文化", "购物", "夜生活"];

interface TravelPageProps {
  active: boolean;
  setNotice: (message: string) => void;
}

type ResearchState = "idle" | "running" | "done" | "error";

export function TravelPage({ active, setNotice }: TravelPageProps) {
  const [city, setCity] = useState("");
  const [days, setDays] = useState(3);
  const [preferences, setPreferences] = useState<string[]>([]);
  const [state, setState] = useState<ResearchState>("idle");
  const [events, setEvents] = useState<TravelResearchEvent[]>([]);
  const [guide, setGuide] = useState<CityGuide | null>(null);
  const [error, setError] = useState("");
  const [history, setHistory] = useState<GuideSummary[]>([]);
  const [fromCache, setFromCache] = useState(false);
  const pollTimer = useRef<number | null>(null);

  const reloadHistory = useCallback(async () => {
    if (!isTauriRuntime()) return;
    try {
      const list = await invoke<GuideSummary[]>("travel_recent_guides");
      setHistory(list);
    } catch {
      // 历史加载失败保持安静
    }
  }, []);

  useEffect(() => { void reloadHistory(); }, [reloadHistory]);

  // 离开页面时停止轮询
  useEffect(() => () => { if (pollTimer.current !== null) window.clearInterval(pollTimer.current); }, []);

  const stopPolling = useCallback(() => {
    if (pollTimer.current !== null) { window.clearInterval(pollTimer.current); pollTimer.current = null; }
  }, []);

  const togglePreference = (preference: string) => {
    setPreferences((previous) =>
      previous.includes(preference)
        ? previous.filter((item) => item !== preference)
        : [...previous, preference]);
  };

  const startResearch = async (force: boolean) => {
    const cityName = city.trim();
    if (!cityName) { setNotice("请输入要探索的城市，例如「杭州」"); return; }
    stopPolling();
    setState("running");
    setGuide(null);
    setEvents([]);
    setFromCache(false);
    setError("");
    try {
      const request = { city: cityName, days, month: null, preferences, force };
      const id = await invoke<string>("travel_research_start", { request });
      // 轮询进度直至完成
      const poll = async () => {
        try {
          const snapshot = await invoke<TravelResearchSnapshot | null>("travel_research_progress", { sessionId: id });
          if (!snapshot) { // 会话不存在（异常）→ 结束
            stopPolling(); setState("error"); setError("研究会话丢失，请重试。");
            return;
          }
          setEvents(snapshot.events);
          setFromCache(snapshot.from_cache);
          if (snapshot.done) {
            stopPolling();
            if (snapshot.guide) { setGuide(snapshot.guide); setState("done"); }
            else { setState("error"); setError(snapshot.error ?? "研究失败，请查看设置后重试。"); }
            void reloadHistory();
          }
        } catch (pollError) {
          stopPolling(); setState("error"); setError(errorMessage(pollError));
        }
      };
      pollTimer.current = window.setInterval(() => void poll(), 600);
      await poll();
    } catch (researchError) {
      stopPolling(); setState("error"); setError(errorMessage(researchError));
    }
  };

  const openHistory = async (summary: GuideSummary) => {
    if (!isTauriRuntime()) return;
    try {
      const loaded = await invoke<CityGuide | null>("travel_load_guide", { city: summary.city, days: summary.days });
      if (loaded) { setCity(summary.city); setDays(summary.days); setGuide(loaded); setEvents([]); setState("done"); setFromCache(false); }
      else setNotice("本地没有找到该攻略，可能已被清除。");
    } catch (openError) { setNotice(errorMessage(openError)); }
  };

  return <div className="page-scroll travel-page">
    <header className="travel-hero">
      <h1>Travel</h1>
      <p>探索一个城市 —— 自动搜索国内互联网、抓取权威来源、交叉验证后整理成结构化攻略。</p>
    </header>

    <section className="travel-search">
      <label htmlFor="travel-city">去哪里？</label>
      <div className="travel-search-row">
        <input id="travel-city" value={city} onChange={(event) => setCity(event.target.value)}
          onKeyDown={(event) => { if (event.key === "Enter") void startResearch(false); }}
          placeholder="城市，如：杭州" disabled={state === "running"} />
        <button disabled={state === "running" || !city.trim()} onClick={() => void startResearch(false)}>
          <Compass size={16} />{state === "running" ? "研究中…" : "开始探索"}
        </button>
        {state === "done" && guide
          ? <button className="travel-research-again" title="跳过 24 小时缓存，重新搜索全部网页" onClick={() => void startResearch(true)}>
            <ArrowsClockwise size={15} />重新研究
          </button> : null}
      </div>
      <div className="travel-search-meta">
        <label htmlFor="travel-days">天数</label>
        <select id="travel-days" value={days} disabled={state === "running"} onChange={(event) => setDays(Number(event.target.value))}>
          {[1, 2, 3, 4, 5, 6, 7].map((value) => <option key={value} value={value}>{value} 天</option>)}
        </select>
        <span className="travel-prefs-label">偏好</span>
        <div className="travel-prefs">
          {TRAVEL_PREFERENCES.map((preference) =>
            <button key={preference} className={preferences.includes(preference) ? "selected" : ""}
              disabled={state === "running"} onClick={() => togglePreference(preference)}>{preference}</button>)}
        </div>
      </div>
    </section>

    {history.length > 0
      ? <section className="travel-history">
        <h2>最近攻略</h2>
        <div className="travel-history-list">
          {history.slice(0, 6).map((summary) =>
            <button key={`${summary.city}-${summary.days}`} onClick={() => void openHistory(summary)}>
              <MapPin size={14} /><span>{summary.city}</span><small>{summary.days} 天</small>
            </button>)}
        </div>
      </section> : null}

    {state === "running"
      ? <section className="travel-research"><TravelProgress events={events} /></section>
      : null}
    {state === "error"
      ? <section className="travel-error"><p>⚠ {error}</p><button onClick={() => setState("idle")}>返回</button></section>
      : null}
    {state === "done" && guide
      ? <>
        {fromCache ? <p className="travel-cache-note">已命中本地缓存（24 小时内生成），点击「重新研究」可获取最新信息。</p> : null}
        <TravelGuide guide={guide} fromCache={fromCache} />
      </>
      : null}
    {state === "idle" ? <p className="travel-hint"><Sparkle size={14} />输入城市后，系统会规划搜索任务 → 搜索 → 抓取网页 → 提取并验证事实 → 生成攻略。</p> : null}
  </div>;
}