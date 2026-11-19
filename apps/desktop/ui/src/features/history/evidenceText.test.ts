import { describe, expect, it } from "vitest";
import { parseEvidenceNote } from "./evidenceText";

describe("parseEvidenceNote", () => {
  it("抽出批次批注里的史料原文，丢掉内部字段", () => {
    const parsed = parseEvidenceNote(
      "major-batch01：source-grounded（资治通鉴/旧唐书）引文：「丙寅，范阳节度使安禄山率蕃、汉之兵十余万，自幽州南向诣阙，以诛杨国忠为名。」；claim_field=result；anchor=段落精确锚。",
    );
    expect(parsed.quote).toBe(
      "丙寅，范阳节度使安禄山率蕃、汉之兵十余万，自幽州南向诣阙，以诛杨国忠为名。",
    );
    expect(parsed.note).toBeNull();
  });

  it("legacy 迁移批注整条不再展示", () => {
    const parsed = parseEvidenceNote(
      "legacy QA source-curated-semantic-v1 已审核（legacy role=主要史料）；文本语料不在本 checkout，等待知识库重建；知识层章节锚定（source_reference normalizer；anchor=章首段）",
    );
    expect(parsed.quote).toBeNull();
    expect(parsed.note).toBeNull();
  });

  it("没有原文时，保留人工写下的说明", () => {
    const parsed = parseEvidenceNote("据《史记》卷六校正地名，原文阙。");
    expect(parsed.quote).toBeNull();
    expect(parsed.note).toBe("据《史记》卷六校正地名，原文阙");
  });

  it("原文与人工说明可以并存", () => {
    const parsed = parseEvidenceNote(
      "引文：「王翦果代李信击荆。」；此处年份依《秦始皇本纪》；claim_field=process；anchor=段落精确锚。",
    );
    expect(parsed.quote).toBe("王翦果代李信击荆。");
    expect(parsed.note).toBe("此处年份依《秦始皇本纪》");
  });

  it("只有批次标号与来源标签时整段丢弃", () => {
    expect(parseEvidenceNote("depth01-D2：晋书源")).toEqual({ quote: null, note: null });
    expect(
      parseEvidenceNote("batch02-11：legacy 悬空锚重定位（章节+内容双核实）：旧唐书玄宗纪"),
    ).toEqual({ quote: null, note: null });
    expect(parseEvidenceNote("source-batch02：源入库后重定位（章节+内容双核实）")).toEqual({
      quote: null,
      note: null,
    });
  });

  it("空值与纯批注不产生噪声", () => {
    expect(parseEvidenceNote(null)).toEqual({ quote: null, note: null });
    expect(parseEvidenceNote("")).toEqual({ quote: null, note: null });
    expect(parseEvidenceNote("claim_field=impact；anchor=段落精确锚。")).toEqual({
      quote: null,
      note: null,
    });
  });
});
