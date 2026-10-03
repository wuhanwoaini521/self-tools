/**
 * 前端命令传输层（Gate 3 Boundary）。
 *
 * 唯一职责：把「命令名 + 参数 → Promise<T>」的调用与具体运行时隔离开。
 * 两个实现：桌面端 TauriTransport、网页端 httpTransport，由 defaultTransport
 * 自动选择 —— feature Client 不需要改动，也不需要自己判断运行环境。
 *
 * 注意：不要在这里引入 RPC 框架 / 事件总线 / 中间件 —— 保持薄接口。
 */
import { invoke as tauriInvoke, type InvokeArgs } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface CommandTransport {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
  /** 当前是否运行在 Tauri 桌面运行时（浏览器预览时返回 false）。 */
  isTauriRuntime(): boolean;
  /**
   * 订阅后端推送事件（V11：agent 进度）。返回取消订阅函数。
   * 浏览器预览（非 Tauri）下退化为 no-op。
   */
  subscribe<T>(event: string, handler: (payload: T) => void): () => void;
}

function inTauriRuntime(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

/** Tauri 实现：唯一职责是把调用转发给 `@tauri-apps/api/core` 的 invoke。 */
export const tauriTransport: CommandTransport = {
  invoke: (command, args) => {
    if (!inTauriRuntime()) {
      return Promise.reject(new Error("Not in Tauri runtime"));
    }
    return tauriInvoke(command, args as InvokeArgs | undefined);
  },
  isTauriRuntime: inTauriRuntime,
  subscribe: (event, handler) => {
    if (!inTauriRuntime()) return () => {};
    let unlisten: (() => void) | null = null;
    let cancelled = false;
    void listen(event, (received) => {
      if (!cancelled) handler(received.payload as never);
    }).then((dispose) => {
      if (cancelled) dispose();
      else unlisten = dispose;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  },
};
// ============================================================================
// HTTP 实现（浏览器 / 网页端）
// ============================================================================

/**
 * 已映射到只读 HTTP 服务的命令：键是 Tauri 命令名，值是把参数渲染成路径。
 *
 * 与 `apps/server` 的 `/api/v1/history/*` 一一对应，返回的是**同一批 Rust 结构体**
 * （`HistorySemanticHome` 等）——服务端与桌面端复用 `HistoryService`，所以网页端
 * 拿到的就是同一份数据、同一套字段。
 *
 * 只列真正有 HTTP 实现的命令。未列出的（geography / travel / language 等）会明确
 * 抛错而非静默返回空：静默会让页面显示「没有数据」，把配置问题伪装成数据缺失。
 */
const HTTP_ENDPOINTS: Record<string, (args: Record<string, unknown>) => string> = {
  history_semantic_home: () => "/api/v1/history/home",
  history_semantic_period: (a) =>
    `/api/v1/history/periods/${encodeURIComponent(String(a.periodId ?? ""))}`,
  history_semantic_story: (a) =>
    `/api/v1/history/stories/${encodeURIComponent(String(a.storyId ?? ""))}`,
  history_semantic_event: (a) =>
    `/api/v1/history/events/${encodeURIComponent(String(a.eventId ?? ""))}`,
  history_semantic_person: (a) =>
    `/api/v1/history/people/${encodeURIComponent(String(a.personId ?? ""))}`,
  history_semantic_work: (a) =>
    `/api/v1/history/works/${encodeURIComponent(String(a.workId ?? ""))}`,
  history_semantic_search: (a) =>
    `/api/v1/history/search?q=${encodeURIComponent(String(a.query ?? ""))}`,

  // ---- Language：与桌面端同一个 LanguageService / LanguageLearningService ----
  language_languages: () => "/api/v1/language/languages",
  language_sources: () => "/api/v1/language/sources",
  language_search: (a) =>
    `/api/v1/language/search?language=${encodeURIComponent(String(a.language ?? "jpn"))}` +
    `&q=${encodeURIComponent(String(a.query ?? ""))}` +
    `&limit=${encodeURIComponent(String(a.limit ?? 30))}`,
  language_item: (a) => `/api/v1/language/detail/${encodeURIComponent(String(a.id ?? ""))}`,
  language_sentences: (a) =>
    `/api/v1/language/sentences?language=${encodeURIComponent(String(a.language ?? "jpn"))}` +
    `&limit=${encodeURIComponent(String(a.limit ?? 20))}`,
  readiness_report: () => "/api/v1/readiness",
  readiness_diagnostics: () => "/api/v1/readiness/diagnostics",
  language_course_today: () => "/api/v1/language/course/today",
  language_course_books: () => "/api/v1/language/course/books",
  language_course_book: (a) =>
    `/api/v1/language/course/book/${encodeURIComponent(String(a.bookId ?? ""))}`,
  language_course_lesson: (a) =>
    `/api/v1/language/course/lesson/${encodeURIComponent(String(a.lessonId ?? ""))}`,
  language_course_progress: () => "/api/v1/language/course/progress",
  language_course_plan_get: () => "/api/v1/language/course/plan",
  language_course_search: (a) =>
    `/api/v1/language/course/search?q=${encodeURIComponent(String(a.query ?? ""))}` +
    `&limit=${encodeURIComponent(String(a.limit ?? 10))}`,
  language_learning_item: (a) =>
    `/api/v1/language/items/${encodeURIComponent(String(a.entityId ?? ""))}`,
  language_sentence_study: (a) =>
    `/api/v1/language/sentences/${encodeURIComponent(String(a.sentenceId ?? ""))}/study`,
  language_study_queue: (a) =>
    `/api/v1/language/study-queue?language=${encodeURIComponent(String(a.language ?? "jpn"))}` +
    `&limit=${encodeURIComponent(String(a.limit ?? 20))}`,
  language_review_queue: (a) =>
    `/api/v1/language/review-queue?limit=${encodeURIComponent(String(a.limit ?? 20))}`,
  language_mistakes: (a) =>
    `/api/v1/language/mistakes?limit=${encodeURIComponent(String(a.limit ?? 50))}`,
  language_progress: (a) =>
    `/api/v1/language/progress?limit=${encodeURIComponent(String(a.limit ?? 100))}`,
  language_weak_items: (a) =>
    `/api/v1/language/weak-items?limit=${encodeURIComponent(String(a.limit ?? 8))}`,
  language_lessons: (a) =>
    `/api/v1/language/lessons?language=${encodeURIComponent(String(a.language ?? "jpn"))}` +
    `&limit=${encodeURIComponent(String(a.limit ?? 20))}`,
  language_lesson: (a) =>
    `/api/v1/language/lessons/${encodeURIComponent(String(a.lessonId ?? ""))}`,
  language_continue_lessons: (a) =>
    `/api/v1/language/continue?limit=${encodeURIComponent(String(a.limit ?? 5))}`,
  // ---- 设置与 AI（与桌面端共用同一个 settings.json）----
  get_settings: () => "/api/v1/settings",
  personal_ai_status: () => "/api/v1/ai/status",

  // ---- Learning OS：Collections / Graph / Review Center / Home 今日面板 ----
  learning_list_progress: (a) =>
    `/api/v1/learning/progress?module=${encodeURIComponent(String(a.moduleFilter ?? ""))}` +
    `&status=${encodeURIComponent(String(a.statusFilter ?? ""))}` +
    `&limit=${encodeURIComponent(String(a.limit ?? 50))}`,
  learning_get_review_queue: (a) =>
    `/api/v1/learning/review/queue?module=${encodeURIComponent(String(a.moduleFilter ?? ""))}` +
    `&limit=${encodeURIComponent(String(a.limit ?? 30))}`,
  learning_get_review_stats: () => "/api/v1/learning/review/stats",
  learning_get_today: () => "/api/v1/learning/today",
  learning_get_graph: (a) =>
    `/api/v1/learning/graph?rootId=${encodeURIComponent(String(a.rootId ?? ""))}` +
    `&hops=${encodeURIComponent(String(a.hops ?? 1))}`,
  learning_get_explore: (a) =>
    `/api/v1/learning/explore?limit=${encodeURIComponent(String(a.limit ?? 6))}`,
  learning_list_collections: () => "/api/v1/learning/collections",
  learning_list_collection_items: (a) =>
    `/api/v1/learning/collections/${encodeURIComponent(String(a.collectionId))}/items`,
  // ---- News：推荐源目录在 core 里（纯函数），两端都读得到 ----
  news_recommended: () => "/api/v1/news/recommended",
  news_sources: () => "/api/v1/news/sources",
  news_starred: (a) =>
    `/api/v1/news/starred?limit=${encodeURIComponent(String(a.limit ?? 20))}`,
  news_search: (a) =>
    `/api/v1/news/search?q=${encodeURIComponent(String(a.query ?? ""))}` +
    `&limit=${encodeURIComponent(String(a.limit ?? 20))}`,
  news_headlines: (a) => {
    const scope = a.scope ?? "all";
    const parts = [`scope=${encodeURIComponent(String(scope))}`];
    if (a.sourceId !== undefined && a.sourceId !== null) {
      parts.push(`source_id=${encodeURIComponent(String(a.sourceId))}`);
    }
    if (a.category !== undefined && a.category !== null) {
      parts.push(`category=${encodeURIComponent(String(a.category))}`);
    }
    parts.push(`limit=${encodeURIComponent(String(a.limit ?? 30))}`);
    return `/api/v1/news/headlines?${parts.join("&")}`;
  },
  geography_home: (a) =>
    `/api/v1/geography/home?cursor=${encodeURIComponent(String(a.cursor ?? 0))}`,
  geography_search: (a) => {
    // 注意：`String(null)` 会得到字符串 "null"。前端用 null 表示「不筛选」，
    // 直接插值会发 entity_type=null，服务端解析成非法类型直接 400。
    const parts = [`query=${encodeURIComponent(String(a.query ?? ""))}`];
    if (a.entityType) {
      parts.push(`entity_type=${encodeURIComponent(String(a.entityType))}`);
    }
    parts.push(`limit=${encodeURIComponent(String(a.limit ?? 30))}`);
    return `/api/v1/geography/search?${parts.join("&")}`;
  },
  geography_detail: (a) =>
    `/api/v1/geography/entities/${encodeURIComponent(String(a.id ?? ""))}`,

};

/** 写操作：POST + JSON body。 */
const HTTP_POST_ENDPOINTS: Record<
  string,
  (args: Record<string, unknown>) => string
> = {
  language_record_study: () => "/api/v1/language/study",
  language_add_to_review: () => "/api/v1/language/add-to-review",
  language_submit_review: () => "/api/v1/language/review",
  language_create_lesson: () => "/api/v1/language/lessons",
  language_save_lesson_position: (a) =>
    `/api/v1/language/lessons/${encodeURIComponent(String(a.lessonId ?? ""))}/position`,
  save_settings: () => "/api/v1/settings",
  personal_ai_chat: () => "/api/v1/ai/chat",
  learning_record_event: () => "/api/v1/learning/progress",
  learning_create_collection: () => "/api/v1/learning/collections",
  learning_add_collection_item: (a) =>
    `/api/v1/learning/collections/${encodeURIComponent(String(a.collectionId))}/items`,
  learning_remove_collection_item: () => "/api/v1/learning/collection-items/remove",
  learning_submit_review: () => "/api/v1/learning/review",
  language_add_collection_item: (a) =>
    `/api/v1/learning/collections/${encodeURIComponent(String(a.collectionId))}/items`,
  news_add_source: () => "/api/v1/news/sources",
  news_toggle_star: (a) =>
    `/api/v1/news/articles/${encodeURIComponent(String(a.storyId))}/star`,
  news_mark_read: (a) =>
    `/api/v1/news/articles/${encodeURIComponent(String(a.storyId))}/read`,
  geography_toggle_favorite: () => "/api/v1/geography/favorite",
  news_remove_source: (a) =>
    `/api/v1/news/sources/${encodeURIComponent(String(a.sourceId))}/remove`,
};

/** 服务端错误体的可能形状（与 `apps/server` 的错误契约一致）。 */
function readErrorMessage(payload: unknown, fallback: string): string {
  if (typeof payload === "string" && payload.trim()) return payload;
  if (payload && typeof payload === "object") {
    const record = payload as Record<string, unknown>;
    for (const key of ["message", "error", "detail"]) {
      const value = record[key];
      if (typeof value === "string" && value.trim()) return value;
    }
  }
  return fallback;
}

/** HTTP 实现：把命令映射到本地只读服务的 REST 端点。 */
export const httpTransport: CommandTransport = {
  invoke: async <T,>(command: string, args: Record<string, unknown> = {}): Promise<T> => {
    const post = HTTP_POST_ENDPOINTS[command];
    const endpoint = HTTP_ENDPOINTS[command];
    if (!post && !endpoint) {
      throw new Error(`「${command}」尚无网页端接口，请启动桌面应用使用该功能。`);
    }
    const url = (post ?? endpoint)(args);
    let response: Response;
    try {
      response = post !== undefined
        ? await fetch(url, {
            method: "POST",
            headers: { "Content-Type": "application/json", Accept: "application/json" },
            body: JSON.stringify(args),
          })
        : await fetch(url, { headers: { Accept: "application/json" } });
    } catch (error) {
      throw new Error(
        `无法连接本地数据服务（${url}）：${
          error instanceof Error ? error.message : String(error)
        }`,
      );
    }
    // 与桌面端「查不到返回 null」保持一致，而不是报错。
    if (response.status === 404) return null as T;
    if (!response.ok) {
      let payload: unknown = null;
      let isJson = true;
      try {
        payload = await response.json();
      } catch {
        isJson = false;
      }
      // 非 JSON 错误体（开发期 Vite 代理在上游未启动时就是这样）几乎都意味着
      // 「服务没起来」，直接说 500 只会让人以为是数据坏了。
      if (!isJson) {
        throw new Error(
          `无法连接本地数据服务（${response.status}）——请确认它已启动。`,
        );
      }
      throw new Error(readErrorMessage(payload, `本地数据服务返回 ${response.status}`));
    }
    return (await response.json()) as T;
  },
  isTauriRuntime: () => false,
  subscribe: () => () => {},
};

/**
 * 自动选择传输层：有 Tauri 宿主走 IPC，否则走 HTTP。
 *
 * feature Client 只依赖这一个，桌面与网页共用同一份代码，无需各自判断运行环境。
 */
export const defaultTransport: CommandTransport = {
  invoke: <T,>(command: string, args?: Record<string, unknown>): Promise<T> =>
    inTauriRuntime()
      ? tauriTransport.invoke<T>(command, args)
      : httpTransport.invoke<T>(command, args),
  isTauriRuntime: inTauriRuntime,
  subscribe: (event, handler) =>
    inTauriRuntime()
      ? tauriTransport.subscribe(event, handler)
      : httpTransport.subscribe(event, handler),
};
