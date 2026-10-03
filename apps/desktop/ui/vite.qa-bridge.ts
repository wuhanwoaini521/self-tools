// Visual QA 专用 Vite 插件（不进生产）：当 `VITE_QA_BRIDGE=1` 时，
// 在 index.html 注入一个 `window.__TAURI_INTERNALS__` 桥，把 Tauri 命令映射到
// 经 Vite 代理的本地只读 HTTP 服务（apps/server，真实数据）。
//
// 仅用于截图 / 无桌面壳验收：页面检测 `__TAURI_INTERNALS__` 以启用数据访问；
// 未在桥内的命令会**明确抛错**（而不是静默返回空数据），这样截图里出现的
// 空态一定是真实的空态，不是伪装出来的。
import type { Plugin } from "vite";

/**
 * 命令 → URL 模板（`${key}` 会被 args[key] 的 URL 编码值替换）。
 *
 * 只列服务端真实实现了的只读端点。
 */
const ENDPOINT_TEMPLATES: Record<string, string> = {
  history_semantic_home: "/__qa/api/v1/history/home",
  history_semantic_period: "/__qa/api/v1/history/periods/${periodId}",
  history_semantic_story: "/__qa/api/v1/history/stories/${storyId}",
  history_semantic_event: "/__qa/api/v1/history/events/${eventId}",
  history_semantic_person: "/__qa/api/v1/history/people/${personId}",
  history_semantic_work: "/__qa/api/v1/history/works/${workId}",
  history_semantic_search: "/__qa/api/v1/history/search?q=${query}",

  // 英语课程（NCE）只读端点。
  get_settings: "/__qa/api/v1/settings",

  language_course_today: "/__qa/api/v1/language/course/today",
  language_course_books: "/__qa/api/v1/language/course/books",
  language_course_book: "/__qa/api/v1/language/course/book/${bookId}",
  language_course_lesson: "/__qa/api/v1/language/course/lesson/${lessonId}",
  language_course_progress: "/__qa/api/v1/language/course/progress",
  language_course_plan_get: "/__qa/api/v1/language/course/plan",

};

/**
 * 写命令 / 其它模块在 QA 桥里返回**内存假结果**：只用于让 UI 走完交互路径
 * （页面逻辑、布局、可点击性），**不冒充真实数据**。
 * 真实数据只来自上面的只读端点（真实 SQLite 数据）。
 */
const WRITE_STUBS: Record<string, unknown> = {
  learning_get_review_queue: [],
  learning_get_today: {
    studied_topics_today: 0,
    pending_reviews_count: 0,
    average_mastery: 0,
    recent_streak_days: 0,
    continue_items: [],
    review_stats: {
      total_due: 0,
      due_count: 0,
      overdue_count: 0,
      upcoming_count: 0,
      by_module: {},
      mastered_count: 0,
      learning_count: 0,
      total_cards: 0,
    },
    explore_recommendations: [],
    recent_collections: [],
    recent_bookmarks: [],
  },
  learning_get_review_stats: {
    total_due: 0,
    due_count: 0,
    overdue_count: 0,
    upcoming_count: 0,
    by_module: {},
    mastered_count: 0,
    learning_count: 0,
    total_cards: 0,
  },
  learning_list_collections: [],
  personal_ai_status: { configured: false, modules: [], tools: [] },

  // English 写路径：走完交互但不改真实数据（QA 只验 UI）。
  language_course_lookup_word: {
    entry: {
      word: "qa",
      lemma: "qa",
      phonetic: "ˌkjuːˈeɪ",
      pos: "abbr.",
      translation_zh: "n. 质量保证（质量评估）",
      definition_en: "quality assurance",
      frequency: 0,
      bnc: 0,
      tags: [],
      collins: 0,
      forms: [],
    },
    seen_count: 0,
    occurrences: [],
    learning: null,
  },
  language_course_update_progress: {
    lesson_id: "qa",
    stage: "vocabulary",
    position_ms: 0,
    sentence_seq: 0,
    vocab_index: 0,
    shadow_seq: 0,
    quiz_score: null,
    completed_at: null,
    study_seconds: 0,
    updated_at: 0,
  },
  language_course_mark_word: {
    entity_key: "language:word:en:qa",
    module: "language",
    entity_type: "word",
    entity_id: "en:qa",
    entity_title: "qa",
    status: "Learning",
    study_count: 1,
    review_count: 0,
    correct_count: 0,
    incorrect_count: 0,
    mastery_score: 10,
    last_studied_at: 0,
    next_review_at: null,
    interval_days: 0,
    ease: 2.5,
    custom_tags: [],
  },
  language_course_quiz: [
    {
      kind: "vocabulary",
      word: "qa",
      phonetic: "ˌkjuːˈeɪ",
      options: ["n. 质量保证", "n. 问答", "adj. 空的", "v. 翻译"],
      answer: 0,
    },
    { kind: "fill_blank", sentence: "Don't ______ to ask questions.", chinese: "有问题尽管问。", answer: "hesitate" },
    { kind: "dictation", lesson_id: "qa", sentence_seq: 0, start_ms: 0, end_ms: 3000, answer: "Excuse me!" },
    { kind: "translate", chinese: "打扰一下！", reference: "Excuse me!" },
  ],
  language_course_submit_quiz: {
    lesson_id: "qa",
    total: 4,
    correct: 3,
    score: 75,
    wrong_words: [],
    finished_at: 0,
  },
  language_lesson_audio: null,
  language_data_status: {
    data_dir: "/Users/you/self-tools/config",
    nce_source: "/Users/you/NCE",
    nce_books: 2,
    nce_lessons: 6,
    nce_lessons_with_audio: 6,
    dict_entries: 770611,
  },
};

const BRIDGE_SOURCE = `
// 事件插件内部（Tauri API 的 listen/unlisten 依赖）：桥里没有真实事件源，记下即可。
window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
  unregisterListener: () => undefined,
};
window.__TAURI_INTERNALS__ = {
  // Tauri API 内部依赖：把回调挂到 window 上并返回 id（真实运行时由 Rust 侧实现）。
  transformCallback: (callback, once) => {
    const id = Math.floor(Math.random() * 1e9);
    Object.defineProperty(window, "_" + id, { value: callback, configurable: true });
    if (once) {
      window.addEventListener("unload", () => { delete window["_" + id]; }, { once: true });
    }
    return id;
  },
  unregisterCallback: (id) => { delete window["_" + id]; },
  invoke: async (cmd, args) => {
    const templates = ${JSON.stringify(ENDPOINT_TEMPLATES)};
    const stubs = ${JSON.stringify(WRITE_STUBS)};
    if (cmd in stubs) return stubs[cmd];
    // 事件订阅/退订（agent 进度等）：桥里没有事件源，返回一个空句柄即可。
    if (typeof cmd === "string" && cmd.startsWith("plugin:event|")) return 0;
    const template = templates[cmd];
    if (!template) throw new Error("qa-bridge: unsupported command " + cmd);
    const render = (source, params) => {
      let out = "";
      let cursor = 0;
      for (;;) {
        const open = source.indexOf("\${", cursor);
        if (open === -1) { out += source.slice(cursor); break; }
        out += source.slice(cursor, open);
        const close = source.indexOf("}", open + 2);
        const key = source.slice(open + 2, close);
        out += encodeURIComponent(String((params ?? {})[key] ?? ""));
        cursor = close + 1;
      }
      return out;
    };
    const response = await fetch(render(template, args));
    if (!response.ok) return null;
    return response.json();
  },
};`;

/** 仅开发模式且显式开启时注入 QA 桥；生产构建绝不携带。 */
export function qaBridge(): Plugin {
  const enabled = process.env.VITE_QA_BRIDGE === "1";
  return {
    name: "visual-qa-bridge",
    apply: "serve",
    enforce: "pre",
    transformIndexHtml() {
      if (!enabled) return undefined;
      return [
        {
          tag: "script",
          attrs: { type: "text/javascript" },
          children: BRIDGE_SOURCE,
        },
      ];
    },
  };
}