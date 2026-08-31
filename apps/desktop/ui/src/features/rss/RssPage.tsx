import DOMPurify from "dompurify";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ArrowBendUpLeft, ArrowSquareOut, ArrowsClockwise, Check, Plus, Rss, TextT, TrashSimple } from "@phosphor-icons/react";
import { useCallback, useEffect, useState, type MouseEvent } from "react";
import type { ArticleDto, FeedDto } from "../../types";
import { errorMessage, formatDateTime, formatRelativeTime, isTauriRuntime } from "../../utils";

/**
 * RSS Feature:订阅管理 + 文章列表 + 阅读面板。
 * 数据全部来自 Rust 命令层(SQLite 持久化 + reqwest/feed-rs 抓取解析);
 * 刷新由外壳统一调度(定时 + 手动),本页面只负责展示与交互。
 */

export type RssIntent = { article: ArticleDto; nonce: number };

interface RssPageProps {
  active: boolean;
  /** 外壳刷新代数:每次后台/手动刷新后 +1,页面据此重载列表 */
  version: number;
  refreshing: boolean;
  onRefresh: () => void;
  /** 数据变化后通知外壳(用于导航未读徽标) */
  onFeedsChanged: (feeds: FeedDto[]) => void;
  setNotice: (message: string) => void;
  intent: RssIntent | null;
}

/** 按标题前缀识别「回复」类条目(Re:/RE:/回复:/答复: 等,常见于论坛/通知类合并回复流的 Feed)。 */
function isReplyTitle(title: string): boolean {
  return /^(?:re|reply|回复|答复|回覆|r)\s*[:：]\s*/i.test(title);
}

/** 文章列表过滤选项:全部 / 只看主题 / 只看回复。 */
const ARTICLE_FILTERS = [
  { key: "all", label: "全部" },
  { key: "topic", label: "主题" },
  { key: "reply", label: "回复" },
] as const;
type ArticleFilter = typeof ARTICLE_FILTERS[number]["key"];

export function RssPage({ active, version, refreshing, onRefresh, onFeedsChanged, setNotice, intent }: RssPageProps) {
  const [feeds, setFeeds] = useState<FeedDto[]>([]);
  const [selectedFeedId, setSelectedFeedId] = useState<number | null>(null);
  const [articles, setArticles] = useState<ArticleDto[]>([]);
  const [selectedArticle, setSelectedArticle] = useState<ArticleDto | null>(null);
  const [newUrl, setNewUrl] = useState("");
  const [adding, setAdding] = useState(false);
  const [fetchedHtml, setFetchedHtml] = useState<string | null>(null);
  const [fetchingArticle, setFetchingArticle] = useState(false);
  /** 文章列表过滤:全部 / 只看主题 / 只看回复。 */
  const [articleFilter, setArticleFilter] = useState<ArticleFilter>("all");

  const loadFeeds = useCallback(async () => {
    if (!isTauriRuntime()) return;
    try {
      const list = await invoke<FeedDto[]>("list_rss_feeds");
      setFeeds(list);
      onFeedsChanged(list);
    } catch (error) { setNotice(errorMessage(error)); }
  }, [onFeedsChanged, setNotice]);

  const loadArticles = useCallback(async (feedId: number) => {
    if (!isTauriRuntime()) return;
    try {
      setArticles(await invoke<ArticleDto[]>("list_rss_articles", { feedId, limit: 200 }));
    } catch (error) { setNotice(errorMessage(error)); }
  }, [setNotice]);

  useEffect(() => { void loadFeeds(); }, [loadFeeds]);

  useEffect(() => {
    if (version === 0) return;
    void loadFeeds();
    if (selectedFeedId !== null) void loadArticles(selectedFeedId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [version]);

  useEffect(() => {
    if (selectedFeedId !== null) void loadArticles(selectedFeedId);
  }, [selectedFeedId, loadArticles]);

  /** 外壳意图:从 Home 打开某篇文章 */
  useEffect(() => {
    if (!intent) return;
    setSelectedFeedId(intent.article.feed_id);
    void openArticle(intent.article);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [intent?.nonce]);

  const markRead = useCallback(async (article: ArticleDto) => {
    if (article.is_read) return;
    try {
      if (isTauriRuntime()) await invoke("mark_rss_article_read", { articleId: article.id });
      const readArticle = { ...article, is_read: true };
      setSelectedArticle(readArticle);
      setArticles((previous) => previous.map((item) => (item.id === article.id ? readArticle : item)));
      setFeeds((previous) => {
        const next = previous.map((feed) => feed.id === article.feed_id ? { ...feed, unread_count: Math.max(0, feed.unread_count - 1) } : feed);
        onFeedsChanged(next);
        return next;
      });
    } catch (error) { setNotice(errorMessage(error)); }
  }, [onFeedsChanged, setNotice]);

  const openArticle = useCallback(async (article: ArticleDto) => {
    setSelectedArticle(article);
    await markRead(article);
  }, [markRead]);

  const addFeed = async () => {
    const url = newUrl.trim();
    if (!url) return;
    setAdding(true);
    try {
      const feed = await invoke<FeedDto>("add_rss_feed", { url });
      setNewUrl("");
      setNotice(`已订阅 ${feed.title}`);
      await loadFeeds();
      setSelectedFeedId(feed.id);
    } catch (error) { setNotice(errorMessage(error)); } finally { setAdding(false); }
  };

  const deleteFeed = async (feed: FeedDto) => {
    if (!window.confirm(`确定取消订阅「${feed.title}」吗？其文章也会一并删除。`)) return;
    try {
      await invoke("delete_rss_feed", { feedId: feed.id });
      if (selectedFeedId === feed.id) { setSelectedFeedId(null); setArticles([]); setSelectedArticle(null); }
      setNotice(`已取消订阅 ${feed.title}`);
      await loadFeeds();
    } catch (error) { setNotice(errorMessage(error)); }
  };

  const openInBrowser = async (url: string) => {
    if (!url) return;
    if (isTauriRuntime()) await openUrl(url);
    else window.open(url, "_blank");
  };

  /** 切换文章时重置抓取结果 */
  useEffect(() => { setFetchedHtml(null); setFetchingArticle(false); }, [selectedArticle?.id]);

  /** 按需抓取文章页正文(仅当 RSS 正文被截断时手动触发)。 */
  const fetchFullText = async (article: ArticleDto) => {
    if (!article.url) return;
    if (!isTauriRuntime()) { window.open(article.url, "_blank"); return; }
    setFetchingArticle(true);
    try {
      const html = await invoke<string>("fetch_article_url", { url: article.url });
      const extracted = extractArticle(html);
      if (extracted.trim().length < 80) {
        setNotice("未能提取正文，页面可能需要登录或依赖脚本渲染。请在浏览器打开原文。");
        return;
      }
      setFetchedHtml(extracted);
    } catch (error) { setNotice(errorMessage(error)); } finally { setFetchingArticle(false); }
  };

  /** 阅读面板内点击任意链接(如源站自带的「查看全文」):一律在系统浏览器打开,避免 Tauri 窗口内跳转。 */
  const handleContentClick = (article: ArticleDto) => (event: MouseEvent<HTMLDivElement>) => {
    const anchor = (event.target as HTMLElement).closest("a");
    const href = anchor?.getAttribute("href");
    if (!href) return;
    event.preventDefault();
    const absolute = resolveUrl(href, article.url || undefined);
    if (absolute) void openInBrowser(absolute);
  };

  return <div className="rss-page">
    <aside className="rss-feeds">
      <header className="rss-feeds-header">
        <p>Feeds</p>
        <button title="立即刷新全部订阅" onClick={onRefresh} disabled={refreshing}><ArrowsClockwise size={15} className={refreshing ? "spin" : undefined} /></button>
      </header>
      <div className="rss-add">
        <input value={newUrl} onChange={(event) => setNewUrl(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") void addFeed(); }} placeholder="https://example.com/feed.xml" aria-label="RSS URL" />
        <button onClick={() => void addFeed()} disabled={adding || !newUrl.trim()} title="添加订阅"><Plus size={16} /></button>
      </div>
      <div className="rss-feed-list">
        {feeds.length === 0
          ? <p className="rss-empty">还没有订阅。<br />输入 RSS URL 添加第一个 Feed。</p>
          : feeds.map((feed) => <div className={"rss-feed-row" + (selectedFeedId === feed.id ? " selected" : "")} key={feed.id}>
            <button className="rss-feed-open" title={feed.last_error ? `上次刷新失败:${feed.last_error}` : feed.url} onClick={() => setSelectedFeedId(feed.id)}>
              <Rss size={15} />
              <span>{feed.title}</span>
              {feed.unread_count > 0 && <b>{feed.unread_count}</b>}
            </button>
            <button className="rss-feed-delete" title="取消订阅" onClick={() => void deleteFeed(feed)}><TrashSimple size={14} /></button>
          </div>)}
      </div>
      <footer className="rss-feeds-footer"><Rss size={15} />{feeds.length} feeds</footer>
    </aside>
    <section className="rss-articles">
      {selectedFeedId === null
        ? <p className="rss-empty-pane">选择左侧订阅查看文章列表。</p>
        : articles.length === 0
          ? <p className="rss-empty-pane">这个订阅还没有文章,试试刷新。</p>
          : <>
            <div className="rss-article-filter">{ARTICLE_FILTERS.map(({ key, label }) => <button key={key} className={articleFilter === key ? "selected" : ""} onClick={() => setArticleFilter(key)}>{label}<b>{key === "all" ? articles.length : articles.filter((article) => isReplyTitle(article.title) === (key === "reply")).length}</b></button>)}</div>
            <div className="rss-article-list">
              {articles.filter((article) => articleFilter === "all" || isReplyTitle(article.title) === (articleFilter === "reply")).map((article) => {
                const reply = isReplyTitle(article.title);
                return <button key={article.id} className={"rss-article-item" + (article.is_read ? " read" : "") + (selectedArticle?.id === article.id ? " selected" : "") + (reply ? " reply" : "")} onClick={() => void openArticle(article)}>
                  {!article.is_read && <i className="rss-unread-dot" />}
                  <span className="rss-article-title">{reply ? <span className="rss-reply-badge"><ArrowBendUpLeft size={12} weight="bold" />回复</span> : null}{article.title}</span>
                  <small className="rss-article-meta">{article.feed_title} · {formatRelativeTime(article.published_at)}</small>
                  {article.summary ? <p className="rss-article-snippet">{stripHtml(article.summary)}</p> : null}
                </button>;
              })}
            </div>
          </>}
    </section>
    <section className="rss-reader">
      {selectedArticle === null
        ? <p className="rss-empty-pane">点击文章开始阅读。</p>
        : <article className="rss-reading">
          <h2>{selectedArticle.title}</h2>
          <p className="rss-reading-meta">{isReplyTitle(selectedArticle.title) ? <><span className="rss-reply-badge"><ArrowBendUpLeft size={12} weight="bold" />回复</span> · </> : null}{selectedArticle.feed_title} · {formatDateTime(selectedArticle.published_at)}</p>
          <div className="rss-reading-actions">
            {selectedArticle.url ? <button onClick={() => void openInBrowser(selectedArticle.url)}><ArrowSquareOut size={16} />在浏览器打开</button> : null}
            {selectedArticle.url ? <button onClick={() => void fetchFullText(selectedArticle)} disabled={fetchingArticle} title="RSS 只有摘要时，抓取文章页正文"><TextT size={16} />{fetchingArticle ? "抓取中…" : fetchedHtml ? "重新抓取" : "抓取全文"}</button> : null}
            <span className="rss-read-tag"><Check size={14} />已读</span>
          </div>
          {fetchedHtml
            ? <p className="rss-fetch-note">已加载网页全文</p>
            : null}
          {fetchedHtml
            ? <div className="rss-reading-content rss-fetched" onClick={handleContentClick(selectedArticle)} dangerouslySetInnerHTML={{ __html: prepareContent(fetchedHtml, selectedArticle.url) }} />
            : selectedArticle.summary
              ? <div className="rss-reading-content" onClick={handleContentClick(selectedArticle)} dangerouslySetInnerHTML={{ __html: prepareContent(selectedArticle.summary, selectedArticle.url) }} />
              : <p className="rss-empty">该文章没有摘要内容,可打开原文阅读。</p>}
        </article>}
    </section>
  </div>;
}

/**
 * 从文章页 HTML 抽取正文容器(阅读器常用启发式):先去噪音,再找语义容器,
 * 最后回退到最长文本块。返回的是待净化的片段,由 prepareContent 再处理。
 */
function extractArticle(html: string): string {
  const document = new DOMParser().parseFromString(html, "text/html");
  document.querySelectorAll(
    "script,style,noscript,iframe,nav,footer,header,aside,form,canvas,svg," +
    "[class*='ad'],[class*='advert'],[class*='comment'],[class*='sidebar']," +
    "[class*='related'],[class*='footer'],[class*='nav'],[class*='share'],[class*='meta']," +
    "[id*='comment'],[id*='sidebar'],[id*='advert']",
  ).forEach((node) => node.remove());
  const candidates = [
    "article", "[role='main']", "main", ".article-content", ".post-content",
    ".entry-content", ".article", ".post", "#article-content", "#content", ".content", "#article",
  ];
  for (const selector of candidates) {
    const element = document.querySelector(selector);
    if (element && (element.textContent || "").trim().length > 400) return element.innerHTML;
  }
  let best: Element | null = null;
  let bestLength = 0;
  for (const child of Array.from(document.body?.children ?? [])) {
    const length = (child.textContent || "").trim().length;
    if (length > bestLength) { bestLength = length; best = child; }
  }
  return best ? best.innerHTML : document.body ? document.body.innerHTML : "";
}

/** 相对链接基于文章原文地址解析为绝对地址。 */
function resolveUrl(href: string, base?: string): string {
  try { return new URL(href, base).toString(); } catch { return href; }
}

/**
 * 阅读正文:先 DOMPurify 净化,再把相对 `a[href]` / `img[src]` 基于原文地址补全为绝对地址
 * (feedparser / Miniflux 在聚合端的标准行为,否则正文里的相对图片会 404)。
 */
function prepareContent(html: string, baseUrl?: string): string {
  const clean = DOMPurify.sanitize(html);
  if (!baseUrl) return clean;
  try {
    const document = new DOMParser().parseFromString(clean, "text/html");
    document.querySelectorAll("a[href], img[src]").forEach((element) => {
      const attribute = element.tagName === "A" ? "href" : "src";
      const value = element.getAttribute(attribute);
      if (!value) return;
      try { element.setAttribute(attribute, new URL(value, baseUrl).toString()); } catch { /* 保持原样 */ }
    });
    document.querySelectorAll("img").forEach((image) => image.setAttribute("loading", "lazy"));
    return document.body.innerHTML;
  } catch {
    return clean;
  }
}

/** 列表摘要:取纯文本,并去掉源站截断尾巴上的「…查看全文」等链接文字。 */
function stripHtml(html: string): string {
  const template = document.createElement("div");
  template.innerHTML = DOMPurify.sanitize(html);
  const text = (template.textContent || "").replace(/\s+/g, " ").trim();
  return text
    .replace(/(?:…{1,2}|\.{2,6}|⋯+)?\s*(?:查看全文|阅读全文|继续阅读|[Rr]ead\s*[Mm]ore)\s*$/u, "")
    .replace(/(?:…{1,2}|\.{2,6}|⋯+)\s*$/u, "")
    .trim();
}
