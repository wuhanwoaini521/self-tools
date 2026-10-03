/**
 * 英语课程（NCE）前端命令客户端。
 *
 * 每个方法对应 `apps/desktop/src/language_course.rs` 的一个真实命令，
 * 参数键使用顶层驼峰约定（Tauri v2 自动映射到 Rust 的 snake_case）。
 *
 * 原则：
 * - 后端返回什么就显示什么（不造数据、不在前端估算进度）；
 * - 命令失败只抛给调用方，由组件各自呈现（页面不整体白屏）；
 * - AI 能力不属于本客户端（走 `aiClient` / `onAskAi`）。
 */
import type { CommandTransport } from "../../../transport";
import { defaultTransport } from "../../../transport";
import type {
  BookView,
  CourseBook,
  DictImportReport,
  DictStatus,
  DictStatus as DictStatusType,
  EnglishProgress,
  EnglishSearchResult,
  LessonDetail,
  LessonProgress,
  LearningPlan,
  LearningProgress,
  NceImportReport,
  NceScanReport,
  ProgressPatch,
  QuizAnswer,
  QuizItem,
  QuizResult,
  TodayDashboard,
  WordEntry,
  WordLookup,
  WordMark,
} from "../../../types";

export interface EnglishClient {
  /** Today 驾驶舱。 */
  today(): Promise<TodayDashboard>;
  books(): Promise<CourseBook[]>;
  book(bookId: string): Promise<BookView | null>;
  lesson(lessonId: string): Promise<LessonDetail | null>;

  updateProgress(
    lessonId: string,
    patch: ProgressPatch,
  ): Promise<LessonProgress>;
  completeLesson(lessonId: string, quizScore: number | null): Promise<LessonProgress>;

  markWord(lessonId: string, word: string, mark: WordMark): Promise<LearningProgress>;
  lookupWord(
    word: string,
    sentence: string | null,
    lessonId: string | null,
  ): Promise<WordLookup>;
  dictLookup(word: string): Promise<WordEntry | null>;
  dictStatus(): Promise<DictStatusType>;

  quiz(lessonId: string): Promise<QuizItem[]>;
  submitQuiz(lessonId: string, answers: QuizAnswer[]): Promise<QuizResult>;

  planGet(): Promise<LearningPlan | null>;
  planSave(plan: LearningPlan): Promise<void>;
  progress(): Promise<EnglishProgress>;
  search(query: string, limit: number): Promise<EnglishSearchResult>;

  /** 读取课时音频二进制（返回 ArrayBuffer，前端转 Blob URL）。 */
  lessonAudio(lessonId: string): Promise<ArrayBuffer>;

  nceScan(sourceDir: string): Promise<NceScanReport>;
  nceImport(sourceDir: string): Promise<NceImportReport>;
  nceCancel(): Promise<void>;
  dictImport(path: string): Promise<DictImportReport>;
  dictCancel(): Promise<void>;
}

export function createEnglishClient(
  transport: CommandTransport = defaultTransport,
): EnglishClient {
  return {
    today: () => transport.invoke<TodayDashboard>("language_course_today"),
    books: () => transport.invoke<CourseBook[]>("language_course_books"),
    book: (bookId) => transport.invoke<BookView | null>("language_course_book", { bookId }),
    lesson: (lessonId) =>
      transport.invoke<LessonDetail | null>("language_course_lesson", { lessonId }),

    updateProgress: (lessonId, patch) =>
      transport.invoke<LessonProgress>("language_course_update_progress", {
        lessonId,
        patch,
      }),
    completeLesson: (lessonId, quizScore) =>
      transport.invoke<LessonProgress>("language_course_complete_lesson", {
        lessonId,
        quizScore,
      }),

    markWord: (lessonId, word, mark) =>
      transport.invoke<LearningProgress>("language_course_mark_word", {
        lessonId,
        word,
        mark,
      }),
    lookupWord: (word, sentence, lessonId) =>
      transport.invoke<WordLookup>("language_course_lookup_word", {
        word,
        sentence,
        lessonId,
      }),
    dictLookup: (word) => transport.invoke<WordEntry | null>("language_dict_lookup", { word }),
    dictStatus: () => transport.invoke<DictStatus>("language_dict_status"),

    quiz: (lessonId) => transport.invoke<QuizItem[]>("language_course_quiz", { lessonId }),
    submitQuiz: (lessonId, answers) =>
      transport.invoke<QuizResult>("language_course_submit_quiz", { lessonId, answers }),

    planGet: () => transport.invoke<LearningPlan | null>("language_course_plan_get"),
    planSave: (plan) => transport.invoke<void>("language_course_plan_save", { plan }),
    progress: () => transport.invoke<EnglishProgress>("language_course_progress"),
    search: (query, limit) =>
      transport.invoke<EnglishSearchResult>("language_course_search", { query, limit }),

    lessonAudio: async (lessonId) => {
      // Tauri 的二进制响应在不同版本里可能是 ArrayBuffer / TypedArray / number[]。
      const response = await transport.invoke<unknown>("language_lesson_audio", {
        lessonId,
      });
      if (response instanceof ArrayBuffer) return response;
      if (ArrayBuffer.isView(response)) {
        return response.buffer.slice(
          response.byteOffset,
          response.byteOffset + response.byteLength,
        ) as ArrayBuffer;
      }
      if (Array.isArray(response)) {
        return new Uint8Array(response as number[]).buffer;
      }
      throw new Error("unexpected audio response type");
    },

    nceScan: (sourceDir) => transport.invoke<NceScanReport>("language_nce_scan", { sourceDir }),
    nceImport: (sourceDir) =>
      transport.invoke<NceImportReport>("language_nce_import", { sourceDir }),
    nceCancel: () => transport.invoke<void>("language_nce_cancel"),
    dictImport: (path) => transport.invoke<DictImportReport>("language_dict_import", { path }),
    dictCancel: () => transport.invoke<void>("language_dict_cancel"),
  };
}

export const englishClient = createEnglishClient();