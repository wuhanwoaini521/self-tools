/**
 * 笔刷引擎单测：压感映射、线宽计算、旧数据兼容。
 *
 * 这些是手感的地基——压感反推与线宽公式一旦回归，画板手感会静默变差，
 * 所以必须有断言锁住。
 */
import { describe, expect, it } from "vitest";
import {
  BRUSH_PROFILES,
  clamp01,
  readPressure,
  taperFactorAt,
  widthAt,
  type BrushKind,
} from "./brush";

describe("压感读取", () => {
  it("触控笔用真实 pressure", () => {
    expect(readPressure({ pointerType: "pen", pressure: 0.25 })).toBeCloseTo(0.25, 5);
    expect(readPressure({ pointerType: "pen", pressure: 0.9 })).toBeCloseTo(0.9, 5);
  });

  it("鼠标写��慢 → 压感大（粗）", () => {
    const slow = readPressure({ pointerType: "mouse", pressure: 0.5, velocity: 0.1 });
    const fast = readPressure({ pointerType: "mouse", pressure: 0.5, velocity: 3 });
    expect(slow).toBeGreaterThan(fast);
  });

  it("指针不动（velocity 0）视为最慢", () => {
    expect(readPressure({ pointerType: "mouse", pressure: 0.5, velocity: 0 })).toBeCloseTo(1, 5);
  });

  it("触控笔压力恒为 1（很多触摸设备）时回退到速度反推", () => {
    // pen 但 pressure=1 不可信 → 应走速度分支，而不是永远最粗。
    const slow = readPressure({ pointerType: "pen", pressure: 1, velocity: 0.1 });
    const fast = readPressure({ pointerType: "pen", pressure: 1, velocity: 3 });
    expect(slow).toBeGreaterThan(fast);
  });

  it("异常值被夹到 0..1", () => {
    expect(clamp01(Number.NaN)).toBe(0.5);
    expect(clamp01(-3)).toBe(0);
    expect(clamp01(9)).toBe(1);
  });
});

describe("线宽计算", () => {
  it("压力越大越粗，且不超过基础宽 + 增益上限", () => {
    const profile = BRUSH_PROFILES.ballpoint;
    const thin = widthAt(profile, 0);
    const thick = widthAt(profile, 1);
    expect(thick).toBeGreaterThan(thin);
    expect(thick).toBeCloseTo(profile.baseWidth, 5);
  });

  it("荧光笔几乎不受压感影响（粗且平）", () => {
    const profile = BRUSH_PROFILES.highlighter;
    // 用**相对**变化判断：荧光笔该是「粗且平」，而不是像圆珠笔那样随压力明显变细。
    const thin = widthAt(profile, 0);
    const thick = widthAt(profile, 1);
    expect(thick / thin).toBeLessThan(1.15);
  });

  it("收笔保留最小可见宽度，不会细到消失", () => {
    const profile = BRUSH_PROFILES.ballpoint;
    expect(widthAt(profile, 0, 0.55)).toBeGreaterThan(0.5);
  });

  it("起笔/收笔收细，中间不收", () => {
    const profile = BRUSH_PROFILES.ballpoint;
    const total = 10;
    expect(taperFactorAt(profile, 0, total)).toBeLessThan(taperFactorAt(profile, 4, total));
    expect(taperFactorAt(profile, 4, total)).toBe(1);
  });

  it("马克笔/荧光笔不收笔（好笔记软件的马克笔是平头）", () => {
    for (const kind of ["marker", "highlighter"] as BrushKind[]) {
      expect(taperFactorAt(BRUSH_PROFILES[kind], 0, 10)).toBe(1);
    }
  });

  it("点太少时不收笔（避免单点变 0 宽）", () => {
    expect(taperFactorAt(BRUSH_PROFILES.ballpoint, 0, 2)).toBe(1);
  });
});
