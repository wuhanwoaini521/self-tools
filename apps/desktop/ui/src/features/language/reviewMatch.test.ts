/**
 * 复习作答匹配测试。
 *
 * 句子卡是这次新增的重点：判定太严（全等）会让「excuse me」被判错，
 * 太松（包含就算对）会让学习者糊弄过去。阈值必须锁住。
 */
import { describe, expect, it } from "vitest";
import { matchAnswer, matchFeedback, reviewTokens } from "./reviewMatch";

describe("复习作答匹配", () => {
  it("大小写与标点差异不算错", () => {
    expect(matchAnswer("excuse me", "Excuse me!").exact).toBe(true);
    expect(matchAnswer("Excuse me.", "Excuse me!").exact).toBe(true);
    expect(matchFeedback(matchAnswer("excuse me", "Excuse me!"))).toBe("全对。");
  });

  it("少一个功能词算「接近了」而不是全错", () => {
    const result = matchAnswer("Excuse me", "Excuse me, sir");
    expect(result.exact).toBe(false);
    expect(result.close).toBe(true);
    expect(result.extra).toEqual([]);
    expect(matchFeedback(result)).toBe("差一个词，基本对了。");
  });

  it("漏掉一半词不算接近", () => {
    const result = matchAnswer("Listen", "Listen to the tape then answer this question");
    expect(result.close).toBe(false);
    expect(result.ratio).toBeLessThan(0.5);
    expect(matchFeedback(result)).toContain("再看一遍原句");
  });

  it("多说的词会被抓出来（防止糊弄）", () => {
    const result = matchAnswer("Is this your handbag today", "Is this your handbag?");
    expect(result.exact).toBe(false);
    // 多说的词 → 不算「基本会」（否则可以把答案念一遍混过去）
    expect(result.close).toBe(false);
    expect(result.extra).toEqual(["today"]);
  });

  it("空作答不判对", () => {
    const result = matchAnswer("   ", "Excuse me!");
    expect(result.ratio).toBe(0);
    expect(result.exact).toBe(false);
    expect(matchFeedback(result)).toContain("没对上");
  });

  it("分词保留撇号与连字符", () => {
    expect(reviewTokens("Don't put it in the hand-bag, please.")).toEqual([
      "don't",
      "put",
      "it",
      "in",
      "the",
      "hand-bag",
      "please",
    ]);
  });
});
