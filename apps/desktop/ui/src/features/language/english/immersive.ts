/**
 * 沉浸式精读的纯逻辑（可单测，不含 React）。
 *
 * 抽出来是因为这些规则最容易在重构里悄悄退化：
 * - 播放位置 → 当前句（LRC 句级时间轴，句子可能重叠或留空）；
 * - 译文显示三态（沉浸输入的关键开关：全隐藏 / 全显示 / 只看当前句）；
 * - 阅读偏好（字号/行距/栏宽）持久化，解析失败必须回落默认值而不是崩。
 */
import type { LessonSentence } from "../../../types";

/** 译文显示模式。 */
export type TranslationMode = "hidden" | "current" | "all";

/** 阅读偏好（按设备存，不同设备可以不同）。 */
export interface ReadingPrefs {
  /** 正文字号（px）。 */
  fontSize: number;
  /** 行高（倍数）。 */
  lineHeight: number;
  /** 栏宽（ch）。 */
  measure: number;
  translationMode: TranslationMode;
}

export const DEFAULT_PREFS: ReadingPrefs = {
  fontSize: 20,
  lineHeight: 1.8,
  measure: 68,
  translationMode: "current",
};

const PREFS_KEY = "immersive-reader:prefs";
const TRANSLATION_MODES: TranslationMode[] = ["hidden", "current", "all"];

function clampNumber(value: unknown, min: number, max: number, fallback: number): number {
  const parsed = typeof value === "number" ? value : Number(value);
  if (!Number.isFinite(parsed)) return fallback;
  return Math.min(max, Math.max(min, parsed));
}

/** 读偏好；localStorage 不可用或数据损坏时回落默认值（不抛错）。 */
export function loadPrefs(storage?: Pick<Storage, "getItem">): ReadingPrefs {
  try {
    const raw = storage?.getItem(PREFS_KEY);
    if (!raw) return { ...DEFAULT_PREFS };
    const parsed = JSON.parse(raw) as Partial<ReadingPrefs>;
    const mode = TRANSLATION_MODES.includes(parsed.translationMode as TranslationMode)
      ? (parsed.translationMode as TranslationMode)
      : DEFAULT_PREFS.translationMode;
    return {
      fontSize: clampNumber(parsed.fontSize, 14, 34, DEFAULT_PREFS.fontSize),
      lineHeight: clampNumber(parsed.lineHeight, 1.2, 2.6, DEFAULT_PREFS.lineHeight),
      measure: clampNumber(parsed.measure, 40, 100, DEFAULT_PREFS.measure),
      translationMode: mode,
    };
  } catch {
    return { ...DEFAULT_PREFS };
  }
}

/** 存偏好；失败静默（偏好丢失不该打断阅读）。 */
export function savePrefs(prefs: ReadingPrefs, storage?: Pick<Storage, "setItem">): void {
  try {
    storage?.setItem(PREFS_KEY, JSON.stringify(prefs));
  } catch {
    // 无痕模式 / 配额满：忽略。
  }
}

/**
 * 播放位置对应的句子下标。
 *
 * 规则：
 * - 命中区间 → 该句；
 * - 落在两句之间的空隙 → 仍是**上一句**（提前高亮跟得上嘴，别跳到还没听到的句子）；
 * - 早于第一句 → 第一句；晚于最后一句 → 最后一句；
 * - 句子无有效时间轴（end_ms ≤ start_ms）时按顺序兜底，避免全篇不高亮。
 */
export function findSentenceIndexAt(sentences: LessonSentence[], ms: number): number {
  if (sentences.length === 0) return -1;
  const timed = sentences.some((item) => item.end_ms > item.start_ms);
  if (!timed) {
    // 没有时间轴：按朗读进度粗略均分，句子数 1 时恒为 0。
    return Math.min(sentences.length - 1, 0);
  }
  let cursor = 0;
  for (let index = 0; index < sentences.length; index += 1) {
    const item = sentences[index];
    if (item.end_ms > item.start_ms && ms >= item.start_ms && ms < item.end_ms) return index;
    if (item.end_ms > item.start_ms && ms >= item.end_ms) cursor = index;
  }
  return ms < sentences[0].start_ms ? 0 : cursor;
}

/** 该句是否显示译文（三态）。 */
export function shouldShowTranslation(
  mode: TranslationMode,
  sentenceIndex: number,
  activeIndex: number,
): boolean {
  if (mode === "all") return true;
  if (mode === "hidden") return false;
  return sentenceIndex === activeIndex;
}

/** 翻译模式循环：hidden → current → all → hidden。 */
export function nextTranslationMode(mode: TranslationMode): TranslationMode {
  const index = TRANSLATION_MODES.indexOf(mode);
  return TRANSLATION_MODES[(index + 1) % TRANSLATION_MODES.length] ?? "current";
}

/** 模式的中文说明（按钮上直接显示，别让用户猜 C 键是干什么的）。 */
export function translationModeLabel(mode: TranslationMode): string {
  switch (mode) {
    case "hidden":
      return "译文隐藏";
    case "all":
      return "译文全显";
    default:
      return "译文逐句";
  }
}

/** 会话时长 → 「12:34」。 */
export function formatSessionClock(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  const minutes = Math.floor(total / 60);
  const rest = total % 60;
  return `${String(minutes).padStart(2, "0")}:${String(rest).padStart(2, "0")}`;
}

/** 一句的朗读时长估计（无音频时间轴时给 TTS 播放留时间）。 */
export function estimateSentenceMs(text: string): number {
  const words = text.trim().split(/\s+/).filter(Boolean).length;
  // 母语者朗读约 150 wpm；留 0.4s 呼吸。
  return Math.round((words / 150) * 60_000) + 400;
}
