/**
 * Language 模块的前端命令客户端。
 *
 * 每个方法对应一个真实命令，参数键使用顶层驼峰约定
 * （`entityId` / `cardId` / `stepIndex` …）。
 *
 * 桌面端走 Tauri IPC，网页端走本地只读 HTTP 服务（同一批 Rust 结构体，
 * 见 `transport.ts` 的 `defaultTransport`），两端数据完全一致。
 *
 * 学习状态（掌握度 / 复习排期 / 事件流）**不在**这里：它由平台的
 * `learningClient` 提供，Language 只提供内容与「加入复习 / 答错记错题」的编排。
 */
import type { CommandTransport } from "../../transport";
import { defaultTransport } from "../../transport";
import type {
  LanguageCode,
  LanguageInfo,
  LanguageItem,
  LanguageLearningItem,
  LanguageSearchHit,
  Lesson,
  LessonView,
  ContinueLesson,
  Mistake,
  SentenceRecord,
  SentenceStudy,
  SourceInfo,
  StudyCard,
  SpeakingScore,
  StarterReport,
  WordDetail,
} from "../../types";
import type {
  LearningProgress,
  ReviewQueueItem,
  ReviewScheduleOutcome,
  UniversalReviewRating,
  WeakItem,
} from "../../types";

/** 学习行为分类（对应后端 `StudyAction`）。 */
export type StudyAction = "view" | "study" | "complete";

export interface CreateLessonInput {
  language: LanguageCode;
  title: string;
  itemIds: string[];
}

export interface LanguageClient {
  // ---- 词典数据 ----
  languages(): Promise<LanguageInfo[]>;
  search(
    language: LanguageCode | null,
    query: string,
    limit: number,
  ): Promise<LanguageSearchHit[]>;
  item(id: string): Promise<WordDetail | null>;
  sentences(language: LanguageCode, limit: number): Promise<SentenceRecord[]>;
  sources(): Promise<SourceInfo[]>;
  installStarter(): Promise<StarterReport>;
  speakingFeedback(
    target: string,
    transcript: string,
    durationMs: number,
    targetMs: number,
    longPausesMs: number[],
  ): Promise<SpeakingScore>;

  // ---- 学习条目 ----
  learningItem(entityId: string): Promise<LanguageLearningItem | null>;
  recordStudy(
    entityId: string,
    action: StudyAction,
  ): Promise<LearningProgress>;
  addToReview(entityId: string): Promise<void>;
  addToCollection(
    collectionId: string,
    entityId: string,
    note?: string,
  ): Promise<void>;

  // ---- 学习卡片队列 ----
  /** 先到期复习、再补新内容。 */
  studyQueue(language: LanguageCode, limit: number): Promise<StudyCard[]>;

  // ---- 复习（平台卡片） ----
  reviewQueue(limit: number): Promise<ReviewQueueItem[]>;
  submitReview(
    cardId: string,
    rating: UniversalReviewRating,
    userAnswer: string,
  ): Promise<ReviewScheduleOutcome>;

  // ---- 错题 ----
  mistakes(limit: number): Promise<Mistake[]>;

  // ---- Lesson ----
  createLesson(input: CreateLessonInput): Promise<Lesson>;
  lessons(language: LanguageCode | null, limit: number): Promise<Lesson[]>;
  lesson(lessonId: string): Promise<LessonView | null>;
  deleteLesson(lessonId: string): Promise<void>;
  saveLessonPosition(lessonId: string, stepIndex: number): Promise<void>;
  continueLessons(limit: number): Promise<ContinueLesson[]>;

  // ---- 句子 / 进度 ----
  sentenceStudy(sentenceId: string): Promise<SentenceStudy | null>;
  progress(limit: number): Promise<LearningProgress[]>;
  weakItems(limit: number): Promise<WeakItem[]>;
}

export function createLanguageClient(
  transport: CommandTransport = defaultTransport,
): LanguageClient {
  return {
    languages: () => transport.invoke<LanguageInfo[]>("language_languages"),
    search: (language, query, limit) =>
      transport.invoke<LanguageSearchHit[]>("language_search", {
        language,
        query,
        limit,
      }),
    item: (id) => transport.invoke<WordDetail | null>("language_item", { id }),
    sentences: (language, limit) =>
      transport.invoke<SentenceRecord[]>("language_sentences", {
        language,
        limit,
      }),
    sources: () => transport.invoke<SourceInfo[]>("language_sources"),
    installStarter: () =>
      transport.invoke<StarterReport>("language_install_starter", { only: null }),
    speakingFeedback: (target, transcript, durationMs, targetMs, longPausesMs) =>
      transport.invoke<SpeakingScore>("language_speaking_feedback", {
        request: { target, transcript, durationMs, targetMs, longPausesMs },
      }),

    learningItem: (entityId) =>
      transport.invoke<LanguageLearningItem | null>("language_learning_item", {
        entityId,
      }),
    recordStudy: (entityId, action) =>
      transport.invoke<LearningProgress>("language_record_study", {
        entityId,
        action,
      }),
    addToReview: (entityId) =>
      transport.invoke<void>("language_add_to_review", { entityId }),
    addToCollection: (collectionId, entityId, note) =>
      transport.invoke<void>("language_add_collection_item", {
        collectionId,
        entityId,
        note: note ?? null,
      }),

    studyQueue: (language, limit) =>
      transport.invoke<StudyCard[]>("language_study_queue", { language, limit }),

    reviewQueue: (limit) =>
      transport.invoke<ReviewQueueItem[]>("language_review_queue", { limit }),
    submitReview: (cardId, rating, userAnswer) =>
      transport.invoke<ReviewScheduleOutcome>("language_submit_review", {
        cardId,
        rating,
        userAnswer,
      }),

    mistakes: (limit) =>
      transport.invoke<Mistake[]>("language_mistakes", { limit }),

    createLesson: (input) =>
      transport.invoke<Lesson>("language_create_lesson", { request: input }),
    lessons: (language, limit) =>
      transport.invoke<Lesson[]>("language_lessons", { language, limit }),
    lesson: (lessonId) =>
      transport.invoke<LessonView | null>("language_lesson", { lessonId }),
    deleteLesson: (lessonId) =>
      transport.invoke<void>("language_delete_lesson", { lessonId }),
    saveLessonPosition: (lessonId, stepIndex) =>
      transport.invoke<void>("language_save_lesson_position", {
        lessonId,
        stepIndex,
      }),
    continueLessons: (limit) =>
      transport.invoke<ContinueLesson[]>("language_continue_lessons", { limit }),

    sentenceStudy: (sentenceId) =>
      transport.invoke<SentenceStudy | null>("language_sentence_study", {
        sentenceId,
      }),
    progress: (limit) =>
      transport.invoke<LearningProgress[]>("language_progress", { limit }),
    weakItems: (limit) =>
      transport.invoke<WeakItem[]>("language_weak_items", { limit }),
  };
}

export const languageClient = createLanguageClient();
