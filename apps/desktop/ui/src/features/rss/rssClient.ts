/**
 * RSS 模块的前端命令客户端（Gate 6）。
 *
 * 每个方法对应一个真实 Tauri 命令（命令名 / 参数 / 返回类型与后端契约冻结一致），
 * RssPage 与外壳（App）只调用本 client，不直接接触 transport 或 `@tauri-apps/api/core`。
 */
import type { CommandTransport } from "../../transport";
import { tauriTransport } from "../../transport";
import type { ArticleDto, FeedDto, RefreshReport } from "../../types";

export interface RssClient {
  listFeeds(): Promise<FeedDto[]>;
  listArticles(feedId: number, limit: number): Promise<ArticleDto[]>;
  addFeed(url: string): Promise<FeedDto>;
  deleteFeed(feedId: number): Promise<void>;
  markArticleRead(articleId: number): Promise<void>;
  fetchArticle(url: string): Promise<string>;
  refreshFeeds(): Promise<RefreshReport>;
  latestArticles(limit: number): Promise<ArticleDto[]>;
}

export function createRssClient(
  transport: CommandTransport = tauriTransport,
): RssClient {
  return {
    listFeeds: () => transport.invoke<FeedDto[]>("list_rss_feeds"),
    listArticles: (feedId, limit) =>
      transport.invoke<ArticleDto[]>("list_rss_articles", { feedId, limit }),
    addFeed: (url) => transport.invoke<FeedDto>("add_rss_feed", { url }),
    deleteFeed: (feedId) =>
      transport.invoke<void>("delete_rss_feed", { feedId }),
    markArticleRead: (articleId) =>
      transport.invoke<void>("mark_rss_article_read", { articleId }),
    fetchArticle: (url) =>
      transport.invoke<string>("fetch_article_url", { url }),
    refreshFeeds: () => transport.invoke<RefreshReport>("refresh_rss_feeds"),
    latestArticles: (limit) =>
      transport.invoke<ArticleDto[]>("latest_rss_articles", { limit }),
  };
}

export const rssClient = createRssClient();