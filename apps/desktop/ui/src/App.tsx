import {
  Brain,
  Compass,
  Gear,
  HardDrives,
  House,
  MapTrifold,
  Notebook,
  Rss,
  Scroll,
  Sparkle,
  Translate,
  Wrench,
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
import { languageClient } from "./features/language/languageClient";
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
import { TravelPage } from "./features/travel/TravelPage";
import { GeographyPage } from "./features/geography/GeographyPage";
import { applyTheme, getTheme, storeThemeId } from "./theme/ThemeManager";
import "./theme/themes";
import {
  useDeviceAttribute,
  useLayout,
  navigationLayout,
} from "./layout";
import { PwaBanner } from "./PwaBanner";
import type {
  AppSettings,
  ArticleDto,
  FeedDto,
  GeographyHome,
  SemanticHistoryHome,
  ReviewCard,
  TodayView,
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
  | "markdown"
  | "rss"
  | "travel"
  | "geography"
  | "history"
  | "language"
  | "knowledge"
  | "study-board"
  | "server"
  | "tools";

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
  { id: "knowledge", label: "Knowledge", icon: Brain },
  { id: "study-board", label: "Study", icon: Notebook },
  { id: "server", label: "Server", icon: HardDrives },
  { id: "tools", label: "Tools", icon: Wrench, disabled: true },
];

const defaultSettings: AppSettings = {
  schema_version: 1,
  recent_files: [],
  workspace_path: null,
  theme_mode: "dark",
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
  const [settings, setSettings] = useState<AppSettings>(defaultSettings);
  const [settingsLoaded, setSettingsLoaded] = useState(!isTauriRuntime());
  const [themeId, setThemeId] = useState("default");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [notice, setNotice] = useState("");
  const [aiOpen, setAiOpen] = useState(false);
  /** AI 的 App Context（Frontend 负责“我在哪”；History 页报告当前实体）。 */
  const [aiContext, setAiContext] = useState<AppContextPayload | null>(null);
  const [rssRefreshing, setRssRefreshing] = useState(false);
  const [rssVersion, setRssVersion] = useState(0);
  const [unreadTotal, setUnreadTotal] = useState(0);
  const [latestArticles, setLatestArticles] = useState<ArticleDto[]>([]);
  const [geographyHome, setGeographyHome] = useState<GeographyHome | null>(
    null,
  );
  const [historyHome, setHistoryHome] = useState<SemanticHistoryHome | null>(
    null,
  );
  const [todayView, setTodayView] = useState<TodayView | null>(null);
  const [reviewCard, setReviewCard] = useState<ReviewCard | null>(null);
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
      .then((loaded) => {
        setSettings(loaded);
        setThemeId(getTheme(loaded.ui_theme).id);
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
    const [geography, history, today, review] = await Promise.allSettled([
      geographyClient.home(0),
      historyClient.home(),
      languageClient.today("jpn"),
      languageClient.reviewNext("jpn"),
    ]);
    if (geography.status === "fulfilled") setGeographyHome(geography.value);
    if (history.status === "fulfilled") setHistoryHome(history.value);
    if (today.status === "fulfilled") setTodayView(today.value);
    if (review.status === "fulfilled") setReviewCard(review.value);
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
      setThemeId(normalized);
      storeThemeId(normalized);
      void updateSettings({ ...settings, ui_theme: normalized });
    },
    [settings, updateSettings],
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
      page === "rss" ||
      page === "tools"
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
        <div className="brand">
          <strong>self-tools</strong>
          <span />
          <p>Personal AI Hub</p>
        </div>
        <button
          className="app-bar-gear"
          title="Settings"
          onClick={() => setSettingsOpen(true)}
        >
          <Gear size={19} />
        </button>
        <button
          className="app-bar-ai"
          title="Ask AI"
          onClick={() => setAiOpen(true)}
        >
          <Sparkle size={18} weight="fill" />
        </button>
      </header>
      <div className="app-body">
        {navigationLayout(layout) === "side" ? (
        <nav className="app-nav" aria-label="功能导航">
          {NAV_ITEMS.map((item) =>
            item.disabled ? (
              <span
                className="app-nav-item disabled"
                key={item.id}
                title="即将推出"
              >
                <item.icon size={18} />
                {item.label}
              </span>
            ) : (
              <button
                key={item.id}
                className={"app-nav-item" + (page === item.id ? " active" : "")}
                onClick={() => setPage(item.id)}
              >
                <item.icon size={18} />
                {item.label}
                {item.id === "rss" && unreadTotal > 0 ? (
                  <b>{unreadTotal > 99 ? "99+" : unreadTotal}</b>
                ) : null}
              </button>
            ),
          )}
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
              todayView={todayView}
              reviewCard={reviewCard}
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
            />
          </section>
          <section
            className={"page-pane" + (page === "study-board" ? "" : " page-hidden")}
          >
            <StudyBoardPage
              active={page === "study-board"}
              onContextChange={setAiContext}
              onAskAi={(prompt, snapshot) => {
                // V11 §110：snapshot 作为 BoardSnapshot ContentPart 进入 PersonalAgent。
                // 当前 PersonalAgent 消息是 string 契约；快照经 AI 面板的粘贴通道发送，
                // 这里先把问题和上下文写进 session（后端 multimodal 契约落地后替换）。
                setAiContext((current) => ({
                  ...(current ?? { module: "study-board", page: "board" }),
                  module: "study-board",
                  page: "board",
                  view_state: { snapshot_bytes: snapshot?.length ?? 0, prompt },
                }));
                setAiOpen(true);
              }}
            />
          </section>
          <section
            className={
              "page-pane" + (page === "rss" ? "" : " page-hidden")
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
            className={"page-pane" + (page === "travel" ? "" : " page-hidden")}
          >
            <TravelPage
              active={page === "travel"}
              setNotice={setNotice}
              onContextChange={(ctx) => setAiContext(ctx)}
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
              onContextChange={(ctx) => setAiContext(ctx)}
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
              onContextChange={(ctx) => setAiContext(ctx)}
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
              onContextChange={(ctx) => setAiContext(ctx)}
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
              onContextChange={(ctx) => setAiContext(ctx)}
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
          {NAV_ITEMS.filter((item) => !item.disabled)
            .slice(0, 5)
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
