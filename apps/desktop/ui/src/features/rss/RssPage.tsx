import DOMPurify from "dompurify";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ArrowSquareOut, ArrowsClockwise, Check, Plus, Rss, TrashSimple } from "@phosphor-icons/react";
import { useCallback, useEffect, useState } from "react";
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

export function RssPage({ active, version, refreshing, onRefresh, onFeedsChanged, setNotice, intent }: RssPageProps) {
  const [feeds, setFeeds] = useState<FeedDto[]>([]);
  const [selectedFeedId, setSelectedFeedId] = useState<number | null>(null);
  const [articles, setArticles] = useState<ArticleDto[]>([]);
  const [selectedArticle, setSelectedArticle] = useState<ArticleDto | null>(null);
  const [newUrl, setNewUrl] = useState("");
  const [adding, setAdding] = useState(false);

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
          : <div className="rss-article-list">
            {articles.map((article) => <button key={article.id} className={"rss-article-item" + (article.is_read ? " read" : "") + (selectedArticle?.id === article.id ? " selected" : "")} onClick={() => void openArticle(article)}>
              {!article.is_read && <i className="rss-unread-dot" />}
              <span className="rss-article-title">{article.title}</span>
              <small className="rss-article-meta">{article.feed_title} · {formatRelativeTime(article.published_at)}</small>
              {article.summary ? <p className="rss-article-snippet">{stripHtml(article.summary)}</p> : null}
            </button>)}
          </div>}
    </section>
    <section className="rss-reader">
      {selectedArticle === null
        ? <p className="rss-empty-pane">点击文章开始阅读。</p>
        : <article className="rss-reading">
          <h2>{selectedArticle.title}</h2>
          <p className="rss-reading-meta">{selectedArticle.feed_title} · {formatDateTime(selectedArticle.published_at)}</p>
          <div className="rss-reading-actions">
            {selectedArticle.url ? <button onClick={() => void openInBrowser(selectedArticle.url)}><ArrowSquareOut size={16} />Open in Browser</button> : null}
            <span className="rss-read-tag"><Check size={14} />已读</span>
          </div>
          {selectedArticle.summary
            ? <div className="rss-reading-content" dangerouslySetInnerHTML={{ __html: DOMPurify.sanitize(selectedArticle.summary) }} />
            : <p className="rss-empty">该文章没有摘要内容,可打开原文阅读。</p>}
        </article>}
    </section>
  </div>;
}

/** 摘要片段用纯文本截断展示(阅读面板才渲染净化后的 HTML)。 */
function stripHtml(html: string): string {
  const template = document.createElement("div");
  template.innerHTML = DOMPurify.sanitize(html);
  return (template.textContent || "").replace(/\s+/g, " ").trim();
}
