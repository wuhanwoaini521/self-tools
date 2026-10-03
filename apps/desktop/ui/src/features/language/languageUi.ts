/**
 * Language 模块内部的共享工具：异步面板（loading / error / 重试 三态）、
 * 键盘快捷键、以及少量格式化。
 *
 * 设计原则：**任何一个面板的失败都只影响它自己**——因此这里没有全局
 * spinner，也没有共享的 error 状态；调用方各自持有 `AsyncPanel`。
 */
import { useCallback, useEffect, useRef, useState } from "react";
import type {
  Difficulty,
  LanguageCode,
  LearningItemType,
  LearningStatus,
  UniversalReviewRating,
} from "../../types";
import { errorMessage } from "../../utils";

// ---------------------------------------------------------------- 异步面板

export interface AsyncPanel<T> {
  /** 上一次成功的数据；失败时保留，便于「旧数据 + 错误提示」共存。 */
  data: T | null;
  loading: boolean;
  error: string | null;
  /** 手动重试；面板的「重试」按钮直接调用它。 */
  reload: () => void;
}

/**
 * 一个独立的异步数据面板。`load` 抛错时只把 error 写进本面板，
 * 绝不冒泡，也绝不 reject —— 否则一个面板失败会连带整页白屏。
 */
export function useAsyncPanel<T>(
  load: () => Promise<T>,
  deps: readonly unknown[],
  enabled = true,
): AsyncPanel<T> {
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState<boolean>(enabled);
  const [error, setError] = useState<string | null>(null);
  const [nonce, setNonce] = useState(0);
  const loadRef = useRef(load);
  loadRef.current = load;

  useEffect(() => {
    if (!enabled) {
      setData(null);
      setLoading(false);
      setError(null);
      return;
    }
    let alive = true;
    setLoading(true);
    loadRef
      .current()
      .then((value) => {
        if (!alive) return;
        setData(value);
        setError(null);
      })
      .catch((err: unknown) => {
        if (!alive) return;
        setError(errorMessage(err));
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, nonce, enabled]);

  const reload = useCallback(() => setNonce((n) => n + 1), []);
  return { data, loading, error, reload };
}

// ---------------------------------------------------------------- 快捷键

/** 输入类元素内不触发快捷键（打字时按空格应当是空格）。 */
export function isTypingTarget(target: EventTarget | null): boolean {
  if (!target || !(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return true;
  return target.isContentEditable;
}

export interface ShortcutHandlers {
  /** Space / Enter */
  onReveal?: () => void;
  /** 1..4 */
  onRate?: (rating: UniversalReviewRating) => void;
  onPrev?: () => void;
  onNext?: () => void;
  /** Esc */
  onClose?: () => void;
}

const RATING_BY_KEY: Record<string, UniversalReviewRating> = {
  "1": "again",
  "2": "hard",
  "3": "good",
  "4": "easy",
};

/**
 * 桌面快捷键。监听挂在 document 上，用 capture 阶段 + `useEffect` 清理；
 * 输入态、修饰键、以及已经 preventDefault 过的按键一律放行。
 */
export function useLanguageShortcuts(handlers: ShortcutHandlers): void {
  const ref = useRef(handlers);
  ref.current = handlers;

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return;
      if (event.metaKey || event.ctrlKey || event.altKey) return;
      if (isTypingTarget(event.target)) return;
      const h = ref.current;
      switch (event.key) {
        case " ":
        case "Spacebar":
        case "Enter":
          if (!h.onReveal) return;
          event.preventDefault();
          h.onReveal();
          return;
        case "Escape":
          if (!h.onClose) return;
          event.preventDefault();
          h.onClose();
          return;
        case "ArrowRight":
          if (!h.onNext) return;
          event.preventDefault();
          h.onNext();
          return;
        case "ArrowLeft":
          if (!h.onPrev) return;
          event.preventDefault();
          h.onPrev();
          return;
        default:
          break;
      }
      const rating = RATING_BY_KEY[event.key];
      if (rating && h.onRate) {
        event.preventDefault();
        h.onRate(rating);
      }
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, []);
}

// ---------------------------------------------------------------- 文案

export const LANGUAGE_LABELS: Record<LanguageCode, string> = {
  eng: "English",
  jpn: "日本語",
  cmn: "普通话",
  yue: "粵語",
};

export const ITEM_TYPE_LABELS: Record<LearningItemType, string> = {
  word: "单词",
  phrase: "短语",
  sentence: "句子",
  article: "文章",
};

export const DIFFICULTY_LABELS: Record<Difficulty, string> = {
  unknown: "未知难度",
  easy: "简单",
  medium: "中等",
  hard: "困难",
};

export const STATUS_LABELS: Record<LearningStatus, string> = {
  not_started: "未开始",
  learning: "学习中",
  familiar: "熟悉",
  mastered: "已掌握",
};

export const RATING_LABELS: Record<UniversalReviewRating, string> = {
  again: "重来",
  hard: "困难",
  good: "良好",
  easy: "简单",
};

/** Unix 秒 → 本地日期时间。 */
export function formatStamp(unixSeconds: number | null | undefined): string {
  if (!unixSeconds) return "—";
  return new Date(unixSeconds * 1000).toLocaleString();
}

/** Unix 秒 → 相对自然语言（「3 小时前」）。 */
export function formatAgo(unixSeconds: number | null | undefined): string {
  if (!unixSeconds) return "—";
  const diff = Math.max(0, Math.floor(Date.now() / 1000) - unixSeconds);
  if (diff < 60) return "刚刚";
  if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`;
  if (diff < 7 * 86400) return `${Math.floor(diff / 86400)} 天前`;
  return new Date(unixSeconds * 1000).toLocaleDateString();
}

/** 0..100 的掌握度，格式化成整数百分比字符串。 */
export function formatMastery(score: number): string {
  return `${Math.max(0, Math.min(100, Math.round(score)))}%`;
}

// ---------------------------------------------------------------- 活动分组

export interface ActivityGroup<T> {
  key: "today" | "yesterday" | "week";
  label: string;
  items: T[];
}

/** 把带 `last_studied_at`（Unix 秒）的记录按 今天 / 昨天 / 本周 分组。 */
export function groupRecentByDay<T>(
  items: readonly T[],
  stamp: (item: T) => number,
): ActivityGroup<T>[] {
  const now = new Date();
  const startOfToday = new Date(
    now.getFullYear(),
    now.getMonth(),
    now.getDate(),
  ).getTime();
  const day = 86400 * 1000;
  const todayStart = Math.floor(startOfToday / 1000);
  const yesterdayStart = todayStart - 86400;
  const weekStart = todayStart - 6 * 86400;

  const buckets: ActivityGroup<T>[] = [
    { key: "today", label: "今天", items: [] },
    { key: "yesterday", label: "昨天", items: [] },
    { key: "week", label: "本周更早", items: [] },
  ];
  for (const item of items) {
    const at = stamp(item);
    const bucket =
      at >= todayStart
        ? buckets[0]
        : at >= yesterdayStart
          ? buckets[1]
          : at >= weekStart
            ? buckets[2]
            : null;
    bucket?.items.push(item);
  }
  return buckets.filter((bucket) => bucket.items.length > 0);
}

// ---------------------------------------------------------------- 复习队列统计

export interface ReviewQueueBreakdown {
  total: number;
  due: number;
  overdue: number;
  fresh: number;
}

/**
 * 到期 / 逾期 / 新卡 的计数。
 * - 逾期只信后端的 `is_overdue`（前端无法从 `due_at` 重建它，因为 `due_at`
 *   的时区语义由后端决定）。
 * - 新卡 = `repetition_count === 0`。
 * - 到期 = 队列本身（`reviewQueue` 只返回已到期的卡片），逾期是新卡的子集。
 */
export function summarizeReviewQueue(
  queue: readonly { card: { repetition_count: number }; is_overdue: boolean }[],
): ReviewQueueBreakdown {
  let due = 0;
  let overdue = 0;
  let fresh = 0;
  for (const entry of queue) {
    due += 1;
    if (entry.is_overdue) overdue += 1;
    if (entry.card.repetition_count === 0) fresh += 1;
  }
  return { total: queue.length, due, overdue, fresh };
}

// ---------------------------------------------------------------- 空态文案

/** 统一的「这不是 bug，是还没开始」文案。 */
export const EMPTY_COPY = {
  review:
    "还没有到期的复习卡。学习一些新内容，到时间复习时它们会出现在这里。",
  mistakes: "目前没有错题。继续学习，回答错误的内容会自动收进这里。",
  weak: "暂无薄弱项。掌握度偏低的内容会自动列在这里。",
  continue: "还没有进行中的课程。创建一个 Lesson，随时可以从中断处继续。",
  activity: "还没有学习记录。打开任意条目开始学习后，这里会显示最近的活动。",
  lessons: "还没有课程。可以从搜索结果或句库里挑几条内容组成一节课。",
  sentences: "这个语言还没有导入句子。先安装 Starter Pack 试试。",
  sources: "还没有登记词库来源。安装 Starter Pack 会自动登记来源与许可证。",
  search: "输入至少一个字符开始搜索。",
  searchNone: "没有匹配的结果。换个词，或者切换到其它语言。",
  chunkMissing: "词典未收录",
} as const;

/** 合并 className，跳过 falsy；多个组件的按钮/行都需要同一套拼接规则。 */
export function cx(...parts: Array<string | false | null | undefined>): string {
  return parts.filter(Boolean).join(" ");
}