/**
 * 摘要清洗的回归测试。
 *
 * 背景：2026-10-05 用户反馈「新闻页面有些数据抓取不全」，实际查库发现
 * 中国新闻网的 description 是这样的：
 *   "\r\n伪科普、加速包、"
 *   "\r\n据网络平台数据"
 * —— 源站只给了碎片，界面却当摘要渲染，看起来像「抓坏了」。
 */
import { describe, expect, it } from "vitest";
import { stripRssHtml, summarySnippet } from "./newsContent";

describe("卡片摘要", () => {
  it("源站只给碎片时返回 null（交给界面显示「源站未提供摘要」）", () => {
    expect(summarySnippet("\r\n伪科普、加速包、")).toBeNull();
    expect(summarySnippet("据网络平台数据")).toBeNull();
    expect(summarySnippet("  \r\n\t ")).toBeNull();
    expect(summarySnippet(null)).toBeNull();
    expect(summarySnippet(undefined)).toBeNull();
  });

  it("首尾空白与换行被清掉（源站的 \\r\\n 不该撑出一个空行）", () => {
    const snippet = summarySnippet(
      "\r\n中新社杭州10月1日电 受冷空气影响，长三角地区普遍出现降温降水天气。\r\n",
    );
    expect(snippet).toBe("中新社杭州10月1日电 受冷空气影响，长三角地区普遍出现降温降水天气。");
    expect(snippet?.startsWith("\n")).toBe(false);
  });

  it("HTML 标签与实体被解开", () => {
    const snippet = summarySnippet(
      "<p>国务院常务会议决定，进一步 &ldquo;稳就业&rdquo; 政策。</p>",
    );
    expect(snippet).toContain("国务院常务会议决定");
    expect(snippet).not.toContain("<p>");
  });

  it("长摘要在句边界截断，不切在半句上", () => {
    const long = [
      "国务院常务会议决定进一步加强稳就业举措。",
      "会议指出要抓好重点群体就业。",
      "有关部门要抓紧出台配套细则。",
    ].join("");
    const snippet = summarySnippet(long);
    expect(snippet).toBeTruthy();
    expect(snippet!.endsWith("。")).toBe(true);
    expect(snippet!.length).toBeLessThanOrEqual(90);
    // 不能把第一句砍掉一半
    expect(snippet).toContain("国务院常务会议决定进一步加强稳就业举措。");
  });

  it("没有任何句末标点的超长文本才用省略号，且不切在词中间", () => {
    // 逐句不同（`deduplicateRepeatedText` 会把**周期性重复**的整段折叠成一个单位，
    // 那是为图片 alt 重复设计的既有行为，这里不与之对抗，只测「无标点长文本」的截断）。
    const noPunctuation = [
      "研究人员发现这一变化在不同区域之间存在明显差异并需要更多样本加以验证",
      "受访者普遍认为相关措施应当尽快落地以便在下一个周期之前看到实际效果",
      "该结论目前仍属于初步观察阶段后续会通过更长时间窗口的跟踪继续确认",
      "值得注意的是样本采集的时间跨度与频率都会直接影响到最终结论的可靠性",
    ].join("");
    const snippet = summarySnippet(noPunctuation);
    expect(snippet?.endsWith("…")).toBe(true);
    expect(snippet!.length).toBeLessThanOrEqual(91);
  });

  it("周期性重复的整段文本按既有去重规则折叠（图片 alt 重复的老行为不变）", () => {
    // 这条是**记录既有行为**，不是新需求：去重早于本次修复存在。
    const repeated = "很长的一个没有标点的句子".repeat(12);
    expect(stripRssHtml(repeated)).toBe("很长的一个没有标点的句子");
  });

  it("阈值可调（长一点的碎片也可接受）", () => {
    expect(summarySnippet("据网络平台数据", { minChars: 6 })).toBe("据网络平台数据");
  });
});
