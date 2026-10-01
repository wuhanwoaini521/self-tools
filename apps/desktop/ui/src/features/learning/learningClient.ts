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
}

/**
 * 学习系统的命令客户端。
 *
 * 与其余 14 个 *Client 一致：**只经 `transport.invoke` 发命令**。
 * 非桌面运行时（浏览器预览）由 transport 统一 reject，调用方的 catch 负责
 * 呈现错误 / 空态。
 *
 * 这里曾为每个方法写一份 `isTauriRuntime() ? invoke(...) : Promise.resolve(<假数据>)`，
 * 其中 recordEvent / submitReview / createCollection / addCollectionItem /
 * removeCollectionItem / deleteCollection 会在根本没落库的情况下返回**伪造的成功**
 * （自造 id、自造 mastery、自造复习排期、删除也 resolve）。那会让 UI 显示
 * "已保存"，数据却不存在。假数据已全部移除。
 */
export function createLearningClient(
  transport: CommandTransport = tauriTransport
): LearningClient {
  return {
    recordEvent: (event) =>
      transport.invoke<LearningProgress>("learning_record_event", { event }),
    getProgress: (entityKey) =>
      transport.invoke<LearningProgress | null>("learning_get_progress", {
        entityKey,
      }),
    listProgress: (moduleFilter, statusFilter, limit) =>
      transport.invoke<LearningProgress[]>("learning_list_progress", {
        moduleFilter,
        statusFilter,
        limit,
      }),
    getToday: () =>
      transport.invoke<TodayDashboardData>("learning_get_today"),
    getReviewQueue: (moduleFilter, limit) =>
      transport.invoke<ReviewQueueItem[]>("learning_get_review_queue", {
        moduleFilter,
        limit,
      }),
    getReviewStats: () =>
      transport.invoke<ReviewQueueStats>("learning_get_review_stats"),
    submitReview: (cardId, rating) =>
      transport.invoke<ReviewScheduleOutcome>("learning_submit_review", {
        cardId,
        rating,
      }),
    getGraph: (rootId, hops) =>
      transport.invoke<GraphNeighborhood>("learning_get_graph", {
        rootId,
        hops,
      }),
    getExplore: (limit) =>
      transport.invoke<ExploreRecommendation[]>("learning_get_explore", {
        limit,
      }),
    listCollections: () =>
      transport.invoke<Collection[]>("learning_list_collections"),
    createCollection: (title, description, tags = []) =>
      transport.invoke<Collection>("learning_create_collection", {
        title,
        description,
        tags,
      }),
    addCollectionItem: (
      collectionId,
      module,
      entityType,
      entityId,
      title,
      note
    ) =>
      transport.invoke<CollectionItem>("learning_add_collection_item", {
        collectionId,
        module,
        entityType,
        entityId,
        title,
        note,
      }),
    listCollectionItems: (collectionId) =>
      transport.invoke<CollectionItem[]>("learning_list_collection_items", {
        collectionId,
      }),
    removeCollectionItem: (itemId) =>
      transport.invoke<void>("learning_remove_collection_item", { itemId }),
    deleteCollection: (collectionId) =>
      transport.invoke<void>("learning_delete_collection", { collectionId }),
  };
}

export const learningClient = createLearningClient();
