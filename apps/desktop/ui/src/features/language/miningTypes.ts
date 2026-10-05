/**
 * 句子挖掘的前后端契约（V13 W3）。
 *
 * 与 `crates/core/src/language/mining.rs` 的 `MinedCard` 一一对应
 * （Rust 侧是 snake_case）。
 */

/** 卡的形态：填空 / 听写 / 中译英。 */
export type MinedCardKind = "cloze" | "dictation" | "translate";

/** 挖空位置的选取理由（可解释：告诉用户「为什么挖这个」）。 */
export type BlankReason = "lesson_vocab" | "function_word" | "content_word" | "fallback";

/** 一张挖掘出来的卡。 */
export interface MinedCard {
  kind: MinedCardKind;
  sentence_id: string;
  sequence: number;
  /** 完整原句（复习时作为语境展示）。 */
  sentence: string;
  chinese: string | null;
  /** 挖空后的题干（只有填空卡有）。 */
  prompt: string | null;
  answer: string;
  start_ms: number | null;
  end_ms: number | null;
  reason: BlankReason;
}

/** 挖掘入库的结果。 */
export interface MinedReport {
  lesson_id: string;
  cards: number;
  items: MinedCard[];
}
