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
import type { MinedCard, MinedReport } from "../miningTypes";
import type { ShadowScoreInput, ShadowScoreResult, ShadowStats } from "../speakingTypes";
import type {
  BookView,
  CourseBook,
  DataStatus,
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
  /** 学习资料现状（教材/词典在哪、缺什么）。 */
  dataStatus(): Promise<DataStatus>;

  quiz(lessonId: string): Promise<QuizItem[]>;
  submitQuiz(lessonId: string, answers: QuizAnswer[]): Promise<QuizResult>;

  planGet(): Promise<LearningPlan | null>;
  planSave(plan: LearningPlan): Promise<void>;
  progress(): Promise<EnglishProgress>;
  search(query: string, limit: number): Promise<EnglishSearchResult>;

  /** 读取课时音频二进制（返回 ArrayBuffer，前端转 Blob URL）。 */
  lessonAudio(lessonId: string): Promise<ArrayBuffer>;

  /**
   * 句子挖掘预览（V13 W3）：本课能挖出哪些复习卡（不写库）。
   * 界面据此告诉用户「本课可挖 N 张」，而不是让用户凭空点一个按钮。
   */
  miningPreview(lessonId: string, maxPerKind?: number): Promise<MinedCard[]>;
  /** 句子挖掘入库：把卡片写进同一套 SRS（幂等）。 */
  miningAdd(lessonId: string, maxPerKind?: number): Promise<MinedReport>;

  /**
   * 跟读发音评分（V13 W2）。
   * `transcript` 必须来自**真实的语音识别结果**；拿不到就不要调用
   * （服务端也会拒绝空转写，不会编造分数）。
   */
  shadowScore(input: ShadowScoreInput): Promise<ShadowScoreResult>;
  /** 跟读统计（开口时长 / 平均准确率；`since` 为 Unix 秒，0 = 不限）。 */
  shadowStats(lessonId?: string | null, since?: number): Promise<ShadowStats>;

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
    dataStatus: () => transport.invoke<DataStatus>("language_data_status"),

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

    miningPreview: async (lessonId, maxPerKind) => {
      const raw = await transport.invoke<{ items?: MinedCard[] } | MinedCard[]>(
        "language_mining_preview",
        { lessonId, maxPerKind },
      );
      return Array.isArray(raw) ? raw : (raw?.items ?? []);
    },
    miningAdd: (lessonId, maxPerKind) =>
      transport.invoke<MinedReport>("language_mining_add", { lessonId, maxPerKind }),

    shadowScore: (input) =>
      transport.invoke<ShadowScoreResult>("language_shadow_score", {
        lessonId: input.lessonId,
        sentenceSeq: input.sentenceSeq,
        transcript: input.transcript,
        durationMs: input.durationMs,
        targetMs: input.targetMs ?? 0,
        longPausesMs: input.longPausesMs ?? [],
      }),
    shadowStats: (lessonId, since) =>
      transport.invoke<ShadowStats>("language_shadow_stats", { lessonId, since }),

    nceScan: (sourceDir) => transport.invoke<NceScanReport>("language_nce_scan", { sourceDir }),    nceImport: (sourceDir) =>
      transport.invoke<NceImportReport>("language_nce_import", { sourceDir }),
    nceCancel: () => transport.invoke<void>("language_nce_cancel"),
    dictImport: (path) => transport.invoke<DictImportReport>("language_dict_import", { path }),
    dictCancel: () => transport.invoke<void>("language_dict_cancel"),
  };
}

export const englishClient = createEnglishClient();