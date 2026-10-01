/**
 * Learning OS 统一学习前端客户端 (V11)。
 *
 * 封装与 Tauri 后端的交互，处理：
 * - 学习事件记录与进度查询
 * - 今日学习看板数据聚合
 * - 复习中心队列与打分
 * - 知识图谱邻域查询
 * - 专题合集 CRUD
 * - 全局跨模块 ⌘K 搜索
 */
import type { CommandTransport } from "../../transport";
import { tauriTransport } from "../../transport";
import type {
  Collection,
  CollectionItem,
  ExploreRecommendation,
  GlobalSearchResultGroup,
  GraphNeighborhood,
  LearningEvent,
  LearningProgress,
  LearningStatus,
  ReviewQueueItem,
  ReviewQueueStats,
  ReviewScheduleOutcome,
  TodayDashboardData,
  UniversalReviewRating,
} from "../../types";

export interface LearningClient {
  recordEvent(event: LearningEvent): Promise<LearningProgress>;
  getProgress(entityKey: string): Promise<LearningProgress | null>;
  listProgress(
    moduleFilter?: string,
    statusFilter?: LearningStatus,
    limit?: number
  ): Promise<LearningProgress[]>;
  getToday(): Promise<TodayDashboardData>;
  getReviewQueue(moduleFilter?: string, limit?: number): Promise<ReviewQueueItem[]>;
  getReviewStats(): Promise<ReviewQueueStats>;
  submitReview(cardId: string, rating: UniversalReviewRating): Promise<ReviewScheduleOutcome>;
  getGraph(rootId?: string, hops?: number): Promise<GraphNeighborhood>;
  getExplore(limit?: number): Promise<ExploreRecommendation[]>;
  listCollections(): Promise<Collection[]>;
  createCollection(
    title: string,
    description?: string,
    tags?: string[]
  ): Promise<Collection>;
  addCollectionItem(
    collectionId: string,
    module: string,
    entityType: string,
    entityId: string,
    title: string,
    note?: string
  ): Promise<CollectionItem>;
  listCollectionItems(collectionId: string): Promise<CollectionItem[]>;
  removeCollectionItem(itemId: string): Promise<void>;
  deleteCollection(collectionId: string): Promise<void>;
  globalSearch(query: string): Promise<GlobalSearchResultGroup[]>;
}

export function createLearningClient(
  transport: CommandTransport = tauriTransport
): LearningClient {
  const isTauri = () => transport.isTauriRuntime();

  return {
    recordEvent: (event) =>
      isTauri()
        ? transport.invoke<LearningProgress>("learning_record_event", { event })
        : Promise.resolve({
            entity_key: `${event.module}:${event.entity_id}`,
            module: event.module,
            entity_type: event.entity_type,
            entity_id: event.entity_id,
            title: event.title,
            status: "learning",
            mastery_score: 10,
            study_count: 1,
            review_count: 0,
            correct_streak: 1,
            last_action: event.action,
            last_studied_at: Math.floor(Date.now() / 1000),
            next_review_at: null,
            created_at: Math.floor(Date.now() / 1000),
            updated_at: Math.floor(Date.now() / 1000),
          }),
    getProgress: (entityKey) =>
      isTauri()
        ? transport.invoke<LearningProgress | null>("learning_get_progress", {
            entityKey,
          })
        : Promise.resolve(null),
    listProgress: (moduleFilter, statusFilter, limit) =>
      isTauri()
        ? transport.invoke<LearningProgress[]>("learning_list_progress", {
            moduleFilter,
            statusFilter,
            limit,
          })
        : Promise.resolve([]),
    getToday: () =>
      isTauri()
        ? transport.invoke<TodayDashboardData>("learning_get_today")
        : Promise.resolve({
            date_str: new Date().toISOString().slice(0, 10),
            greeting: "你好，开启今天的知识探索",
            studied_topics_today: 0,
            pending_reviews_count: 0,
            average_mastery: 0,
            recent_streak_days: 0,
            continue_items: [],
            review_stats: {
              due_count: 0,
              total_cards: 0,
              by_module: {},
              mastered_count: 0,
              learning_count: 0,
            },
            explore_recommendations: [],
            today_news_summary: null,
            recent_collections: [],
            recent_bookmarks: [],
          }),
    getReviewQueue: (moduleFilter, limit) =>
      isTauri()
        ? transport.invoke<ReviewQueueItem[]>("learning_get_review_queue", {
            moduleFilter,
            limit,
          })
        : Promise.resolve([]),
    getReviewStats: () =>
      isTauri()
        ? transport.invoke<ReviewQueueStats>("learning_get_review_stats")
        : Promise.resolve({
            due_count: 0,
            total_cards: 0,
            by_module: {},
            mastered_count: 0,
            learning_count: 0,
          }),
    submitReview: (cardId, rating) =>
      isTauri()
        ? transport.invoke<ReviewScheduleOutcome>("learning_submit_review", {
            cardId,
            rating,
          })
        : Promise.resolve({
            card_id: cardId,
            new_state: "reviewing",
            interval_days: 1,
            ease_factor: 2.5,
            due_at: Math.floor(Date.now() / 1000) + 86400,
            lapses: 0,
            mastery_delta: 5,
          }),
    getGraph: (rootId, hops) =>
      isTauri()
        ? transport.invoke<GraphNeighborhood>("learning_get_graph", {
            rootId,
            hops,
          })
        : Promise.resolve({
            center: {
              id: rootId ?? "root",
              name: "知识中心",
              entity_type: "topic",
              module: "all",
            },
            nodes: [],
            edges: [],
            total_nodes: 0,
            total_edges: 0,
          }),
    getExplore: (limit) =>
      isTauri()
        ? transport.invoke<ExploreRecommendation[]>("learning_get_explore", {
            limit,
          })
        : Promise.resolve([]),
    listCollections: () =>
      isTauri()
        ? transport.invoke<Collection[]>("learning_list_collections")
        : Promise.resolve([]),
    createCollection: (title, description, tags = []) =>
      isTauri()
        ? transport.invoke<Collection>("learning_create_collection", {
            title,
            description,
            tags,
          })
        : Promise.resolve({
            id: "col-" + Date.now(),
            title,
            description: description ?? null,
            tags,
            items_count: 0,
            created_at: Math.floor(Date.now() / 1000),
            updated_at: Math.floor(Date.now() / 1000),
          }),
    addCollectionItem: (
      collectionId,
      module,
      entityType,
      entityId,
      title,
      note
    ) =>
      isTauri()
        ? transport.invoke<CollectionItem>("learning_add_collection_item", {
            collectionId,
            module,
            entityType,
            entityId,
            title,
            note,
          })
        : Promise.resolve({
            id: "item-" + Date.now(),
            collection_id: collectionId,
            module,
            entity_type: entityType,
            entity_id: entityId,
            title,
            note: note ?? null,
            created_at: Math.floor(Date.now() / 1000),
          }),
    listCollectionItems: (collectionId) =>
      isTauri()
        ? transport.invoke<CollectionItem[]>("learning_list_collection_items", {
            collectionId,
          })
        : Promise.resolve([]),
    removeCollectionItem: (itemId) =>
      isTauri()
        ? transport.invoke<void>("learning_remove_collection_item", { itemId })
        : Promise.resolve(),
    deleteCollection: (collectionId) =>
      isTauri()
        ? transport.invoke<void>("learning_delete_collection", { collectionId })
        : Promise.resolve(),
    globalSearch: (query) =>
      isTauri()
        ? transport.invoke<GlobalSearchResultGroup[]>("learning_global_search", {
            query,
          })
        : Promise.resolve([]),
  };
}

export const learningClient = createLearningClient();
