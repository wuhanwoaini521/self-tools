import { invoke } from "@tauri-apps/api/core";
import { Compass, Gear, House, MapTrifold, Notebook, Rss, Scroll, Translate, Wrench, X } from "@phosphor-icons/react";
import { useCallback, useEffect, useRef, useState } from "react";
import { SettingsDialog } from "./SettingsDialog";
import { HomePage } from "./features/home/HomePage";
import { MarkdownPage, type MarkdownIntent } from "./features/markdown/MarkdownPage";
import { RssPage, type RssIntent } from "./features/rss/RssPage";
import { HistoryPage } from "./features/history/HistoryPage";
import { LanguagePage } from "./features/language/LanguagePage";
import { TravelPage } from "./features/travel/TravelPage";
import { GeographyPage } from "./features/geography/GeographyPage";
import { applyTheme, getTheme, storeThemeId } from "./theme/ThemeManager";
import "./theme/themes";
import type { AppSettings, ArticleDto, FeedDto, GeographyHome, HistoryHome, ReviewCard, TodayView } from "./types";
import { errorMessage, isTauriRuntime } from "./utils";

/**
 * Personal Dashboard 外壳：
 * - 顶栏(品牌 + 全局设置) + 左侧功能导航 + 右侧当前 Feature 页面;
 * - 每个 Feature(Home / Markdown / RSS / Travel / …)保持挂载,切换仅显隐,状态自然保留;
 * - 新增模块 = 新增一个 feature 目录 + 注册一个导航项,外壳不需要感知模块内部。
 */

type PageId = "home" | "markdown" | "rss" | "travel" | "geography" | "history" | "language" | "tools";

interface NavItem {
  id: PageId;
  label: string;
  icon: typeof House;
  disabled?: boolean;
}

/** 导航注册表:新功能在这里加一行即可(Tools 为未来模块的占位) */
const NAV_ITEMS: NavItem[] = [
  { id: "home", label: "Home", icon: House },
  { id: "markdown", label: "Markdown", icon: Notebook },
  { id: "rss", label: "RSS", icon: Rss },
  { id: "travel", label: "Travel", icon: Compass },
  { id: "geography", label: "Geography", icon: MapTrifold },
  { id: "history", label: "History", icon: Scroll },
  { id: "language", label: "Language", icon: Translate },
  { id: "tools", label: "Tools", icon: Wrench, disabled: true },
];

const defaultSettings: AppSettings = {
  schema_version: 1, recent_files: [], workspace_path: null, theme_mode: "dark", ui_theme: "default",
  rss_refresh_minutes: 30, editor_font_size: 14, auto_save: false, markdown_default_view: "split",
  travel: {
    search_backend: "auto", searxng_url: null, llm_base_url: null, llm_api_key: null, llm_model: null,
    amap_api_key: null, qweather_api_key: null, qweather_api_host: null, baidu_map_api_key: null,
  },
  geography: { amap_api_key: null, amap_security_js_code: null },
};

export default function App() {
  const [page, setPage] = useState<PageId>("home");
  const [settings, setSettings] = useState<AppSettings>(defaultSettings);
  const [settingsLoaded, setSettingsLoaded] = useState(!isTauriRuntime());
  const [themeId, setThemeId] = useState("default");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [notice, setNotice] = useState("");

  const [rssRefreshing, setRssRefreshing] = useState(false);
  const [rssVersion, setRssVersion] = useState(0);
  const [unreadTotal, setUnreadTotal] = useState(0);
  const [latestArticles, setLatestArticles] = useState<ArticleDto[]>([]);
  const [geographyHome, setGeographyHome] = useState<GeographyHome | null>(null);
  const [historyHome, setHistoryHome] = useState<HistoryHome | null>(null);
  const [todayView, setTodayView] = useState<TodayView | null>(null);
  const [reviewCard, setReviewCard] = useState<ReviewCard | null>(null);
  const [markdownIntent, setMarkdownIntent] = useState<MarkdownIntent | null>(null);
  const [rssIntent, setRssIntent] = useState<RssIntent | null>(null);
  const [geographyIntent, setGeographyIntent] = useState<{ entityId: string; nonce: number } | null>(null);
  const [historyIntent, setHistoryIntent] = useState<{ id: string; nonce: number } | null>(null);
  const [languageIntent, setLanguageIntent] = useState<{ id: string; nonce: number } | null>(null);
  const startupRefreshed = useRef(false);

  /** 主题即时切换:CSS 变量作用于 :root,所有页面同步更新 */
  useEffect(() => { applyTheme(themeId); }, [themeId]);

  /** 全局设置更新:写入 settings.json(浏览器模式仅内存)。 */
  const updateSettings = useCallback(async (next: AppSettings) => {
    setSettings(next);
    if (!isTauriRuntime()) return;
    try { await invoke("put_settings", { settings: next }); } catch (error) { setNotice(errorMessage(error)); }
  }, []);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    void invoke<AppSettings>("get_settings")
      .then((loaded) => { setSettings(loaded); setThemeId(getTheme(loaded.ui_theme).id); setSettingsLoaded(true); })
      .catch((error) => { setSettingsLoaded(true); setNotice(errorMessage(error)); });
  }, []);

  const reloadLatest = useCallback(async () => {
    if (!isTauriRuntime()) return;
    try { setLatestArticles(await invoke<ArticleDto[]>("latest_rss_articles", { limit: 6 })); } catch { /* 首页数据加载失败保持安静 */ }
  }, []);

  const reloadHomeKnowledge = useCallback(async () => {
    if (!isTauriRuntime()) return;
    const [geography, history, today, review] = await Promise.allSettled([
      invoke<GeographyHome>("geography_home", { cursor: 0 }),
      invoke<HistoryHome>("history_home", { cursor: 0 }),
      invoke<TodayView>("language_today", { language: "jpn" }),
      invoke<ReviewCard | null>("language_review_next", { language: "jpn" }),
    ]);
    if (geography.status === "fulfilled") setGeographyHome(geography.value);
    if (history.status === "fulfilled") setHistoryHome(history.value);
    if (today.status === "fulfilled") setTodayView(today.value);
    if (review.status === "fulfilled") setReviewCard(review.value);
  }, []);

  useEffect(() => { void reloadHomeKnowledge(); }, [reloadHomeKnowledge]);

  /** 刷新全部订阅(手动按钮 / 定时任务 / 启动时各一次)。 */
  const refreshFeeds = useCallback(async (silent = false) => {
    if (!isTauriRuntime()) return;
    setRssRefreshing(true);
    try {
      const report = await invoke<import("./types").RefreshReport>("refresh_rss_feeds");
      setRssVersion((value) => value + 1);
      void reloadLatest();
      if (!silent) {
        setNotice(report.failures.length > 0
          ? `刷新完成,${report.failures.length} 个源失败:${report.failures.map((failure) => failure.feed_title).join("、")}`
          : report.new_articles > 0 ? `刷新完成,${report.new_articles} 篇新文章` : "订阅已是最新");
      }
    } catch (error) {
      if (!silent) setNotice(errorMessage(error));
    } finally { setRssRefreshing(false); }
  }, [reloadLatest]);

  // 启动时静默刷新一次;之后按设置的间隔定时刷新(间隔可在设置中调整)。
  useEffect(() => {
    if (!isTauriRuntime() || startupRefreshed.current) return;
    startupRefreshed.current = true;
    void refreshFeeds(true);
  }, [refreshFeeds]);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    const minutes = Math.max(1, settings.rss_refresh_minutes || 30);
    const timer = window.setInterval(() => void refreshFeeds(true), minutes * 60_000);
    return () => window.clearInterval(timer);
  }, [settings.rss_refresh_minutes, refreshFeeds]);

  const handleFeedsChanged = useCallback((feeds: FeedDto[]) => {
    setUnreadTotal(feeds.reduce((total, feed) => total + feed.unread_count, 0));
  }, []);

  const changeTheme = useCallback((nextId: string) => {
    const normalized = getTheme(nextId).id;
    setThemeId(normalized);
    storeThemeId(normalized);
    void updateSettings({ ...settings, ui_theme: normalized });
  }, [settings, updateSettings]);

  const changeRefreshMinutes = useCallback((minutes: number) => {
    void updateSettings({ ...settings, rss_refresh_minutes: minutes });
  }, [settings, updateSettings]);

  const openNote = useCallback((filePath: string) => { setPage("markdown"); setMarkdownIntent({ type: "open", path: filePath, nonce: Date.now() }); }, []);
  const newNote = useCallback(() => { setPage("markdown"); setMarkdownIntent({ type: "new", nonce: Date.now() }); }, []);
  const openArticle = useCallback((article: ArticleDto) => { setPage("rss"); setRssIntent({ article, nonce: Date.now() }); }, []);
  const openGeography = useCallback((id?: string) => { setPage("geography"); if (id) setGeographyIntent({ entityId: id, nonce: Date.now() }); }, []);
  const openHistory = useCallback((id?: string) => { setPage("history"); if (id) setHistoryIntent({ id, nonce: Date.now() }); }, []);
  const openLanguage = useCallback((id?: string) => { setPage("language"); if (id) setLanguageIntent({ id, nonce: Date.now() }); }, []);

  return <div className="app-shell">
    <header className="app-bar">
      <div className="brand"><strong>DevToolbox</strong><span /><p>Personal Dashboard</p></div>
      <button className="app-bar-gear" title="Settings" onClick={() => setSettingsOpen(true)}><Gear size={19} /></button>
    </header>
    <div className="app-body">
      <nav className="app-nav" aria-label="功能导航">
        {NAV_ITEMS.map((item) => item.disabled
          ? <span className="app-nav-item disabled" key={item.id} title="即将推出"><item.icon size={18} />{item.label}</span>
          : <button key={item.id} className={"app-nav-item" + (page === item.id ? " active" : "")} onClick={() => setPage(item.id)}>
            <item.icon size={18} />{item.label}
            {item.id === "rss" && unreadTotal > 0 ? <b>{unreadTotal > 99 ? "99+" : unreadTotal}</b> : null}
          </button>)}
        <div className="app-nav-divider" />
        <button className={"app-nav-item" + (settingsOpen ? " active" : "")} onClick={() => setSettingsOpen(true)}><Gear size={18} />Settings</button>
        <footer className="app-nav-footer">Personal Workspace</footer>
      </nav>
      <main className="app-content">
        <section className={"page-pane" + (page === "home" ? "" : " page-hidden")}><HomePage recentFiles={settings.recent_files} latestArticles={latestArticles} geographyHome={geographyHome} historyHome={historyHome} todayView={todayView} reviewCard={reviewCard} rssRefreshing={rssRefreshing} onOpenNote={openNote} onOpenArticle={openArticle} onOpenGeography={openGeography} onOpenHistory={openHistory} onOpenLanguage={openLanguage} onNewNote={newNote} onRefreshRss={() => void refreshFeeds()} /></section>
        <section className={"page-pane" + (page === "markdown" ? "" : " page-hidden")}><MarkdownPage settings={settings} onSettingsChange={(next) => void updateSettings(next)} setNotice={setNotice} active={page === "markdown"} intent={markdownIntent} initialWorkspace={settingsLoaded ? settings.workspace_path : undefined} /></section>
        <section className={"page-pane" + (page === "rss" ? "" : " page-hidden")}><RssPage active={page === "rss"} version={rssVersion} refreshing={rssRefreshing} onRefresh={() => void refreshFeeds()} onFeedsChanged={handleFeedsChanged} setNotice={setNotice} intent={rssIntent} /></section>
        <section className={"page-pane" + (page === "travel" ? "" : " page-hidden")}><TravelPage active={page === "travel"} setNotice={setNotice} amapApiKey={settings.travel.amap_api_key} /></section>
        <section className={"page-pane geography-pane" + (page === "geography" ? "" : " page-hidden")}><GeographyPage active={page === "geography"} setNotice={setNotice} amapApiKey={settings.geography.amap_api_key} amapSecurityJsCode={settings.geography.amap_security_js_code} intent={geographyIntent} /></section>
        <section className={"page-pane history-pane" + (page === "history" ? "" : " page-hidden")}><HistoryPage active={page === "history"} setNotice={setNotice} intent={historyIntent} /></section>
        <section className={"page-pane language-pane" + (page === "language" ? "" : " page-hidden")}><LanguagePage active={page === "language"} setNotice={setNotice} intent={languageIntent} /></section>
      </main>
    </div>
    {notice ? <button className="toast" onClick={() => setNotice("")}>{notice}<X size={16} /></button> : null}
    {settingsOpen ? <SettingsDialog themeId={themeId} onThemeChange={changeTheme} rssRefreshMinutes={settings.rss_refresh_minutes} onRefreshMinutesChange={changeRefreshMinutes} travel={settings.travel} onTravelChange={(next) => void updateSettings({ ...settings, travel: next })} geography={settings.geography} onGeographyChange={(next) => void updateSettings({ ...settings, geography: next })} onClose={() => setSettingsOpen(false)} /> : null}
  </div>;
}
