/**
 * News（V12 / ADR-010：独立 bounded context 的新闻发现与阅读模块）。
 *
 * 数据流：newsClient（news_* 命令，读 + 用户状态 + 添加/删除新闻源）
 *         ← NewsService / NewsIngestService（application）
 *         ← NewsRepositoryPort → config/news.db（系统 seed + 抓取落地）
 * 摄取骨架（并发抓取）与 RSS 共享 `feed_fetcher`，但**领域与存储完全独立**：
 * 本页不调用任何 `add_rss_feed` / `set_rss_feed_kind` —— 不存在 RSS↔News 转换。
 *
 * AI 接入走统一出口 `onAskAi(prompt, context)`（与 StudyBoard / Knowledge 同模式）：
 * 组件不自己调 LLM、不持有 provider；Personal 侧由 App 的 AIPanel 承接。
 */

import { useCallback, useEffect, useRef, useState } from "react";
import {
  ArrowSquareOut,
  ArrowsClockwise,
  BookmarkSimple,
  Check,
  Sparkle,
  TextT,
  Warning,
} from "@phosphor-icons/react";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { AppContextPayload } from "../ai/aiTypes";
import type {
  NewsArticle,
  NewsOverview,
  NewsSource,
  RecommendedSource,
} from "../../types";
import { errorMessage, formatDateTime, formatRelativeTime, isTauriRuntime } from "../../utils";
import { newsClient } from "./newsClient";
import { learningClient } from "../learning/learningClient";
import { prepareRssContent, stripRssHtml } from "./newsContent";

/** 栏目：今日 / 稍后读 / 订阅源。 */
type NewsTab = "today" | "starred" | "sources";

/** 分类（与后端 `NewsCategory` 对齐；添加新闻源时写入 `news_sources.category`）。 */
const CATEGORY_LABELS: Record<string, string> = {
  general: "综合",
  tech: "科技",
  finance: "财经",
  world: "国际",
  china: "中国",
};

export interface NewsPageProps {
  active: boolean;
  /** 当前 AppContext（前端负责「我在哪」）。 */
  onContextChange?: (context: AppContextPayload | null) => void;
  /** 统一 AI 出口：把问题 + 上下文交给 PersonalAgent。 */
  onAskAi?: (prompt: string, context: AppContextPayload | null) => void;
  /** 订阅源变化后通知外壳（RSS 未读徽标）。 */
  onUnreadChanged?: (unreadTotal: number) => void;
  setNotice: (message: string) => void;
}

export function NewsPage({
  active,
  onContextChange,
  onAskAi,
  onUnreadChanged,
  setNotice,
}: NewsPageProps) {
  const [tab, setTab] = useState<NewsTab>("today");
  const [overview, setOverview] = useState<NewsOverview | null>(null);
  const [stories, setStories] = useState<NewsArticle[]>([]);
  const [selected, setSelected] = useState<NewsArticle | null>(null);
  const [sourceFilter, setSourceFilter] = useState<number | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [searching, setSearching] = useState(false);
  const [fetchedHtml, setFetchedHtml] = useState<string | null>(null);
  const [fetchingArticle, setFetchingArticle] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [recommendations, setRecommendations] = useState<RecommendedSource[]>([]);
  /** 添加新闻源（自定义 URL）—— 订阅第一个源后也必须能继续加。 */
  const [newSourceUrl, setNewSourceUrl] = useState("");
  const [addingSource, setAddingSource] = useState(false);
  /** 加载失败的图片 id 集合：源站图床常用防盗链，失败即隐藏而非留破图。 */
  const [brokenImages, setBrokenImages] = useState<ReadonlySet<number>>(new Set());
  const refreshNonce = useRef(0);

  const sources = overview?.sources ?? [];
  const hasSubscriptions = sources.length > 0;

  // --- 数据加载 -----------------------------------------------------------

  const loadOverview = useCallback(async () => {
    if (!isTauriRuntime()) return;
    try {
      const next = await newsClient.sources();
      setOverview(next);
      onUnreadChanged?.(
        next.sources.reduce((total, source) => total + source.unread_count, 0),
      );
    } catch (error) {
      setNotice(errorMessage(error));
    }
  }, [onUnreadChanged, setNotice]);

  const loadStories = useCallback(async () => {
    if (!isTauriRuntime()) return;
    try {
      if (tab === "starred") {
        setStories(await newsClient.starred(100));
      } else if (searchQuery.trim()) {
        setStories(await newsClient.search(searchQuery.trim(), 100));
      } else if (tab === "sources") {
        setStories([]);
      } else {
        setStories(await newsClient.headlines(null, sourceFilter, null, 100));
      }
    } catch (error) {
      setNotice(errorMessage(error));
    }
  }, [tab, sourceFilter, searchQuery, setNotice]);

  const autoRefreshedRef = useRef(false);

  /** 触发一次抓取（news_refresh_now → RSS 摄取管道；本页不复制刷新逻辑）。 */
  const refresh = useCallback(async () => {
    if (!isTauriRuntime()) return;
    setRefreshing(true);
    try {
      const report = await newsClient.refreshNow();
      if (report.failures.length > 0) {
        setNotice(`刷新完成，${report.failures.length} 个源失败`);
      }
      refreshNonce.current += 1;
      await Promise.all([loadOverview(), loadStories()]);
    } catch (error) {
      setNotice(errorMessage(error));
    } finally {
      setRefreshing(false);
    }
  }, [loadOverview, loadStories, setNotice]);

  useEffect(() => {
    void loadOverview();
  }, [loadOverview]);

  useEffect(() => {
    if (!active) return;
    void loadStories();
  }, [active, loadStories, refreshNonce.current]);

  /** 首次进入新闻页面时，若已有配置好的新闻源但本地尚无文章缓存，自动发起一次静默抓取。 */
  useEffect(() => {
    if (!active || autoRefreshedRef.current) return;
    if (sources.length > 0 && stories.length === 0 && !refreshing) {
      autoRefreshedRef.current = true;
      void refresh();
    }
  }, [active, sources.length, stories.length, refreshing, refresh]);

  /** 切文章时重置抓取结果。 */
  useEffect(() => {
    setFetchedHtml(null);
    setFetchingArticle(false);
  }, [selected?.id]);

  // --- 交互 ---------------------------------------------------------------

  const openStory = useCallback(
    async (story: NewsArticle) => {
      setSelected(story);
      void learningClient.recordEvent({
        module: "news",
        entity_type: "article",
        entity_id: String(story.id),
        title: story.title,
        action: "read",
      });
      if (story.is_read) return;
      try {
        await newsClient.markRead(story.id);
        const read = { ...story, is_read: true };
        setStories((previous) => previous.map((item) => (item.id === story.id ? read : item)));
        setSelected(read);
        // 未读计数只在前端递减，等下次 sources 拉取时与后端对齐。
        setOverview((current) =>
          current
            ? {
                ...current,
                sources: current.sources.map((source) =>
                  source.id === story.source_id
                    ? { ...source, unread_count: Math.max(0, source.unread_count - 1) }
                    : source,
                ),
              }
            : current,
        );
      } catch (error) {
        setNotice(errorMessage(error));
      }
    },
    [setNotice],
  );

  const toggleStar = useCallback(
    async (story: NewsArticle) => {
      try {
        const starred = await newsClient.toggleStar(story.id);
        const next = { ...story, starred };
        setStories((previous) => previous.map((item) => (item.id === story.id ? next : item)));
        if (selected?.id === story.id) setSelected(next);
        setNotice(starred ? "已加入稍后读" : "已取消收藏");
      } catch (error) {
        setNotice(errorMessage(error));
      }
    },
    [selected, setNotice],
  );

  /** 按需抓取文章页正文（仅 RSS 只有摘要时手动触发）。 */
  const fetchFullText = useCallback(
    async (story: NewsArticle) => {
      if (!story.url) return;
      setFetchingArticle(true);
      try {
        const html = await newsClient.fetchArticle(story.url);
        if (!html.trim()) {
          setNotice("未能提取正文，页面可能需要登录或依赖脚本渲染。");
          return;
        }
        setFetchedHtml(html);
      } catch (error) {
        setNotice(errorMessage(error));
      } finally {
        setFetchingArticle(false);
      }
    },
    [setNotice],
  );

  const openInBrowser = useCallback((url: string) => {
    if (!url) return;
    void openUrl(url);
  }, []);

  /** 统一 AI 出口：单篇新闻 + 用户问题。 */
  const askAbout = useCallback(
    (story: NewsArticle, prompt?: string) => {
      const context: AppContextPayload = {
        module: "news",
        page: selected?.id === story.id ? "reader" : "list",
        entity: {
          kind: "article",
          id: String(story.id),
          label: story.title,
        },
        view_state: {
          article: {
            id: story.id,
            source: story.source,
            source_id: story.source_id,
            title: story.title,
            url: story.url,
            author: story.author,
            published_at: story.published_at,
            summary: stripRssHtml(story.summary ?? "", story.url).slice(0, 600),
          },
        },
      };
      onAskAi?.(
        prompt ?? `请用简单的话解释这条新闻：${story.title}`,
        context,
      );
    },
    [onAskAi, selected],
  );

  /** AppContext：页面级（当前栏目/文章）。 */
  useEffect(() => {
    if (!onContextChange) return;
    if (!selected) {
      onContextChange({
        module: "news",
        page: tab === "starred" ? "starred" : tab === "sources" ? "sources" : "home",
        view_state: { sources: sources.length, tab },
      });
      return;
    }
    onContextChange({
      module: "news",
      page: "reader",
      entity: { kind: "article", id: String(selected.id), label: selected.title },
      view_state: {
        article: {
          id: selected.id,
          source: selected.source,
          title: selected.title,
          url: selected.url,
          published_at: selected.published_at,
        },
      },
    });
  }, [onContextChange, selected, tab, sources.length]);

  // --- 渲染 ---------------------------------------------------------------

  const degraded = overview?.health === "degraded";

  /** 推荐源只从后端目录取（URL 单一来源；不订阅 = 不抓取）。 */
  useEffect(() => {
    if (!active) return;
    if (!isTauriRuntime()) return;
    let cancelled = false;
    void (async () => {
      try {
        const list = await newsClient.recommended();
        if (!cancelled) setRecommendations(list);
      } catch (error) {
        if (!cancelled) setNotice(errorMessage(error));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [active, setNotice]);

  /** 订阅一个推荐源（走 RSS 摄取层；用户显式动作才写库）。 */
  const subscribe = useCallback(
    async (recommendation: RecommendedSource) => {
      if (!isTauriRuntime()) return;
      try {
        await newsClient.addSource(recommendation.url, recommendation.category);
        setNotice(`已订阅 ${recommendation.name}`);
        setRecommendations((current) =>
          current.filter((item) => item.url !== recommendation.url),
        );
        await refresh();
      } catch (error) {
        setNotice(errorMessage(error));
      }
    },
    [refresh, setNotice],
  );

  /** 添加自定义新闻源 URL（同一个 RSS 摄取入口，kind = news）。 */
  const addCustomSource = useCallback(async () => {
    const url = newSourceUrl.trim();
    if (!url) return;
    if (!isTauriRuntime()) return;
    setAddingSource(true);
    try {
      const source = await newsClient.addSource(newSourceUrl.trim(), "general");
      setNewSourceUrl("");
      setNotice(`已添加 ${source.name}`);
      await refresh();
    } catch (error) {
      setNotice(errorMessage(error));
    } finally {
      setAddingSource(false);
    }
  }, [newSourceUrl, refresh, setNotice]);


  return (
    <div className="news-page">
      <header className="news-header">
        <nav className="news-tabs" aria-label="新闻栏目">
          {(
            [
              ["today", "今日"],
              ["starred", "稍后读"],
              ["sources", "订阅源"],
            ] as [NewsTab, string][]
          ).map(([key, label]) => (
            <button
              key={key}
              className={tab === key ? "selected" : ""}
              onClick={() => setTab(key)}
            >
              {label}
            </button>
          ))}
        </nav>
        <div className="news-header-actions">
          <input
            value={searchQuery}
            onChange={(event) => setSearchQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") void loadStories();
            }}
            placeholder="搜索已缓存的新闻"
            aria-label="搜索新闻"
          />
          <button
            title="立即刷新全部订阅"
            onClick={() => void refresh()}
            disabled={refreshing || !hasSubscriptions}
          >
            <ArrowsClockwise size={15} className={refreshing ? "spin" : undefined} />
          </button>
        </div>
      </header>

      {degraded ? (
        <p className="news-degraded">
          <Warning size={14} /> 有订阅源最近刷新失败：{sources.find((s) => s.last_error)?.last_error}
        </p>
      ) : null}

      <div className="news-body">
        <section className="news-list">
          {!hasSubscriptions && tab === "today" ? (
            <div className="news-onboarding">
              <h3>选择你感兴趣的内容</h3>
              <p>推荐源只是候选 —— 确认订阅后才会抓取并存入本地数据库。</p>
              <ul>
                {recommendations.map((item) => (
                  <li key={item.url}>
                    <div>
                      <b>{item.name}</b>
                      <small>
                        {CATEGORY_LABELS[item.category] ?? item.category} · {item.note}
                      </small>
                    </div>
                    <button onClick={() => void subscribe(item)}>订阅</button>
                  </li>
                ))}
              </ul>
            </div>
          ) : tab === "sources" ? (
            <div className="news-sources">
              <h3>已订阅 {sources.length} 个新闻源</h3>
              {sources.map((source: NewsSource) => (
                <div className="news-source-row" key={source.id}>
                  <button onClick={() => { setSourceFilter(source.id); setTab("today"); }}>
                    <b>{source.name}</b>
                    <small>
                      未读 {source.unread_count}
                      {source.last_updated ? ` · ${formatRelativeTime(source.last_updated)}` : ""}
                      {source.last_error ? ` · ${source.last_error}` : ""}
                    </small>
                  </button>
                </div>
              ))}
              {sourceFilter !== null ? (
                <button className="news-clear-filter" onClick={() => setSourceFilter(null)}>
                  清除筛选：{sources.find((s) => s.id === sourceFilter)?.name}
                </button>
              ) : null}

              {/* 常驻添加入口：订阅第一个源后也必须能继续加。 */}
              <h3 className="news-sources-add-title">添加新闻源</h3>
              <div className="news-add-source">
                <input
                  value={newSourceUrl}
                  onChange={(event) => setNewSourceUrl(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") void addCustomSource();
                  }}
                  placeholder="https://example.com/feed.xml"
                  aria-label="新闻源 URL"
                />
                <button
                  onClick={() => void addCustomSource()}
                  disabled={addingSource || !newSourceUrl.trim()}
                >
                  {addingSource ? "添加中…" : "添加"}
                </button>
              </div>

              <h3 className="news-sources-add-title">推荐源</h3>
              {recommendations.length === 0 ? (
                <p className="news-empty">推荐源都已订阅。</p>
              ) : (
                <ul className="news-recommend-list">
                  {recommendations.map((item) => (
                    <li key={item.url}>
                      <div>
                        <b>{item.name}</b>
                        <small>
                          {CATEGORY_LABELS[item.category] ?? item.category} · {item.note}
                        </small>
                      </div>
                      <button onClick={() => void subscribe(item)}>订阅</button>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          ) : stories.length === 0 ? (
            <div className="news-empty" style={{ textAlign: "center", padding: "48px 24px" }}>
              <p style={{ color: "var(--text-secondary, #6b7280)", fontSize: 14, marginBottom: 16 }}>
                {tab === "starred"
                  ? "还没有收藏。阅读时点 ☆ 加入稍后读。"
                  : searchQuery.trim()
                    ? "本地缓存的新闻里没有命中该关键词。"
                    : "已配置内置新闻源，暂无本地抓取缓存。"}
              </p>
              {tab === "today" && !searchQuery.trim() && (
                <button
                  type="button"
                  onClick={() => void refresh()}
                  disabled={refreshing}
                  style={{
                    display: "inline-flex",
                    alignItems: "center",
                    gap: 6,
                    padding: "8px 18px",
                    borderRadius: 8,
                    background: "var(--accent-primary, #2563eb)",
                    color: "#ffffff",
                    border: "none",
                    fontWeight: 600,
                    fontSize: 13,
                    cursor: "pointer",
                    boxShadow: "0 2px 8px rgba(37, 99, 235, 0.2)",
                  }}
                >
                  <ArrowsClockwise size={15} className={refreshing ? "spin" : undefined} />
                  {refreshing ? "正在抓取最新新闻..." : "立即抓取最新新闻"}
                </button>
              )}
            </div>
          ) : (
            <ul className="news-story-list">
              {stories.map((story) => {
                // 无封面（或封面已加载失败）时列表项不渲染 <img>，列模板必须同步收掉缩略图列，
                // 否则 grid auto-placement 会把正文挤进那 72px 里。
                const cover = story.image_url && !brokenImages.has(story.id) ? story.image_url : null;
                return (
                  <li key={story.id}>
                    <button
                      className={
                        "news-story-item" +
                        (cover ? "" : " no-media") +
                        (story.is_read ? " read" : "") +
                        (selected?.id === story.id ? " selected" : "")
                      }
                      onClick={() => void openStory(story)}
                    >
                      {!story.is_read && <i className="news-unread-dot" />}
                      {cover ? (
                        <img
                          src={cover}
                          alt=""
                          loading="lazy"
                          onError={() =>
                            setBrokenImages((current) => new Set(current).add(story.id))
                          }
                        />
                      ) : null}
                      <span className="news-story-main">
                        <span className="news-story-title">{story.title}</span>
                        <small className="news-story-meta">
                          {story.source}
                          {story.author ? ` · ${story.author}` : ""}
                          {story.published_at ? ` · ${formatRelativeTime(story.published_at)}` : ""}
                        </small>
                        {story.summary ? (
                          <p className="news-story-snippet">{stripRssHtml(story.summary, story.url)}</p>
                        ) : null}
                      </span>
                      {story.starred && <BookmarkSimple size={14} weight="fill" />}
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </section>

        <section className="news-reader">
          {selected === null ? (
            <p className="news-empty-pane">选择一条新闻开始阅读。</p>
          ) : (
            <article className="news-reading">
              <h2>{selected.title}</h2>
              <p className="news-reading-meta">
                {selected.source}
                {selected.author ? ` · ${selected.author}` : ""}
                {selected.published_at ? ` · ${formatDateTime(selected.published_at)}` : ""}
              </p>
              {selected.image_url && !brokenImages.has(selected.id) ? (
                <img
                  className="news-hero"
                  src={selected.image_url}
                  alt=""
                  loading="lazy"
                  onError={() =>
                    setBrokenImages((current) => new Set(current).add(selected.id))
                  }
                />
              ) : null}
              <div className="news-reading-actions">
                <button onClick={() => askAbout(selected)} title="让 AI 用简单的话解释这条新闻">
                  <Sparkle size={15} /> 解释这条
                </button>
                <button onClick={() => askAbout(selected, "总结这条新闻的要点")}>总结要点</button>
                <button onClick={() => void toggleStar(selected)}>
                  <BookmarkSimple size={15} weight={selected.starred ? "fill" : "regular"} />
                  {selected.starred ? "已收藏" : "稍后读"}
                </button>
                {selected.url ? (
                  <button onClick={() => openInBrowser(selected.url)}>
                    <ArrowSquareOut size={15} /> 原文
                  </button>
                ) : null}
                {selected.url ? (
                  <button
                    onClick={() => void fetchFullText(selected)}
                    disabled={fetchingArticle}
                    title="RSS 只有摘要时，抓取文章页正文"
                  >
                    <TextT size={15} />
                    {fetchingArticle ? "抓取中…" : fetchedHtml ? "重新抓取" : "抓全文"}
                  </button>
                ) : null}
                <span className="news-read-tag">
                  <Check size={13} /> {selected.is_read ? "已读" : "未读"}
                </span>
              </div>
              {fetchedHtml ? <p className="news-fetch-note">已加载网页全文</p> : null}
              {!fetchedHtml && !selected.summary?.trim() ? (
                <p className="news-reading-empty">该新闻没有摘要，可打开原文阅读。</p>
              ) : (
                <div
                  className="news-reading-content"
                  onClick={(event) => {
                    const anchor = (event.target as HTMLElement).closest("a");
                    const href = anchor?.getAttribute("href");
                    if (href) {
                      event.preventDefault();
                      try {
                        openInBrowser(new URL(href, selected.url || undefined).toString());
                      } catch {
                        openInBrowser(href);
                      }
                    }
                  }}
                  dangerouslySetInnerHTML={{
                    __html: prepareRssContent(
                      fetchedHtml ?? selected.summary ?? "",
                      selected.url || undefined,
                    ),
                  }}
                />
              )}
            </article>
          )}
        </section>
      </div>
    </div>
  );
}
