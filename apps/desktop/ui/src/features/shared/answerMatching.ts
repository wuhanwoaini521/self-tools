/**
 * 复习作答的匹配判定（V13 W3 句子卡需要它）。
 *
 * 原来只做 `答案 === 标准答案` 的全等比较。单词卡没问题，但**句子卡不行**：
 * 用户写 "excuse me"、标准答案是 "Excuse me"（或反过来少一个冠词），
 * 全等判定一律判错 —— 于是句子卡变成纯挫败。
 *
 * 判定改为**词级比对**（忽略大小写与标点）：
 * - 全对 → 100%
 * - 差一两个词 → 提示「接近了」，仍然算「基本会」
 * - 差很多 / 顺序完全不对 → 判错
 *
 * 纯函数，可单测。
 */

/** 词级切分：小写、去掉标点，保留字母与内部连字符/撇号。 */
export function reviewTokens(text: string): string[] {
  return text
    .toLowerCase()
    .split(/[^a-z0-9'’-]+/)
    .filter((token) => token.length > 0);
}

export interface MatchResult {
  /** 词级覆盖率 0–1。 */
  ratio: number;
  /** 每个标准答案词是否出现在作答里。 */
  hit: boolean[];
  /** 作答里多出来的词（不在标准答案中）。 */
  extra: string[];
  /** 是否全对（含大小写/标点差异时也算）。 */
  exact: boolean;
  /** 是否「基本会」（见 `matchAnswer` 的判定规则）。 */
  close: boolean;
}

/**
 * 比对作答与标准答案。空作答 → ratio 0（不算对）。
 *
 * 「基本会」的规则：没多出多余的词，且
 * - 全部命中；或
 * - 只差 **1 个词** 且覆盖率 ≥ 0.6。
 *
 * 为什么不是简单的「覆盖率 ≥ 0.8」：短句（`Excuse me, sir`）差一个词就是
 * 只覆盖 66%，按 0.8 判会把「基本说对」判成错；长句差一两个词又确实该练。
 * 「最多差 1 个词」同时照顾了两种句子。
 */
export function matchAnswer(answer: string, expected: string): MatchResult {
  const answerTokens = reviewTokens(answer);
  const expectedTokens = reviewTokens(expected);
  if (expectedTokens.length === 0) {
    return { ratio: 0, hit: [], extra: answerTokens, exact: false, close: false };
  }
  const hit = expectedTokens.map((token) => answerTokens.includes(token));
  const covered = hit.filter(Boolean).length;
  const extra = answerTokens.filter((token) => !expectedTokens.includes(token));
  const ratio = covered / expectedTokens.length;
  let missed = expectedTokens.length - covered;
  return {
    ratio,
    hit,
    extra,
    exact: missed == 0 && extra.length === 0,
    close: extra.length === 0 && (missed == 0 || (missed == 1 && ratio >= 0.6)),
  };
}

/** 给用户看的一句话反馈（不显示分数，只说人话）。 */
export function matchFeedback(result: MatchResult): string {
  if (result.exact) return "全对。";
  if (result.close) {
    const missed = result.hit.filter((value) => !value).length;
    return missed === 1 ? "差一个词，基本对了。" : `差 ${missed} 个词，基本对了。`;
  }
  if (result.ratio > 0) {
    return `只对上 ${Math.round(result.ratio * 100)}% —— 再看一遍原句。`;
  }
  return "没对上。先别急，看原句再试一次。";
}
