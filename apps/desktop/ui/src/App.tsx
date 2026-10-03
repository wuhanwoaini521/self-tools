import {
  Brain,
  Cards,
  Compass,
  FolderSimple,
  Gear,
  HardDrives,
  Heartbeat,
  House,
  MagnifyingGlass,
  MapTrifold,
  Newspaper,
  Notebook,
  Rss,
  Scroll,
  Sparkle,
  Translate,
  TreeStructure,
  X,
} from "@phosphor-icons/react";
import { openPath } from "@tauri-apps/plugin-opener";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { SettingsDialog } from "./SettingsDialog";
import { HomePage } from "./features/home/HomePage";
import {
  MarkdownPage,
  type MarkdownIntent,
} from "./features/markdown/MarkdownPage";
import { RssPage, type RssIntent } from "./features/rss/RssPage";
import { rssClient } from "./features/rss/rssClient";
import { NewsPage } from "./features/news/NewsPage";
import { learningClient } from "./features/learning/learningClient";
import { settingsClient } from "./settingsClient";
import { HistoryPage } from "./features/history/HistoryPage";
import { historyClient } from "./features/history/historyClient";
import { geographyClient } from "./features/geography/geographyClient";
import { LanguagePage } from "./features/language/LanguagePage";
import { AIPanel } from "./features/ai/AIPanel";
import type { AgentAction, AppContextPayload } from "./features/ai/aiTypes";
import {
  KnowledgePage,
  type KnowledgeIntent,
} from "./features/knowledge/KnowledgePage";
import { knowledgeClient } from "./features/knowledge/knowledgeClient";
import { serverClient } from "./features/server/serverClient";
import type {
  ConfirmMemoryTarget,
  OpenDocumentTarget,
  OpenFileTarget,
} from "./features/knowledge/knowledgeTypes";
import { ServerPage } from "./features/server/ServerPage";
import { StudyBoardPage } from "./features/study/StudyBoardPage";
import { SystemReadinessPage } from "./features/system/SystemReadinessPage";
import { GlobalSearchPage } from "./features/system/GlobalSearchPage";
import { TravelPage } from "./features/travel/TravelPage";
import { GeographyPage } from "./features/geography/GeographyPage";
import { ReviewCenterPage } from "./features/learning/ReviewCenterPage";
import { KnowledgeGraphPage } from "./features/learning/KnowledgeGraphPage";
import { CollectionsPage } from "./features/learning/CollectionsPage";
import {
  applyTheme,
  getTheme,
  initialThemeId,
  storeThemeId,
} from "./theme/ThemeManager";
import "./theme/themes";
import {
  useDeviceAttribute,
  useLayout,
  navigationLayout,
} from "./layout";
import { shortcutDigitForPage, useGlobalShortcuts } from "./useGlobalShortcuts";
import { PwaBanner } from "./PwaBanner";
import type {
  AppSettings,
  ArticleDto,
  FeedDto,
  GeographyHome,
  SemanticHistoryHome,
  TodayDashboardData,
} from "./types";
import { errorMessage, isTauriRuntime } from "./utils";

/**
 * Personal Dashboard 外壳：
 * - 顶栏(品牌 + 全局设置) + 左侧功能导航 + 右侧当前 Feature 页面;
 * - 每个 Feature(Home / Markdown / RSS / Travel / …)保持挂载,切换仅显隐,状态自然保留;
 * - 新增模块 = 新增一个 feature 目录 + 注册一个导航项,外壳不需要感知模块内部。
 */

type PageId =
  | "home"
  | "review"
  | "graph"
  | "collections"
  | "markdown"
  | "rss"
  | "news"
  | "travel"
  | "geography"
  | "history"
  | "language"
  | "knowledge"
  | "study-board"
  | "server"
  | "system"
  | "search";

interface NavItem {
  id: PageId;
  label: string;
  icon: typeof House;
}

/** 页面 id 集合（hash 路由与导航共用）。 */
const PAGE_IDS: PageId[] = [
  "home",
  "review",
  "graph",
  "collections",
  "markdown",
  "rss",
  "news",
  "travel",
  "geography",
  "history",
  "language",
  "knowledge",
  "study-board",
  "server",
  "system",
  "search",
];

/**
 * 侧边导航分组。
 *
 * 分组只影响呈现，不影响路由：每个条目仍然是独立 hash route，`PAGE_IDS`
 * 与历史深链接全部保持不变。目的是让 16 个入口不再平铺成一层，让用户一眼
 * 看出「学习 / 发现 / 创作 / 知识 / 系统」这几条主线。
 */
const NAV_GROUPS: Array<{ label: string; items: NavItem[] }> = [
  {
    label: "概览",
    items: [
      { id: "home", label: "Home", icon: House },
      { id: "review", label: "Review", icon: Cards },
    ],
  },
  {
    label: "学习",
    items: [
      { id: "history", label: "History", icon: Scroll },
      { id: "geography", label: "Geography", icon: MapTrifold },
      { id: "language", label: "Language", icon: Translate },
      { id: "study-board", label: "Study", icon: Notebook },
    ],
  },
  {
    label: "发现",
    items: [
      { id: "news", label: "News", icon: Newspaper },
      { id: "rss", label: "RSS", icon: Rss },
      { id: "travel", label: "Travel", icon: Compass },
    ],
  },
  {
    label: "创作",
    items: [
      { id: "markdown", label: "Markdown", icon: Notebook },
      { id: "collections", label: "Collections", icon: FolderSimple },
    ],
  },
  {
    label: "知识",
    items: [
      { id: "knowledge", label: "Knowledge", icon: Brain },
      { id: "graph", label: "Graph", icon: TreeStructure },
      { id: "search", label: "Search", icon: MagnifyingGlass },
    ],
  },
  {
    label: "系统",
    items: [
      { id: "server", label: "Server", icon: HardDrives },
      { id: "system", label: "System", icon: Heartbeat },
    ],
  },
];

const defaultSettings: AppSettings = {
  schema_version: 1,
  recent_files: [],
  workspace_path: null,
  theme_mode: "light",
  ui_theme: "default",
  rss_refresh_minutes: 30,
  editor_font_size: 14,
  auto_save: false,
  markdown_default_view: "split",
  travel: {
    search_backend: "auto",
    searxng_url: null,
    llm_base_url: null,
    llm_api_key: null,
    llm_model: null,
    amap_api_key: null,
    qweather_api_key: null,
    qweather_api_host: null,
    baidu_map_api_key: null,
  },
  geography: { amap_api_key: null, amap_security_js_code: null },
  ai: {
    provider: null,
    model: null,
    base_url: null,
    api_key: null,
    timeout_secs: null,
  },
  knowledge: {
    file_roots: [],
    document_roots: [],
    max_document_bytes: 1_000_000,
    max_read_chars: 20_000,
    max_indexed_files: 5_000,
    startup_sync: false,
  },
  server: {
    mcp: {
      enabled: true,
      stdio_enabled: true,
      http_enabled: false,
      bind: "127.0.0.1",
      remote_enabled: false,
      port: 8787,
    },
    services: [],
    applications: [],
    thresholds: {
      disk_warn_ratio: 0.8,
      disk_critical_ratio: 0.92,
      memory_warn_ratio: 0.85,
      cpu_warn_ratio: 0.9,
    },
    confirmation_ttl_secs: 60,
    cooldown_secs: 60,
    max_system_per_session: 5,
    audit_max_entries: 500,
    audit_retention_days: 30,
  },
};

export default function App() {
  const layout = useLayout();
  useDeviceAttribute();
  const [page, setPage] = useState<PageId>("home");

  // hash 路由（V11：`#study-board` 等直链必须能打开对应页面）。
  //
  // 顺序很重要：**先读后写**。若分开两个 effect，写 hash 的 effect 会在
  // setPage 生效前用初始值 home 覆盖 URL，直链就永远丢了。
  useEffect(() => {
    const fromHash = (): PageId | null => {
      const raw = window.location.hash.replace(/^#/, "").trim();
      if (!raw) return null;
      return (PAGE_IDS as string[]).includes(raw) ? (raw as PageId) : null;
    };
    const initial = fromHash();
    if (initial) setPage(initial);
    const onHashChange = () => {
      const next = fromHash();
      if (next) setPage(next);
    };
    window.addEventListener("hashchange", onHashChange);
    return () => window.removeEventListener("hashchange", onHashChange);
  }, []);

  // 页面切换时回写 hash（刷新 / 分享链接保持当前页）。
  // 跳过首次执行（首次交给上面的 effect 读，避免互相覆盖）。
  const pageSyncedOnce = useRef(false);
  useEffect(() => {
    if (!pageSyncedOnce.current) {
      pageSyncedOnce.current = true;
      return;
    }
    const current = window.location.hash.replace(/^#/, "");
    if (current !== page) {
      window.history.replaceState(null, "", `#${page}`);
    }
  }, [page]);
  const [settings, setSettings] = useState<AppSettings>(defaultSettings);
  const [settingsLoaded, setSettingsLoaded] = useState(!isTauriRuntime());
  const [themeId, setThemeId] = useState(initialThemeId);
  const themeIdRef = useRef(themeId);
  const themeChangedByUserRef = useRef(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [notice, setNotice] = useState("");
  const [aiOpen, setAiOpen] = useState(false);
  /** AI 投递队列（V11：白板等外部入口把「问题 + 快照」一次送进面板）。 */
  const [conversationOpen, setConversationOpen] = useState(false);
  const [aiDelivery, setAiDelivery] = useState<{
    nonce: number;
    text: string;
    parts?: Array<
      | { type: "text"; text: string }
      | {
          type: "image";
          data: string;
          mime: string;
          source: string;
          caption?: string;
        }
    >;
  } | null>(null);

  const deliverToAi = useCallback(
    (
      text: string,
      context?: AppContextPayload | null,
      parts?: Array<
        | { type: "text"; text: string }
        | {
            type: "image";
            data: string;
            mime: string;
            source: string;
            caption?: string;
          }
      >,
    ) => {
      setAiOpen(true);
      // 上下文先行：面板打开时 PersonalAgent 已带着「我在读哪条新闻」。
      if (context) setAiContext(context);
      setAiDelivery({ nonce: Date.now(), text, parts });
    },
    [],
  );
  /** AI 的 App Context（Frontend 负责“我在哪”；History 页报告当前实体）。 */
  const [aiContext, setAiContext] = useState<AppContextPayload | null>(null);
  const [rssRefreshing, setRssRefreshing] = useState(false);
  const [rssVersion, setRssVersion] = useState(0);
  const [unreadTotal, setUnreadTotal] = useState(0);
  /** News 未读（ADR-010：与 RSS 徽标分开计数，两个 context 不串台）。 */
  const [newsUnreadTotal, setNewsUnreadTotal] = useState(0);
  const [latestArticles, setLatestArticles] = useState<ArticleDto[]>([]);
  const [geographyHome, setGeographyHome] = useState<GeographyHome | null>(
    null,
  );
  const [historyHome, setHistoryHome] = useState<SemanticHistoryHome | null>(
    null,
  );
  // Language 在首页只贡献「待复习条数」这一个数字，而那是**平台** Today 的职责
  // （`TodayDashboardData.pending_reviews_count` / `review_stats`）。
  // 此前这里调的是 Language 私有的 `today_plan` / `review_next`——第二套复习队列。
  const [platformToday, setPlatformToday] = useState<TodayDashboardData | null>(null);
  const [markdownIntent, setMarkdownIntent] = useState<MarkdownIntent | null>(
    null,
  );
  const [rssIntent, setRssIntent] = useState<RssIntent | null>(null);
  const [geographyIntent, setGeographyIntent] = useState<{
    entityId: string;
    nonce: number;
  } | null>(null);
  const [historyIntent, setHistoryIntent] = useState<{
    id: string;
    kind?: "event" | "person" | "story";
    nonce: number;
  } | null>(null);
  const [languageIntent, setLanguageIntent] = useState<{
    id: string;
    nonce: number;
  } | null>(null);
  const [knowledgeIntent, setKnowledgeIntent] = useState<KnowledgeIntent | null>(
    null,
  );
  const startupRefreshed = useRef(false);

  /** 主题即时切换:CSS 变量作用于 :root,所有页面同步更新 */
  useEffect(() => {
    applyTheme(themeId);
  }, [themeId]);

  /** 全局设置更新:写入 settings.json(浏览器模式仅内存)。 */
  const updateSettings = useCallback(async (next: AppSettings) => {
    setSettings(next);
    if (!isTauriRuntime()) return;
    try {
      await settingsClient.put(next);
    } catch (error) {
      setNotice(errorMessage(error));
    }
  }, []);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    void settingsClient
      .get()
      .then(async (loaded) => {
        const resolvedThemeId = themeChangedByUserRef.current
          ? themeIdRef.current
          : getTheme(loaded.ui_theme).id;
        const resolvedSettings = { ...loaded, ui_theme: resolvedThemeId };
        setSettings(resolvedSettings);
        themeIdRef.current = resolvedThemeId;
        setThemeId(resolvedThemeId);
        if (themeChangedByUserRef.current) {
          await settingsClient.put(resolvedSettings);
        }
        setSettingsLoaded(true);
      })
      .catch((error) => {
        setSettingsLoaded(true);
        setNotice(errorMessage(error));
      });
  }, []);

  const reloadLatest = useCallback(async () => {
    if (!isTauriRuntime()) return;
    try {
      setLatestArticles(await rssClient.latestArticles(6));
    } catch {
      /* 首页数据加载失败保持安静 */
    }
  }, []);

  const reloadHomeKnowledge = useCallback(async () => {
    if (!isTauriRuntime()) return;
    const [geography, history, today] = await Promise.allSettled([
      geographyClient.home(0),
      historyClient.home(),
      learningClient.getToday(),
    ]);
    if (geography.status === "fulfilled") setGeographyHome(geography.value);
    if (history.status === "fulfilled") setHistoryHome(history.value);
    if (today.status === "fulfilled") setPlatformToday(today.value);
  }, []);

  useEffect(() => {
    void reloadHomeKnowledge();
  }, [reloadHomeKnowledge]);

  /** 刷新全部订阅(手动按钮 / 定时任务 / 启动时各一次)。 */
  const refreshFeeds = useCallback(
    async (silent = false) => {
      if (!isTauriRuntime()) return;
      setRssRefreshing(true);
      try {
        const report = await rssClient.refreshFeeds();
        setRssVersion((value) => value + 1);
        void reloadLatest();
        if (!silent) {
          setNotice(
            report.failures.length > 0
              ? `刷新完成,${report.failures.length} 个源失败:${report.failures.map((failure) => failure.feed_title).join("、")}`
              : report.new_articles > 0
                ? `刷新完成,${report.new_articles} 篇新文章`
                : "订阅已是最新",
          );
        }
      } catch (error) {
        if (!silent) setNotice(errorMessage(error));
      } finally {
        setRssRefreshing(false);
      }
    },
    [reloadLatest],
  );

  // 启动时静默刷新一次;之后按设置的间隔定时刷新(间隔可在设置中调整)。
  useEffect(() => {
    if (!isTauriRuntime() || startupRefreshed.current) return;
    startupRefreshed.current = true;
    void refreshFeeds(true);
  }, [refreshFeeds]);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    const minutes = Math.max(1, settings.rss_refresh_minutes || 30);
    const timer = window.setInterval(
      () => void refreshFeeds(true),
      minutes * 60_000,
    );
    return () => window.clearInterval(timer);
  }, [settings.rss_refresh_minutes, refreshFeeds]);

  const handleFeedsChanged = useCallback((feeds: FeedDto[]) => {
    setUnreadTotal(feeds.reduce((total, feed) => total + feed.unread_count, 0));
  }, []);

  const changeTheme = useCallback(
    (nextId: string) => {
      const normalized = getTheme(nextId).id;
      themeChangedByUserRef.current = true;
      themeIdRef.current = normalized;
      applyTheme(normalized);
      setThemeId(normalized);
      storeThemeId(normalized);
      if (settingsLoaded) {
        void updateSettings({ ...settings, ui_theme: normalized });
      }
    },
    [settings, settingsLoaded, updateSettings],
  );

  const changeRefreshMinutes = useCallback(
    (minutes: number) => {
      void updateSettings({ ...settings, rss_refresh_minutes: minutes });
    },
    [settings, updateSettings],
  );

  const openNote = useCallback((filePath: string) => {
    setPage("markdown");
    setMarkdownIntent({ type: "open", path: filePath, nonce: Date.now() });
  }, []);
  const newNote = useCallback(() => {
    setPage("markdown");
    setMarkdownIntent({ type: "new", nonce: Date.now() });
  }, []);
  const openArticle = useCallback((article: ArticleDto) => {
    setPage("rss");
    setRssIntent({ article, nonce: Date.now() });
  }, []);
  const openGeography = useCallback((id?: string) => {
    setPage("geography");
    if (id) setGeographyIntent({ entityId: id, nonce: Date.now() });
  }, []);
  const openHistory = useCallback(
    (id?: string, kind?: "event" | "person" | "story") => {
      setPage("history");
      if (id)
        setHistoryIntent({ id, kind: kind ?? "event", nonce: Date.now() });
    },
    [],
  );
  const openLanguage = useCallback((id?: string) => {
    setPage("language");
    if (id) setLanguageIntent({ id, nonce: Date.now() });
  }, []);

  const openKnowledge = useCallback((tab?: "memory" | "documents" | "files", docId?: string) => {
    setPage("knowledge");
    setKnowledgeIntent({
      tab: tab ?? "memory",
      documentId: docId ?? null,
      nonce: Date.now(),
    });
  }, []);

  const navigateToHash = useCallback(
    (hash: string) => {
      const raw = hash.startsWith("#") ? hash.slice(1) : hash;
      const [pathPart, queryPart] = raw.split("?");
      const params = new URLSearchParams(queryPart ?? "");

      if (pathPart === "history") {
        const storyId = params.get("story");
        const personId = params.get("person");
        const eventId = params.get("event") || params.get("id");
        const type = params.get("type");
        if (storyId) openHistory(storyId, "story");
        else if (personId) openHistory(personId, "person");
        else if (type === "story" && eventId) openHistory(eventId, "story");
        else if (type === "person" && eventId) openHistory(eventId, "person");
        else if (eventId) {
          if (eventId.startsWith("story-")) {
            openHistory(eventId, "story");
          } else {
            openHistory(eventId, "event");
          }
        } else {
          setPage("history");
        }
      } else if (pathPart === "geography") {
        const id = params.get("id") || params.get("entityId");
        openGeography(id ?? undefined);
      } else if (pathPart === "language") {
        const id = params.get("id") || params.get("word");
        openLanguage(id ?? undefined);
      } else if (pathPart === "news") setPage("news");
      else if (pathPart === "study-board" || pathPart === "study") setPage("study-board");
      else if (pathPart === "knowledge") {
        const tab = params.get("tab") as "memory" | "documents" | "files" | null;
        const docId = params.get("doc") || params.get("documentId");
        if (tab || docId) {
          openKnowledge(tab ?? undefined, docId ?? undefined);
        } else {
          setPage("knowledge");
        }
      } else if (pathPart === "server") setPage("server");
      else if (pathPart === "markdown") {
        const path = params.get("path");
        const isNew = params.get("new");
        if (path) openNote(path);
        else if (isNew) newNote();
        else setPage("markdown");
      }
      else if (pathPart === "travel") setPage("travel");
      else if (pathPart === "search") setPage("search");
      else if (PAGE_IDS.includes(pathPart as PageId)) setPage(pathPart as PageId);
    },
    [openGeography, openHistory, openLanguage, openKnowledge, openNote, newNote],
  );

  /** 全局快捷键与命令流监听 (⌘K / ⌘/ / ⌘, / ⌘1..9 / Esc) */
  useGlobalShortcuts({
    onToggleSearch: useCallback(() => {
      setPage("search");
    }, []),
    onToggleAi: useCallback(() => {
      setAiOpen((prev) => !prev);
    }, []),
    onOpenSettings: useCallback(() => {
      setSettingsOpen(true);
    }, []),
    onCloseModals: useCallback(() => {
      setSettingsOpen(false);
      setAiOpen(false);
      setConversationOpen(false);
    }, []),
    onSelectPage: useCallback((targetPage) => {
      setPage(targetPage);
    }, []),
  });

  /** 执行 AI 的 Action 请求（V4 §51：Frontend 决定是否执行）。 */
  const handleAiNavigate = useCallback(
    (action: AgentAction) => {
      const target = (action.target ?? {}) as {
        kind?: string;
        id?: string;
        entityId?: string;
      };
      const id = target.id ?? target.entityId;
      switch (action.module) {
        case "history": {
          if (id) {
            const kind =
              target.kind === "person"
                ? "person"
                : target.kind === "story"
                  ? "story"
                  : "event";
            openHistory(id, kind);
          } else {
            setPage("history");
          }
          break;
        }
        case "geography":
          if (id) openGeography(id);
          else setPage("geography");
          break;
        case "travel":
          setPage("travel");
          break;
        case "language":
          if (id) openLanguage(id);
          else setPage("language");
          break;
        case "markdown":
          setPage("markdown");
          break;
        default:
          setPage("home");
      }
    },
    [openHistory, openGeography, openLanguage],
  );

  /** 切换到无上报器的页面时，清除当前上下文（回到 General，§40）。 */
  useEffect(() => {
    if (
      page === "home" ||
      page === "markdown" ||
      page === "rss"
    ) {
      setAiContext(null);
    }
  }, [page]);

  /** 上下文 chip 文案（如 “History · 毛泽东”）；无上下文 = General。 */
  const aiContextLabel = useMemo(() => {
    if (!aiContext) return null;
    const moduleName = aiContext.module
      ? aiContext.module.charAt(0).toUpperCase() + aiContext.module.slice(1)
      : null;
    const entityName = aiContext.entity?.label ?? aiContext.entity?.id ?? null;
    if (moduleName && entityName) return `${moduleName} · ${entityName}`;
    return entityName ?? moduleName;
  }, [aiContext]);

  return (
    <div className={"app-shell app-shell-" + layout.device}>
      <PwaBanner />
      <header className="app-bar">
        <div
          className="brand"
          onClick={() => setPage("home")}
          style={{ cursor: "pointer" }}
        >
          <strong>self-tools</strong>
          <span />
          <p>Personal AI Hub</p>
        </div>
        <div className="app-bar-actions">
          <button
            className={
              "app-bar-btn app-bar-search" +
              (page === "search" ? " active" : "")
            }
            title="Search & Commands (⌘K)"
            onClick={() => setPage(page === "search" ? "home" : "search")}
          >
            <MagnifyingGlass size={16} />
            <span className="app-bar-hotkey">⌘K</span>
          </button>
          <button
            className="app-bar-gear"
            title="Settings (⌘,)"
            aria-label="Settings"
            onClick={() => setSettingsOpen(true)}
          >
            <Gear size={19} />
          </button>
          <button
            className={"app-bar-ai" + (aiOpen ? " active" : "")}
            title="Ask AI (⌘/)"
            aria-label="Ask AI"
            onClick={() => setAiOpen((prev) => !prev)}
          >
            <Sparkle size={18} weight="fill" />
            <span className="app-bar-hotkey">⌘/</span>
          </button>
        </div>
      </header>
      <div className="app-body">
        {navigationLayout(layout) === "side" ? (
        <nav className="app-nav" aria-label="功能导航">
          {NAV_GROUPS.map((group) => (
            <div className="app-nav-group" key={group.label}>
              <span className="app-nav-group-label">{group.label}</span>
              {group.items.map((item) => (
                <button
                  key={item.id}
                  className={"app-nav-item" + (page === item.id ? " active" : "")}
                  onClick={() => setPage(item.id)}
                  // 按钮内含快捷键徽标与未读数，文本节点不是稳定标识；
                  // 显式 aria-label 同时让读屏只念模块名而不念「⌘1」。
                  aria-label={item.label}
                  aria-current={page === item.id ? "page" : undefined}
                >
                  <item.icon size={18} />
                  {item.label}
                  {shortcutDigitForPage(item.id) ? (
                    // 按真实快捷键映射渲染，而不是侧边栏顺序：
                    // Review/Graph/Collections 没有数字快捷键，显示序号会误导。
                    <span className="nav-shortcut">
                      ⌘{shortcutDigitForPage(item.id)}
                    </span>
                  ) : null}
                  {item.id === "rss" && unreadTotal > 0 ? (
                    <b>{unreadTotal > 99 ? "99+" : unreadTotal}</b>
                  ) : null}
                  {item.id === "news" && newsUnreadTotal > 0 ? (
                    <b>{newsUnreadTotal > 99 ? "99+" : newsUnreadTotal}</b>
                  ) : null}
                </button>
              ))}
            </div>
          ))}
          <div className="app-nav-divider" />
          <button
            className={"app-nav-item" + (settingsOpen ? " active" : "")}
            onClick={() => setSettingsOpen(true)}
          >
            <Gear size={18} />
            Settings
          </button>
          <footer className="app-nav-footer">Personal Workspace</footer>
        </nav>
        ) : null}
        <main className="app-content">
          <section
            className={"page-pane" + (page === "home" ? "" : " page-hidden")}
          >
            <HomePage
              recentFiles={settings.recent_files}
              latestArticles={latestArticles}
              geographyHome={geographyHome}
              historyHome={historyHome}
              platformToday={platformToday}
              rssRefreshing={rssRefreshing}
              onOpenNote={openNote}
              onOpenArticle={openArticle}
              onOpenGeography={openGeography}
              onOpenHistory={openHistory}
              onOpenLanguage={openLanguage}
              onNewNote={newNote}
              onRefreshRss={() => void refreshFeeds()}
              onAskAi={() => setAiOpen(true)}
              onOpenServer={() => setPage("server")}
              onOpenStudyBoard={() => setPage("study-board")}
              onOpenKnowledge={() => setPage("knowledge")}
              onNavigate={navigateToHash}
            />
          </section>
          <section
            className={"page-pane" + (page === "review" ? "" : " page-hidden")}
          >
            <ReviewCenterPage
              onNavigate={navigateToHash}
              onAskAi={(prompt) => deliverToAi(prompt)}
            />
          </section>
          <section
            className={"page-pane" + (page === "graph" ? "" : " page-hidden")}
          >
            <KnowledgeGraphPage
              active={page === "graph"}
              onNavigate={navigateToHash}
              onAskAi={(prompt) => deliverToAi(prompt)}
            />
          </section>
          <section
            className={"page-pane" + (page === "collections" ? "" : " page-hidden")}
          >
            <CollectionsPage
              onNavigate={navigateToHash}
              onAskAi={(prompt) => deliverToAi(prompt)}
            />
          </section>
          <section
            className={"page-pane" + (page === "system" ? "" : " page-hidden")}
          >
            <SystemReadinessPage active={page === "system"} />
          </section>
          <section
            className={"page-pane" + (page === "search" ? "" : " page-hidden")}
          >
            <GlobalSearchPage
              active={page === "search"}
              onNavigate={(module, target) => {
                if (module === "history") {
                  const id = target.id || target.entityId || target.entity_id;
                  const kind = target.kind || target.entityType || target.entity_type;
                  openHistory(id ? String(id) : undefined, kind ? (String(kind) as "event" | "person" | "story") : undefined);
                } else if (module === "geography") {
                  const id = target.id || target.entityId || target.entity_id;
                  openGeography(id ? String(id) : undefined);
                } else if (module === "language") {
                  const id = target.id || target.entityId || target.entity_id;
                  openLanguage(id ? String(id) : undefined);
                } else if (module === "knowledge" || module === "memory" || module === "documents" || module === "files") {
                  const tab = module === "memory" ? "memory" : module === "documents" ? "documents" : module === "files" ? "files" : ((target.tab as "memory" | "documents" | "files") ?? "memory");
                  const docId = target.documentId || target.doc || target.id;
                  openKnowledge(tab, docId ? String(docId) : undefined);
                } else if (module === "markdown") {
                  if (target.path) openNote(String(target.path));
                  else setPage("markdown");
                } else if (module === "study_board" || module === "study") {
                  setPage("study-board");
                } else if (PAGE_IDS.includes(module as PageId)) {
                  setPage(module as PageId);
                }
              }}
              onNewNote={newNote}
              onRefreshRss={() => void refreshFeeds()}
              onAskAi={() => setAiOpen(true)}
              onOpenSettings={() => setSettingsOpen(true)}
            />
          </section>
          <section
            className={"page-pane" + (page === "study-board" ? "" : " page-hidden")}
          >
            <StudyBoardPage
              active={page === "study-board"}
              onContextChange={setAiContext}
              onAskAi={(prompt, snapshot) => {
                // V11 §110：板 → 快照 → BoardSnapshot/Image ContentPart → PersonalAgent。
                setAiContext((current) => ({
                  ...(current ?? { module: "study-board", page: "board" }),
                  module: "study-board",
                  page: "board",
                  view_state: { snapshot_bytes: snapshot?.length ?? 0, prompt },
                }));
                const parts: Array<
                  | { type: "text"; text: string }
                  | {
                      type: "image";
                      data: string;
                      mime: string;
                      source: string;
                      caption?: string;
                    }
                > = [{ type: "text", text: prompt }];
                if (snapshot) {
                  parts.push({
                    type: "image",
                    source: "base64",
                    data: snapshot,
                    mime: "image/png",
                    caption: "学习板快照",
                  });
                }
                deliverToAi(prompt, null, parts);
              }}
            />
          </section>
          <section
            className={
              "page-pane" + (page === "markdown" ? "" : " page-hidden")
            }
          >
            <MarkdownPage
              settings={settings}
              onSettingsChange={(next) => void updateSettings(next)}
              setNotice={setNotice}
              active={page === "markdown"}
              intent={markdownIntent}
              initialWorkspace={
                settingsLoaded ? settings.workspace_path : undefined
              }
            />
          </section>
          <section
            className={"page-pane" + (page === "rss" ? "" : " page-hidden")}
          >
            <RssPage
              active={page === "rss"}
              version={rssVersion}
              refreshing={rssRefreshing}
              onRefresh={() => void refreshFeeds()}
              onFeedsChanged={handleFeedsChanged}
              setNotice={setNotice}
              intent={rssIntent}
            />
          </section>
          <section
            className={"page-pane news-pane" + (page === "news" ? "" : " page-hidden")}
          >
            <NewsPage
              active={page === "news"}
              onContextChange={setAiContext}
              onAskAi={(prompt, context) => deliverToAi(prompt, context)}
              onUnreadChanged={setNewsUnreadTotal}
              setNotice={setNotice}
            />
          </section>
          <section
            className={"page-pane" + (page === "travel" ? "" : " page-hidden")}
          >
            <TravelPage
              active={page === "travel"}
              setNotice={setNotice}
              onContextChange={setAiContext}
              amapApiKey={settings.geography.amap_api_key}
              amapSecurityJsCode={settings.geography.amap_security_js_code}
            />
          </section>
          <section
            className={
              "page-pane geography-pane" +
              (page === "geography" ? "" : " page-hidden")
            }
          >
            <GeographyPage
              active={page === "geography"}
              setNotice={setNotice}
              onContextChange={setAiContext}
              amapApiKey={settings.geography.amap_api_key}
              amapSecurityJsCode={settings.geography.amap_security_js_code}
              intent={geographyIntent}
            />
          </section>
          <section
            className={
              "page-pane history-pane" +
              (page === "history" ? "" : " page-hidden")
            }
          >
            <HistoryPage
              active={page === "history"}
              setNotice={setNotice}
              intent={historyIntent}
              onContextChange={setAiContext}
              onNavigateToGeography={(request) =>
                openGeography(request.entityId)
              }
            />
          </section>
          <section
            className={
              "page-pane language-pane" +
              (page === "language" ? "" : " page-hidden")
            }
          >
            <LanguagePage
              active={page === "language"}
              setNotice={setNotice}
              intent={languageIntent}
              onContextChange={setAiContext}
              onAskAi={(prompt) => deliverToAi(prompt)}
            />
          </section>
          <section
            className={
              "page-pane knowledge-pane" +
              (page === "knowledge" ? "" : " page-hidden")
            }
          >
            <KnowledgePage
              active={page === "knowledge"}
              setNotice={setNotice}
              onOpenSettings={() => setSettingsOpen(true)}
              intent={knowledgeIntent}
              onContextChange={setAiContext}
            />
          </section>
          <section
            className={
              "page-pane server-pane" +
              (page === "server" ? "" : " page-hidden")
            }
          >
            <ServerPage active={page === "server"} setNotice={setNotice} />
          </section>
        </main>
      </div>
      {navigationLayout(layout) === "bottom" ? (
        <nav className="app-bottom-nav" aria-label="主导航">
          {NAV_GROUPS.flatMap((group) => group.items)
            .map((item) => (
              <button
                key={item.id}
                className={"app-bottom-nav-item" + (page === item.id ? " active" : "")}
                onClick={() => setPage(item.id)}
              >
                <item.icon size={20} />
                <span>{item.label}</span>
              </button>
            ))}
          <button
            className={"app-bottom-nav-item" + (settingsOpen ? " active" : "")}
            onClick={() => setSettingsOpen(true)}
          >
            <Gear size={20} />
            <span>Settings</span>
          </button>
        </nav>
      ) : null}
      {notice ? (
        <button className="toast" onClick={() => setNotice("")}>
          {notice}
          <X size={16} />
        </button>
      ) : null}
      <AIPanel
        open={aiOpen}
        onClose={() => setAiOpen(false)}
        conversationOpen={conversationOpen}
        onConversationOpenChange={setConversationOpen}
        pendingSend={aiDelivery}
        context={aiContext}
        contextLabel={aiContextLabel}
        onClearContext={() => setAiContext(null)}
        onNavigate={handleAiNavigate}
        onOpenSettings={() => setSettingsOpen(true)}
        onConfirmMemory={async (target) => {
          if (target.memory_id) {
            await knowledgeClient.memoryConfirm(target.memory_id);
            setNotice("已记住");
            return;
          }
          await knowledgeClient.memorySave({
            category: target.category,
            content: target.content,
            source_type: target.source_type ?? "conversation_candidate",
            source_reference: target.source_reference ?? null,
          });
          setNotice("已记住");
        }}
        onDismissMemory={async (target) => {
          if (target.memory_id) {
            await knowledgeClient.memoryReject(target.memory_id);
            setNotice("已忽略这条记忆");
          }
        }}
        onOpenFile={(target) => {
          void openPath(target.path).catch((error) =>
            setNotice(`打开文件失败：${errorMessage(error)}`),
          );
        }}
        onOpenDocument={(target) => {
          setKnowledgeIntent({
            tab: "documents",
            documentId: target.document_id,
            nonce: Date.now(),
          });
          setPage("knowledge");
        }}
        onConfirmSystemAction={async (confirmation, decision) => {
          if (decision === "cancel") {
            await serverClient.cancelAction(confirmation.confirmation_id);
            setNotice("已取消");
            return;
          }
          const result = await serverClient.confirmAction(
            confirmation.confirmation_id,
            confirmation.target_id,
          );
          setNotice(
            result.outcome === "success"
              ? `已重启 ${confirmation.target_id}`
              : `操作结果：${result.outcome}`,
          );
        }}
      />
      {settingsOpen ? (
        <SettingsDialog
          themeId={themeId}
          onThemeChange={changeTheme}
          rssRefreshMinutes={settings.rss_refresh_minutes}
          onRefreshMinutesChange={changeRefreshMinutes}
          travel={settings.travel}
          onTravelChange={(next) =>
            void updateSettings({ ...settings, travel: next })
          }
          geography={settings.geography}
          onGeographyChange={(next) =>
            void updateSettings({ ...settings, geography: next })
          }
          ai={settings.ai}
          onAiChange={(next) => void updateSettings({ ...settings, ai: next })}
          knowledge={settings.knowledge}
          onKnowledgeChange={(next) =>
            void updateSettings({ ...settings, knowledge: next })
          }
          server={settings.server}
          onServerChange={(next) =>
            void updateSettings({ ...settings, server: next })
          }
          onClose={() => setSettingsOpen(false)}
        />
      ) : null}
    </div>
  );
}
