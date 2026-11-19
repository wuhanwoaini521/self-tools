import type {
  Collection,
  CollectionItem,
  NewsArticle,
  TodayDashboardData,
  TravelPlan,
} from "../../types";
import { learningClient } from "../learning/learningClient";
import { newsClient } from "../news/newsClient";
import { travelClient } from "../travel/travelClient";
import { stripRssHtml } from "../news/newsContent";

export type SavedReference = { collection: Collection; item: CollectionItem };
export type HomeHeadline = Pick<NewsArticle, "id" | "source" | "published_at" | "is_read" | "title"> & {
  summary: string | null;
};

/** Home's read-only aggregation facade over the public module clients. */
export interface HomeClient {
  today(): Promise<TodayDashboardData>;
  savedReferences(): Promise<SavedReference[]>;
  recentTravelPlans(limit?: number): Promise<TravelPlan[]>;
  headlines(limit?: number): Promise<HomeHeadline[]>;
}

export function createHomeClient(): HomeClient {
  return {
    today: () => learningClient.getToday(),
    savedReferences: async () => {
      const collections = await learningClient.listCollections();
      const recent = [...collections]
        .sort((left, right) => right.updated_at - left.updated_at)
        .slice(0, 5);
      const groups = await Promise.all(
        recent.map(async (collection) => ({
          collection,
          items: await learningClient.listCollectionItems(collection.id),
        })),
      );
      return groups
        .flatMap(({ collection, items }) => items.map((item) => ({ collection, item })))
        .sort((left, right) => right.item.added_at - left.item.added_at)
        .slice(0, 5);
    },
    recentTravelPlans: (limit = 5) => travelClient.recentPlans(limit),
    headlines: async (limit = 4) => {
      const articles = await newsClient.headlines("all", null, null, limit);
      return articles.map((article) => ({
        id: article.id,
        source: article.source,
        published_at: article.published_at,
        is_read: article.is_read,
        title: article.title,
        summary: article.summary ? stripRssHtml(article.summary, article.url) : null,
      }));
    },
  };
}

export const homeClient = createHomeClient();
