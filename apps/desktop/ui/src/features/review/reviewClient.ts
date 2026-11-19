import type { CommandTransport } from "../../transport";
import { defaultTransport } from "../../transport";
import type {
  LearningProgress,
  ReviewQueueItem,
  ReviewQueueStats,
  ReviewScheduleOutcome,
  UniversalReviewRating,
} from "../../types";

/** Review module API facade. Transport command names remain an adapter detail. */
export interface ReviewClient {
  getQueue(moduleFilter?: string, limit?: number): Promise<ReviewQueueItem[]>;
  getStats(): Promise<ReviewQueueStats>;
  submit(cardId: string, rating: UniversalReviewRating, requestId?: string): Promise<ReviewScheduleOutcome>;
  getProgress(entityKey: string): Promise<LearningProgress | null>;
}

export function createReviewClient(
  transport: CommandTransport = defaultTransport,
): ReviewClient {
  return {
    getQueue: (moduleFilter, limit) =>
      transport.invoke<ReviewQueueItem[]>("learning_get_review_queue", {
        moduleFilter,
        limit,
      }),
    getStats: () =>
      transport.invoke<ReviewQueueStats>("learning_get_review_stats"),
    submit: (cardId, rating, requestId) =>
      transport.invoke<ReviewScheduleOutcome>("learning_submit_review", {
        cardId,
        rating,
        ...(requestId ? { requestId } : {}),
      }),
    getProgress: (entityKey) =>
      transport.invoke<LearningProgress | null>("learning_get_progress", {
        entityKey,
      }),
  };
}

export const reviewClient = createReviewClient();
