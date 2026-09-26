/**
 * News 模块的前端命令客户端（V12 / ADR-010）。
 *
 * 契约与后端 `news_*` 冻结一致。**只操作 News 自己的命令** ——
 * 订阅源增删走 `news_add_source` / `news_remove_source`（写 `news.db`），
 * **不调用** `add_rss_feed` / `set_rss_feed_kind`（RSS 与 News 是两个
 * bounded context，不存在互相转换）。
 *
 * 唯一共享的是 HTML 正文抓取（`fetch_article_url`）—— 属于共享基础设施，
 * 不属于任一领域的用例。
 */
import type { CommandTransport } from "../../transport";
import { tauriTransport } from "../../transport";
import type { NewsArticle, NewsOverview, NewsSource, RecommendedSource } from "../../types";

export interface NewsClient {
  /** 新闻源清单 + 健康态（含系统 seed 源）。 */
  sources(): Promise<NewsOverview>;
  /** 推荐源目录（**候选**，不在 news_sources 里的那些）。 */
  recommended(): Promise<RecommendedSource[]>;
  /**
   * 新闻流。
   * `scope` = all | starred | by_category | by_source；
   * `category` / `sourceId` 按 scope 取用。
   */
  headlines(
    scope: string | null,
    sourceId: number | null,
    category: string | null,
    limit: number,
  ): Promise<NewsArticle[]>;
  /** 关键词检索（读本地 news.db）。 */
  search(query: string, limit: number): Promise<NewsArticle[]>;
  /** 稍后读。 */
  starred(limit: number): Promise<NewsArticle[]>;
  /** 收藏 / 取消收藏（幂等开关）。 */
  toggleStar(articleId: number): Promise<boolean>;
  /** 标已读（幂等）。 */
  markRead(articleId: number): Promise<void>;
  /** 立即刷新全部新闻源（共享抓取骨架，落库写 news.db）。 */
  refreshNow(): Promise<{
    new_articles: number;
    failures: { source: string; message: string }[];
  }>;
  /** 添加新闻源（**写 news.db，不写 RSS 的 feeds**）。 */
  addSource(url: string, category: string): Promise<NewsSource>;
  /** 删除新闻源（文章级联删除）。 */
  removeSource(sourceId: number): Promise<void>;
  /** 改新闻源分类。 */
  setCategory(sourceId: number, category: string): Promise<void>;
  /** 抓文章页 HTML（共享基础设施；正文抽取在前端 `newsContent`）。 */
  fetchArticle(url: string): Promise<string>;
}

export function createNewsClient(
  transport: CommandTransport = tauriTransport,
): NewsClient {
  return {
    sources: () => transport.invoke<NewsOverview>("news_sources"),
    recommended: () =>
      transport.invoke<RecommendedSource[]>("news_recommended"),
    headlines: (scope, sourceId, category, limit) =>
      transport.invoke<NewsArticle[]>("news_headlines", {
        scope,
        sourceId,
        category,
        limit,
      }),
    search: (query, limit) =>
      transport.invoke<NewsArticle[]>("news_search", { query, limit }),
    starred: (limit) =>
      transport.invoke<NewsArticle[]>("news_starred", { limit }),
    toggleStar: (articleId) =>
      transport.invoke<boolean>("news_toggle_star", { storyId: articleId }),
    markRead: (articleId) =>
      transport.invoke<void>("news_mark_read", { storyId: articleId }),
    refreshNow: () =>
      transport.invoke<{
        new_articles: number;
        failures: { source: string; message: string }[];
      }>("news_refresh_now"),
    addSource: (url, category) =>
      transport.invoke<NewsSource>("news_add_source", { url, category }),
    removeSource: (sourceId) =>
      transport.invoke<void>("news_remove_source", { sourceId }),
    setCategory: (sourceId, category) =>
      transport.invoke<void>("news_set_category", { sourceId, category }),
    fetchArticle: (url) =>
      transport.invoke<string>("fetch_article_url", { url }),
  };
}

export const newsClient = createNewsClient();
