/**
 * Language 模块的前端命令客户端（Gate 6）。
 *
 * 每个方法对应一个真实 Tauri 命令（命令名 / 参数 / 返回类型与后端契约一致），
 * Language 各面板与外壳（App / Settings）只调用本模块，不直接持有 transport
 * 或 `@tauri-apps/api/core`。
 */
import type { CommandTransport } from "../../transport";
import { tauriTransport } from "../../transport";
import type {
  LanguageCode,
  LanguageInfo,
  LanguageItem,
  LanguageSearchHit,
  LearningStateKind,
  ProgressView,
  ReviewCard,
  ReviewOutcome,
  ReviewRating,
  SentenceRecord,
  SourceInfo,
  SpeakingScore,
  StarterReport,
  TodayView,
  WordDetail,
} from "../../types";

export interface LanguageClient {
  languages(): Promise<LanguageInfo[]>;
  today(language: LanguageCode): Promise<TodayView>;
  search(
    language: LanguageCode,
    query: string,
    limit: number,
  ): Promise<LanguageSearchHit[]>;
  sentences(language: LanguageCode, limit: number): Promise<SentenceRecord[]>;
  item(id: string): Promise<WordDetail | null>;
  toggleFavorite(itemId: string): Promise<boolean>;
  setState(itemId: string, state: LearningStateKind): Promise<void>;
  reviewNext(language: LanguageCode): Promise<ReviewCard | null>;
  reviewRate(itemId: string, rating: ReviewRating): Promise<ReviewOutcome>;
  favorites(limit: number): Promise<LanguageItem[]>;
  progress(): Promise<ProgressView>;
  sources(): Promise<SourceInfo[]>;
  installStarter(): Promise<StarterReport>;
  speakingFeedback(
    target: string,
    transcript: string,
    durationMs: number,
    targetMs: number,
    longPausesMs: number[],
  ): Promise<SpeakingScore>;
}

export function createLanguageClient(
  transport: CommandTransport = tauriTransport,
): LanguageClient {
  return {
    languages: () => transport.invoke<LanguageInfo[]>("language_languages"),
    today: (language) =>
      transport.invoke<TodayView>("language_today", { language }),
    search: (language, query, limit) =>
      transport.invoke<LanguageSearchHit[]>("language_search", {
        language,
        query,
        limit,
      }),
    sentences: (language, limit) =>
      transport.invoke<SentenceRecord[]>("language_sentences", {
        language,
        limit,
      }),
    item: (id) => transport.invoke<WordDetail | null>("language_item", { id }),
    toggleFavorite: (itemId) =>
      transport.invoke<boolean>("language_toggle_favorite", { itemId }),
    setState: (itemId, state) =>
      transport.invoke<void>("language_set_state", {
        request: { itemId, state },
      }),
    reviewNext: (language) =>
      transport.invoke<ReviewCard | null>("language_review_next", {
        language,
      }),
    reviewRate: (itemId, rating) =>
      transport.invoke<ReviewOutcome>("language_review_rate", {
        request: { itemId, rating },
      }),
    favorites: (limit) =>
      transport.invoke<LanguageItem[]>("language_favorites", { limit }),
    progress: () => transport.invoke<ProgressView>("language_progress"),
    sources: () => transport.invoke<SourceInfo[]>("language_sources"),
    installStarter: () =>
      transport.invoke<StarterReport>("language_install_starter", {
        only: null,
      }),
    speakingFeedback: (target, transcript, durationMs, targetMs, longPausesMs) =>
      transport.invoke<SpeakingScore>("language_speaking_feedback", {
        request: { target, transcript, durationMs, targetMs, longPausesMs },
      }),
  };
}

export const languageClient = createLanguageClient();