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

export interface SubscribeOptions {
  /**
   * HTTP 传输用它定位具体运行的 SSE 流（`/api/v1/ai/runs/{traceId}/events`）。
   * Tauri 事件本身已带 `trace_id`，无需此参数。
   */
  traceId?: string;
}

export interface CommandTransport {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
  /** 当前是否运行在 Tauri 桌面运行时（浏览器预览时返回 false）。 */
  isTauriRuntime(): boolean;
  /**
   * 订阅后端推送事件（V11：agent 进度）。返回取消订阅函数。
   * Tauri 走 `listen`；网页端对 `agent-progress` 走 SSE，其余事件为 no-op。
   */
  subscribe<T>(
    event: string,
    handler: (payload: T) => void,
    options?: SubscribeOptions,
  ): () => void;
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
  // History AI 解读：与桌面端共用同一个 EnrichmentRunnerPort。
  history_enrichment_state: (a) =>
    `/api/v1/history/enrichment/state?entityType=${encodeURIComponent(String(a.entityType ?? "event"))}` +
    `&entityId=${encodeURIComponent(String(a.entityId ?? ""))}` +
    `&locale=${encodeURIComponent(String(a.locale ?? ""))}`,
  history_enrichment_view: (a) =>
    `/api/v1/history/enrichment/view?entityType=${encodeURIComponent(String(a.entityType ?? "event"))}` +
    `&entityId=${encodeURIComponent(String(a.entityId ?? ""))}` +
    `&section=${encodeURIComponent(String(a.section ?? "overview"))}` +
    `&locale=${encodeURIComponent(String(a.locale ?? ""))}`,

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
  // Personal Knowledge（两端同一份索引库）：此前网页端全部缺接口 → 只能显示「不支持」。
  memory_list: (a) =>
    `/api/v1/memory/list?query=${encodeURIComponent(String(a.query ?? ""))}` +
    `&category=${encodeURIComponent(String(a.category ?? ""))}` +
    `&status=${encodeURIComponent(String(a.status ?? ""))}` +
    `&limit=${encodeURIComponent(String(a.limit ?? 100))}`,
  memory_stats: () => "/api/v1/memory/stats",
  documents_status: () => "/api/v1/documents/status",
  documents_recent: (a) => `/api/v1/documents/recent?limit=${encodeURIComponent(String(a.limit ?? 20))}`,
  documents_search: (a) =>
    `/api/v1/documents/search?query=${encodeURIComponent(String(a.query ?? ""))}` +
    `&limit=${encodeURIComponent(String(a.limit ?? 20))}`,
  files_status: () => "/api/v1/files/status",
  // 单条读取：与桌面端同名命令同义（文档 id / 记忆 id 由后端校验存在性）。
  memory_get: (a) => `/api/v1/memory/${encodeURIComponent(String(a.id ?? ""))}`,
  documents_get: (a) =>
    `/api/v1/documents/${encodeURIComponent(String(a.documentId ?? ""))}`,
  knowledge_status: () => "/api/v1/knowledge/status",
  files_recent: (a) => `/api/v1/files/recent?limit=${encodeURIComponent(String(a.limit ?? 20))}`,
  files_search: (a) =>
    `/api/v1/files/search?query=${encodeURIComponent(String(a.query ?? ""))}` +
    `&limit=${encodeURIComponent(String(a.limit ?? 20))}`,
  global_search: (a) =>
    `/api/v1/search/global?query=${encodeURIComponent(String(a.query ?? ""))}` +
    `&limit=${encodeURIComponent(String(a.limit ?? 8))}`,
  // RSS：与桌面端共用 dashboard.db，网关登录后网页/平板可订阅与阅读。
  list_rss_feeds: () => "/api/v1/rss/feeds",
  list_rss_articles: (a) =>
    `/api/v1/rss/feeds/${encodeURIComponent(String(a.feedId ?? ""))}/articles` +
    `?limit=${encodeURIComponent(String(a.limit ?? 200))}`,
  latest_rss_articles: (a) =>
    `/api/v1/rss/articles/latest?limit=${encodeURIComponent(String(a.limit ?? 5))}`,
  readiness_report: () => "/api/v1/readiness",
  // 语言数据导入（网页端也要能导入，别逼人切到桌面应用）
  language_data_status: () => "/api/v1/language/data-status",
  language_dict_status: () => "/api/v1/language/dict/status",
  language_import_status: () => "/api/v1/language/import-status",
  readiness_diagnostics: () => "/api/v1/readiness/diagnostics",
  language_course_today: () => "/api/v1/language/course/today",
  language_course_books: () => "/api/v1/language/course/books",
  language_course_book: (a) =>
    `/api/v1/language/course/book/${encodeURIComponent(String(a.bookId ?? ""))}`,
  language_course_lesson: (a) =>
    `/api/v1/language/course/lesson/${encodeURIComponent(String(a.lessonId ?? ""))}`,
  language_course_progress: () => "/api/v1/language/course/progress",
  language_course_plan_get: () => "/api/v1/language/course/plan",
  language_course_quiz: (a) =>
    `/api/v1/language/course/quiz?lesson_id=${encodeURIComponent(String(a.lessonId ?? ""))}`,
  language_dict_lookup: (a) =>
    `/api/v1/language/course/dict/${encodeURIComponent(String(a.word ?? ""))}`,
  // Travel：provider 装配已下沉，网页端与桌面端共用同一套 + 同一份 travel.db。
  travel_research_progress: (a) =>
    `/api/v1/travel/research/progress?session_id=${encodeURIComponent(String(a.sessionId ?? ""))}`,
  travel_recent_guides: () => "/api/v1/travel/recent-guides",
  travel_plan_recent: (a) =>
    `/api/v1/travel/plans/recent?limit=${encodeURIComponent(String(a.limit ?? 20))}`,
  travel_load_guide: (a) =>
    `/api/v1/travel/guide?city=${encodeURIComponent(String(a.city ?? ""))}` +
    `&days=${encodeURIComponent(String(a.days ?? 3))}`,
  travel_plan_get: (a) => {
    const context = (a.context ?? {}) as Record<string, unknown>;
    const range = context.date_range as Record<string, unknown> | null | undefined;
    const query = [
      `city=${encodeURIComponent(String(context.city ?? ""))}`,
      `days=${encodeURIComponent(String(context.days ?? 0))}`,
    ];
    if (range) {
      query.push(`date_start=${encodeURIComponent(String(range.start ?? ""))}`);
      query.push(`date_end=${encodeURIComponent(String(range.end ?? ""))}`);
    }
    return `/api/v1/travel/plans?${query.join("&")}`;
  },
  language_lesson_audio: (a) =>
    `/api/v1/language/course/lesson/${encodeURIComponent(String(a.lessonId ?? ""))}/audio`,
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
  // 进度：状态快照 + SSE 事件流（seq 与桌面 Tauri 事件一致）。
  ai_run_status: (a) =>
    `/api/v1/ai/runs/${encodeURIComponent(String(a.traceId ?? ""))}`,
  ai_run_events: (a) =>
    `/api/v1/ai/runs/${encodeURIComponent(String(a.traceId ?? ""))}/events`,
  // ---- 会话历史（V11-K）：与桌面端同一份 conversations.db ----
  conversation_list: (a) =>
    `/api/v1/conversations?limit=${encodeURIComponent(String(a.limit ?? 20))}` +
    `&includeArchived=${encodeURIComponent(String(a.includeArchived ?? false))}`,
  conversation_load: (a) =>
    `/api/v1/conversations/${encodeURIComponent(String(a.id ?? ""))}`,

  // ---- Learning OS：Collections / Graph / Review Center / Home 今日面板 ----
  learning_list_progress: (a) =>
    `/api/v1/learning/progress?module=${encodeURIComponent(String(a.moduleFilter ?? ""))}` +
    `&status=${encodeURIComponent(String(a.statusFilter ?? ""))}` +
    `&limit=${encodeURIComponent(String(a.limit ?? 50))}`,
  learning_get_progress: (a) =>
    `/api/v1/learning/progress/key?entityKey=${encodeURIComponent(String(a.entityKey ?? ""))}`,
  learning_get_review_queue: (a) =>
    `/api/v1/learning/review/queue?module=${encodeURIComponent(String(a.moduleFilter ?? ""))}` +
    `&limit=${encodeURIComponent(String(a.limit ?? 30))}`,
  learning_get_review_stats: () => "/api/v1/learning/review/stats",
  learning_get_review_card: (a) =>
    `/api/v1/learning/review/cards/${encodeURIComponent(String(a.cardId ?? ""))}`,
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
  exploration_location_history: (a) =>
    `/api/v1/exploration/geography/${encodeURIComponent(String(a.id ?? ""))}/history`,
  exploration_geography_for_history_place: (a) =>
    `/api/v1/exploration/history-places/${encodeURIComponent(String(a.placeId ?? ""))}/geography`,

  // ---- 学习板：同一份用例、同一份 study_boards.db（此前只写 localStorage）----
  study_board_list: (a) =>
    `/api/v1/study-boards?limit=${encodeURIComponent(String(a.limit ?? 20))}`,
  study_board_get: (a) =>
    `/api/v1/study-boards/${encodeURIComponent(String(a.boardId ?? ""))}`,
  // 26 周能力路线图（V13 W6）：静态计划 + 真实统计。
  english_roadmap: () => "/api/v1/english/roadmap",
  // 句子挖掘预览（V13 W3）：本课能挖出哪些卡（只读，不写库）。
  language_mining_preview: (a) =>
    `/api/v1/language/mining/lesson/${encodeURIComponent(String(a.lessonId ?? ""))}` +
    `?maxPerKind=${encodeURIComponent(String(a.maxPerKind ?? 6))}`,
  // 跟读统计（V13 W2）：进度页的「开口时长 / 平均准确率」来自这里。
  language_shadow_stats: (a) => {
    const parts: string[] = [];
    if (a.lessonId) parts.push(`lessonId=${encodeURIComponent(String(a.lessonId))}`);
    if (a.since) parts.push(`since=${encodeURIComponent(String(a.since))}`);
    return `/api/v1/language/shadow/stats${parts.length ? `?${parts.join("&")}` : ""}`;
  },
  language_speaking_capabilities: () => "/api/v1/language/speaking/capabilities",
  language_speaking_scenarios: () => "/api/v1/language/speaking/scenarios",
  language_speaking_scenario: (a) =>
    `/api/v1/language/speaking/scenarios/${encodeURIComponent(String(a.scenarioId ?? ""))}`,
  language_speaking_session: (a) =>
    `/api/v1/language/speaking/sessions/${encodeURIComponent(String(a.sessionId ?? ""))}`,
  language_speaking_report: (a) =>
    `/api/v1/language/speaking/sessions/${encodeURIComponent(String(a.sessionId ?? ""))}/report`,
  // ---- Home Server（Phase 1.4）：读开放；写端点由网关 admin 标记授权 ----
  server_status: () => "/api/v1/server/status",
  server_services_list: () => "/api/v1/server/services",
  server_apps_list: () => "/api/v1/server/apps",
  server_actions_recent: (a) =>
    `/api/v1/server/actions/recent?limit=${encodeURIComponent(String(a.limit ?? 20))}`,
  services_get_logs: (a) => {
    const parts: string[] = [];
    const push = (name: string, value: unknown) => {
      if (value === undefined || value === null || value === "") return;
      parts.push(`${name}=${encodeURIComponent(String(value))}`);
    };
    push("serviceId", a.serviceId);
    push("logSourceId", a.logSourceId);
    push("maxLines", a.maxLines);
    push("maxBytes", a.maxBytes);
    push("maxAgeSecs", a.maxAgeSecs);
    return `/api/v1/server/logs${parts.length ? `?${parts.join("&")}` : ""}`;
  },
  mcp_status: () => "/api/v1/server/mcp",
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
  personal_ai_cancel_run: (a) =>
    `/api/v1/ai/runs/${encodeURIComponent(String(a.traceId ?? a.trace_id ?? ""))}/cancel`,
  conversation_create: () => "/api/v1/conversations",
  conversation_append: (a) =>
    `/api/v1/conversations/${encodeURIComponent(String(a.id ?? ""))}/messages`,
  personal_ai_confirm_travel_change: (a) =>
    `/api/v1/ai/travel-changes/${encodeURIComponent(String(a.proposalId ?? a.proposal_id ?? ""))}/confirm`,
  personal_ai_cancel_travel_change: (a) =>
    `/api/v1/ai/travel-changes/${encodeURIComponent(String(a.proposalId ?? a.proposal_id ?? ""))}/cancel`,
  learning_record_event: () => "/api/v1/learning/progress",
  travel_research_start: () => "/api/v1/travel/research/start",
  travel_plan_save: () => "/api/v1/travel/plans",
  language_nce_scan: () => "/api/v1/language/nce/scan",
  language_nce_import: () => "/api/v1/language/nce/import",
  language_dict_import: () => "/api/v1/language/dict/import",
  test_travel_llm: () => "/api/v1/travel/test/llm",
  test_travel_amap: () => "/api/v1/travel/test/amap",
  test_travel_qweather: () => "/api/v1/travel/test/qweather",
  // 英语课程写路径：网页端要能真正「学」，不只是「看」。
  // 此前这些只有桌面端有，点「不认识」直接报「尚无网页端接口」。
  language_course_update_progress: (a) =>
    `/api/v1/language/course/lesson/${encodeURIComponent(String(a.lessonId ?? ""))}/progress`,
  language_course_complete_lesson: (a) =>
    `/api/v1/language/course/lesson/${encodeURIComponent(String(a.lessonId ?? ""))}/complete`,
  language_course_mark_word: (a) =>
    `/api/v1/language/course/lesson/${encodeURIComponent(String(a.lessonId ?? ""))}/mark-word`,
  language_course_lookup_word: () => "/api/v1/language/course/lookup-word",
  language_course_submit_quiz: () => "/api/v1/language/course/quiz/submit",
  language_course_plan_save: () => "/api/v1/language/course/plan",
  learning_create_collection: () => "/api/v1/learning/collections",
  learning_add_collection_item: (a) =>
    `/api/v1/learning/collections/${encodeURIComponent(String(a.collectionId))}/items`,
  learning_remove_collection_item: () => "/api/v1/learning/collection-items/remove",
  learning_submit_review: () => "/api/v1/learning/review",
  learning_save_knowledge_review_card: () => "/api/v1/learning/review/cards/from-knowledge",
  language_add_collection_item: (a) =>
    `/api/v1/learning/collections/${encodeURIComponent(String(a.collectionId))}/items`,
  history_enrichment_ensure: () => "/api/v1/history/enrichment/ensure",
  history_enrichment_refresh: () => "/api/v1/history/enrichment/refresh",
  history_enrichment_review: () => "/api/v1/history/enrichment/review",
  add_rss_feed: () => "/api/v1/rss/feeds",
  refresh_rss_feeds: () => "/api/v1/rss/refresh",
  mark_rss_article_read: (a) =>
    `/api/v1/rss/articles/${encodeURIComponent(String(a.articleId ?? ""))}/read`,
  fetch_article_url: () => "/api/v1/articles/fetch",
  news_add_source: () => "/api/v1/news/sources",
  news_refresh_now: () => "/api/v1/news/refresh",
  news_toggle_star: (a) =>
    `/api/v1/news/articles/${encodeURIComponent(String(a.storyId))}/star`,
  news_mark_read: (a) =>
    `/api/v1/news/articles/${encodeURIComponent(String(a.storyId))}/read`,
  geography_toggle_favorite: () => "/api/v1/geography/favorite",
  news_remove_source: (a) =>
    `/api/v1/news/sources/${encodeURIComponent(String(a.sourceId))}/remove`,
  study_board_save: () => "/api/v1/study-boards",
  // ---- Personal Knowledge 写入 / 扫描（与桌面端同名命令同一用例）----
  memory_save: () => "/api/v1/memory",
  memory_update: () => "/api/v1/memory/update",
  memory_confirm: (a) =>
    `/api/v1/memory/${encodeURIComponent(String(a.id ?? ""))}/confirm`,
  memory_reject: (a) =>
    `/api/v1/memory/${encodeURIComponent(String(a.id ?? ""))}/reject`,
  memory_archive: (a) =>
    `/api/v1/memory/${encodeURIComponent(String(a.id ?? ""))}/archive`,
  documents_scan: () => "/api/v1/documents/scan",
  documents_read: () => "/api/v1/documents/read",
  files_scan: () => "/api/v1/files/scan",
  files_metadata: () => "/api/v1/files/metadata",
  files_read_text: () => "/api/v1/files/read_text",
  study_board_snapshot: (a) =>
    `/api/v1/study-boards/${encodeURIComponent(String(a.boardId ?? ""))}/snapshots`,
  language_shadow_score: () => "/api/v1/language/shadow/score",
  language_speaking_create_session: () => "/api/v1/language/speaking/sessions",
  language_speaking_submit_turn: (a) =>
    `/api/v1/language/speaking/sessions/${encodeURIComponent(String(a.sessionId ?? ""))}/turns`,
  language_speaking_submit_audio: (a) =>
    `/api/v1/language/speaking/sessions/${encodeURIComponent(String(a.sessionId ?? ""))}/audio`,
  language_speaking_tts: (a) =>
    `/api/v1/language/speaking/sessions/${encodeURIComponent(String(a.sessionId ?? ""))}/tts`,
  language_speaking_pause: (a) =>
    `/api/v1/language/speaking/sessions/${encodeURIComponent(String(a.sessionId ?? ""))}/pause`,
  language_speaking_resume: (a) =>
    `/api/v1/language/speaking/sessions/${encodeURIComponent(String(a.sessionId ?? ""))}/resume`,
  language_speaking_complete: (a) =>
    `/api/v1/language/speaking/sessions/${encodeURIComponent(String(a.sessionId ?? ""))}/complete`,
  language_speaking_interrupt: (a) =>
    `/api/v1/language/speaking/sessions/${encodeURIComponent(String(a.sessionId ?? ""))}/interrupt`,
  language_speaking_add_review: (a) =>
    `/api/v1/language/speaking/sessions/${encodeURIComponent(String(a.sessionId ?? ""))}/review`,
  language_mining_add: (a) =>
    `/api/v1/language/mining/lesson/${encodeURIComponent(String(a.lessonId ?? ""))}` +
    `?maxPerKind=${encodeURIComponent(String(a.maxPerKind ?? 6))}`,
  // Home Server 写操作：restart 只签票据；confirm/cancel 才动执行链。
  services_restart: () => "/api/v1/server/actions/restart",
  confirm_action: (a) =>
    `/api/v1/server/actions/${encodeURIComponent(String(a.confirmationId ?? ""))}/confirm`,
  cancel_action: (a) =>
    `/api/v1/server/actions/${encodeURIComponent(String(a.confirmationId ?? ""))}/cancel`,
};

/**
 * POST 请求体的例外：桌面端有些命令把 DTO 包在 `request` 里（Tauri 的入参形状），
 * HTTP 契约不沿用这层包装 —— 直接发 DTO 本体，服务端 DTO 与桌面端一致。
 * 未列出的命令沿用「参数对象即请求体」。
 */
const HTTP_POST_BODIES: Record<
  string,
  (args: Record<string, unknown>) => unknown
> = {
  memory_save: (a) => a.request ?? {},
  memory_update: (a) => a.request ?? {},
  documents_read: (a) => a.request ?? {},
  // HTTP 契约只发 DTO 本体（桌面 Tauri 命令的 `request` 包装不沿用）。
  services_restart: (a) => ({ serviceId: a.serviceId ?? null }),
  confirm_action: (a) => ({ serviceId: a.serviceId ?? null }),
  cancel_action: () => ({}),
};

const HTTP_DELETE_ENDPOINTS: Record<string, (args: Record<string, unknown>) => string> = {
  delete_rss_feed: (a) =>
    `/api/v1/rss/feeds/${encodeURIComponent(String(a.feedId ?? ""))}`,
  learning_delete_knowledge_review_card: (a) =>
    `/api/v1/learning/review/cards/${encodeURIComponent(String(a.cardId ?? ""))}`,
  conversation_delete: (a) =>
    `/api/v1/conversations/${encodeURIComponent(String(a.id ?? ""))}`,
};

/**
 * PATCH 端点：路径与请求体**都要显式声明**。
 *
 * 旧实现把所有 PATCH 的 body 硬编码成 `{ note }` —— 加第二个 PATCH 端点时
 * 多字段更新会被静默截断。显式 `body` 让每个端点的契约在映射处就写清楚。
 */
export interface PatchEndpoint {
  path: (args: Record<string, unknown>) => string;
  body: (args: Record<string, unknown>) => unknown;
}

const HTTP_PATCH_ENDPOINTS: Record<string, PatchEndpoint> = {
  learning_update_collection_item_note: {
    path: (a) =>
      `/api/v1/learning/collection-items/${encodeURIComponent(String(a.itemId ?? ""))}/note`,
    body: (a) => ({ note: a.note ?? null }),
  },
  // 改名 / 归档：多字段 PATCH，body 与桌面端 `conversation_update` 参数同形。
  conversation_update: {
    path: (a) => `/api/v1/conversations/${encodeURIComponent(String(a.id ?? ""))}`,
    body: (a) => ({ title: a.title ?? null, archived: a.archived ?? null }),
  },
};

/** PATCH 请求的目标（路径 + 显式请求体）；命令没有映射时返回 null。 */
export function patchRequest(
  command: string,
  args: Record<string, unknown>,
  endpoints: Record<string, PatchEndpoint> = HTTP_PATCH_ENDPOINTS,
): { url: string; body: unknown } | null {
  const endpoint = endpoints[command];
  return endpoint ? { url: endpoint.path(args), body: endpoint.body(args) } : null;
}

export class HttpTransportError extends Error {
  constructor(
    message: string,
    readonly status: number,
    readonly code: string | null = null,
  ) {
    super(message);
    this.name = "HttpTransportError";
  }
}

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

function readErrorCode(payload: unknown): string | null {
  if (!payload || typeof payload !== "object") return null;
  const code = (payload as Record<string, unknown>).code;
  return typeof code === "string" ? code : null;
}

/** HTTP 实现：把命令映射到本地数据服务的 REST 端点。 */
export const httpTransport: CommandTransport = {
  invoke: async <T,>(command: string, args: Record<string, unknown> = {}): Promise<T> => {
    const post = HTTP_POST_ENDPOINTS[command];
    const remove = HTTP_DELETE_ENDPOINTS[command];
    const patch = patchRequest(command, args);
    const endpoint = HTTP_ENDPOINTS[command];
    let url: string;
    if (post !== undefined) url = post(args);
    else if (remove !== undefined) url = remove(args);
    else if (patch !== null) url = patch.url;
    else if (endpoint !== undefined) url = endpoint(args);
    else throw new Error(`「${command}」尚无网页端接口，请启动桌面应用使用该功能。`);
    const postBody = (HTTP_POST_BODIES[command] ?? ((value: Record<string, unknown>) => value))(args);
    let response: Response;
    try {
      response = post !== undefined
        ? await fetch(url, {
            method: "POST",
            headers: { "Content-Type": "application/json", Accept: "application/json" },
            body: JSON.stringify(postBody),
          })
        : remove !== undefined
          ? await fetch(url, { method: "DELETE", headers: { Accept: "application/json" } })
          : patch !== null
            ? await fetch(url, {
                method: "PATCH",
                headers: { "Content-Type": "application/json", Accept: "application/json" },
                body: JSON.stringify(patch.body),
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
        throw new HttpTransportError(
          `无法连接本地数据服务（${response.status}）——请确认它已启动。`,
          response.status,
        );
      }
      throw new HttpTransportError(
        readErrorMessage(payload, `本地数据服务返回 ${response.status}`),
        response.status,
        readErrorCode(payload),
      );
    }
    // 二进制响应（课时音频）：按 Content-Type 判断，不能走 json()。
    // Tauri 的 IPC 对同一命令返回 ArrayBuffer，这里保持一致，
    // 否则前端的 lessonAudio 会拿到解析失败的结果。
    // headers 可能缺失（测试 mock / 极简环境）——按 JSON 处理即可。
    const contentType = response.headers?.get?.("content-type") ?? "";
    if (contentType.startsWith("audio/") || contentType.startsWith("application/octet-stream")) {
      return (await response.arrayBuffer()) as T;
    }
    return (await response.json()) as T;
  },
  isTauriRuntime: () => false,
  subscribe: (event, handler, options) => {
    if (event !== "agent-progress" || !options?.traceId) return () => {};
    return subscribeAgentProgress(options.traceId, handler);
  },
};

/**
 * SSE 订阅一次运行进度。
 *
 * - `event: progress` + `id: {seq}`：按 seq 去重（重连 / 重放可能重复）；
 * - 收到 `done` / `failed` 主动关闭，避免 EventSource 无意义重连；
 * - 出错（例如游标过期返回 409）时用状态端点恢复最近状态，终态则收尾。
 */
function subscribeAgentProgress<T>(
  traceId: string,
  handler: (payload: T) => void,
): () => void {
  if (typeof EventSource === "undefined") return () => {};
  const eventsUrl = HTTP_ENDPOINTS.ai_run_events({ traceId });
  const statusUrl = HTTP_ENDPOINTS.ai_run_status({ traceId });
  const source = new EventSource(eventsUrl);
  let closed = false;
  let lastSeq = 0;

  const deliver = (payload: unknown) => {
    if (!payload || typeof payload !== "object") return;
    const progress = payload as { seq?: unknown; stage?: unknown };
    if (typeof progress.seq === "number") {
      if (progress.seq <= lastSeq) return;
      lastSeq = progress.seq;
    }
    handler(progress as T);
    if (progress.stage === "done" || progress.stage === "failed") {
      closed = true;
      source.close();
    }
  };

  source.addEventListener("progress", (message) => {
    if (closed) return;
    try {
      deliver(JSON.parse((message as MessageEvent).data));
    } catch {
      // 不完整帧交给下一次重连 / 状态端点恢复。
    }
  });

  source.onerror = () => {
    if (closed) return;
    void fetch(statusUrl, { headers: { Accept: "application/json" } })
      .then((response) => (response.ok ? response.json() : null))
      .then((snapshot: { latest?: unknown; state?: string } | null) => {
        if (closed || !snapshot) return;
        if (snapshot.latest) deliver(snapshot.latest);
        if (snapshot.state === "done" || snapshot.state === "failed") {
          closed = true;
          source.close();
        }
      })
      .catch(() => {
        // 状态端点也不可用：让 EventSource 自己重试。
      });
  };

  return () => {
    closed = true;
    source.close();
  };
}

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
  subscribe: (event, handler, options) =>
    inTauriRuntime()
      ? tauriTransport.subscribe(event, handler, options)
      : httpTransport.subscribe(event, handler, options),
};
