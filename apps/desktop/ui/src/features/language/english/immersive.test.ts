/**
 * 沉浸式精读的纯逻辑测试。
 *
 * 句级高亮是这屏的命脉：算错一次就会「高亮跳句」，用户立刻觉得卡。
 * 译文三态与偏好持久化同样容易在重构里退化。
 */
import { describe, expect, it } from "vitest";
import type { LessonSentence } from "../../../types";
import {
  DEFAULT_PREFS,
  estimateSentenceMs,
  findSentenceIndexAt,
  formatSessionClock,
  loadPrefs,
  nextTranslationMode,
  savePrefs,
  shouldShowTranslation,
  translationModeLabel,
} from "./immersive";

function sentence(sequence: number, startMs: number, endMs: number): LessonSentence {
  return {
    id: `s-${sequence}`,
    lesson_id: "nce:1:1",
    sequence,
    start_ms: startMs,
    end_ms: endMs,
    english: `Sentence ${sequence}.`,
    chinese: `句子 ${sequence}`,
  } as LessonSentence;
}

const LINES = [sentence(0, 0, 1000), sentence(1, 1000, 2500), sentence(2, 2500, 4000)];

describe("句级高亮", () => {
  it("命中区间返回该句", () => {
    expect(findSentenceIndexAt(LINES, 0)).toBe(0);
    expect(findSentenceIndexAt(LINES, 999)).toBe(0);
    expect(findSentenceIndexAt(LINES, 1000)).toBe(1);
    expect(findSentenceIndexAt(LINES, 2499)).toBe(1);
    expect(findSentenceIndexAt(LINES, 3000)).toBe(2);
  });

  it("句间空隙保持上一句（高亮要跟得上嘴，不能提前跳）", () => {
    // 构造一个句子之间有 500ms 静默的时间轴。
    const spaced = [sentence(0, 0, 1000), sentence(1, 1500, 2000), sentence(2, 2500, 3000)];
    expect(findSentenceIndexAt(spaced, 1200)).toBe(0);
    expect(findSentenceIndexAt(spaced, 2300)).toBe(1);
  });

  it("早于第一句 / 晚于最后一句都夹在有效范围内", () => {
    const offset = [sentence(0, 5000, 6000), sentence(1, 6000, 7000)];
    expect(findSentenceIndexAt(offset, 0)).toBe(0);
    expect(findSentenceIndexAt(offset, 9999)).toBe(1);
  });

  it("没有有效时间轴时不报错，退化为停在第一句", () => {
    const broken = [sentence(0, 0, 0), sentence(1, 0, 0)];
    expect(findSentenceIndexAt(broken, 1234)).toBe(0);
    expect(findSentenceIndexAt([], 100)).toBe(-1);
  });
});

describe("译文三态", () => {
  it("隐藏模式永不显示；全显模式总是显示；逐句只显示当前句", () => {
    expect(shouldShowTranslation("hidden", 2, 2)).toBe(false);
    expect(shouldShowTranslation("all", 0, 5)).toBe(true);
    expect(shouldShowTranslation("current", 5, 5)).toBe(true);
    expect(shouldShowTranslation("current", 4, 5)).toBe(false);
  });

  it("C 键循环 hidden → current → all → hidden", () => {
    expect(nextTranslationMode("hidden")).toBe("current");
    expect(nextTranslationMode("current")).toBe("all");
    expect(nextTranslationMode("all")).toBe("hidden");
  });

  it("模式标签说人话（按钮上直接给用户看）", () => {
    expect(translationModeLabel("hidden")).toBe("译文隐藏");
    expect(translationModeLabel("current")).toBe("译文逐句");
    expect(translationModeLabel("all")).toBe("译文全显");
  });
});

describe("阅读偏好持久化", () => {
  function memoryStorage(seed?: string) {
    const map = new Map<string, string>();
    if (seed !== undefined) map.set("immersive-reader:prefs", seed);
    return {
      getItem: (key: string) => map.get(key) ?? null,
      setItem: (key: string, value: string) => {
        map.set(key, value);
      },
      dump: () => map.get("immersive-reader:prefs"),
    };
  }

  it("写入后能读回", () => {
    const storage = memoryStorage();
    savePrefs({ ...DEFAULT_PREFS, fontSize: 26, translationMode: "all" }, storage);
    const loaded = loadPrefs(storage);
    expect(loaded.fontSize).toBe(26);
    expect(loaded.translationMode).toBe("all");
  });

  it("损坏数据 / 越界值都回落默认值，不崩也不放任坏值", () => {
    expect(loadPrefs(memoryStorage("{not json"))).toEqual(DEFAULT_PREFS);
    const wild = loadPrefs(
      memoryStorage(JSON.stringify({ fontSize: 999, lineHeight: 0, measure: -5, translationMode: "weird" })),
    );
    expect(wild.fontSize).toBe(34);
    expect(wild.lineHeight).toBe(1.2);
    expect(wild.measure).toBe(40);
    expect(wild.translationMode).toBe("current");
  });

  it("存储不可用时静默失败（无痕模式不该打断阅读）", () => {
    const broken = {
      getItem: () => {
        throw new Error("denied");
      },
      setItem: () => {
        throw new Error("denied");
      },
    };
    expect(loadPrefs(broken)).toEqual(DEFAULT_PREFS);
    expect(() => savePrefs(DEFAULT_PREFS, broken)).not.toThrow();
  });
});

describe("小工具", () => {
  it("会话时钟补零", () => {
    expect(formatSessionClock(0)).toBe("00:00");
    expect(formatSessionClock(61)).toBe("01:01");
    expect(formatSessionClock(-5)).toBe("00:00");
  });

  it("朗读时长估计随词数增长，并留出呼吸", () => {
    const short = estimateSentenceMs("Excuse me.");
    const long = estimateSentenceMs(
      "I was very sorry to hear that your wife is ill and I hope she will get better very soon.",
    );
    expect(long).toBeGreaterThan(short);
    expect(short).toBeGreaterThan(400);
  });
});
