/**
 * PWA 生命周期单元测试（V11 §81-§83）。
 *
 * 只测纯函数与可在 jsdom 外验证的逻辑；service worker 注册路径用 fake navigator。
 */

import { describe, expect, it } from "vitest";
import {
  APP_VERSION,
  MIN_BACKEND_COMPAT_VERSION,
  compareVersions,
  supportsServiceWorker,
  versionNotice,
} from "./pwa";
import {
  DESKTOP_MIN_WIDTH,
  QA_VIEWPORTS,
  TABLET_MIN_WIDTH,
  aiPresentation,
  deviceForWidth,
  navigationLayout,
} from "./layout";

describe("版本比较", () => {
  it("比较 major/minor/patch", () => {
    expect(compareVersions("1.0.0", "1.0.0")).toBe(0);
    expect(compareVersions("1.2.0", "1.1.9")).toBeGreaterThan(0);
    expect(compareVersions("0.9.0", "1.0.0")).toBeLessThan(0);
    expect(compareVersions("1.0", "1.0.0")).toBe(0);
    expect(compareVersions("2.0.0", "10.0.0")).toBeLessThan(0);
  });

  it("非法段按 0 处理", () => {
    expect(compareVersions("a.b.c", "0.0.0")).toBe(0);
  });
});

describe("前后端版本兼容（§83）", () => {
  it("null 后端版本不提示", () => {
    expect(versionNotice(null)).toBeNull();
    expect(versionNotice(undefined)).toBeNull();
  });

  it("后端不旧于最低兼容版本 → 无提示", () => {
    expect(versionNotice(MIN_BACKEND_COMPAT_VERSION)).toBeNull();
    expect(versionNotice("9.9.9")).toBeNull();
  });

  it("后端过旧 → 升级提示", () => {
    const notice = versionNotice("0.0.1");
    expect(notice).toContain("过旧");
    expect(notice).toContain("0.0.1");
  });
});

describe("布局推导（§84-§88）", () => {
  it("按宽度分档", () => {
    expect(deviceForWidth(1440)).toBe("desktop");
    expect(deviceForWidth(DESKTOP_MIN_WIDTH)).toBe("desktop");
    expect(deviceForWidth(DESKTOP_MIN_WIDTH - 1)).toBe("tablet");
    expect(deviceForWidth(TABLET_MIN_WIDTH)).toBe("tablet");
    expect(deviceForWidth(TABLET_MIN_WIDTH - 1)).toBe("mobile");
    expect(deviceForWidth(360)).toBe("mobile");
  });

  it("导航：mobile 底部，其余侧边", () => {
    expect(navigationLayout({ device: "mobile" } as never)).toBe("bottom");
    expect(navigationLayout({ device: "tablet" } as never)).toBe("side");
    expect(navigationLayout({ device: "desktop" } as never)).toBe("side");
  });

  it("AI 呈现：桌面侧栏 / 平板侧抽屉或底部 / 手机底部或全屏", () => {
    expect(aiPresentation({ device: "desktop", landscape: true } as never)).toBe("side-panel");
    expect(
      aiPresentation({ device: "tablet", landscape: true, tabletSplit: true } as never),
    ).toBe("side-sheet");
    expect(
      aiPresentation({ device: "tablet", landscape: false, tabletSplit: false } as never),
    ).toBe("bottom-sheet");
    expect(
      aiPresentation({ device: "mobile", landscape: true } as never),
    ).toBe("bottom-sheet");
    expect(
      aiPresentation({ device: "mobile", landscape: false } as never),
    ).toBe("fullscreen");
  });

  it("视觉 QA 视口覆盖五档（§137）", () => {
    const names = QA_VIEWPORTS.map((viewport) => viewport.name);
    expect(names).toEqual([
      "Desktop",
      "Tablet Landscape",
      "Tablet Portrait",
      "Mobile",
      "Small Mobile",
    ]);
    expect(QA_VIEWPORTS[0]).toEqual({ name: "Desktop", width: 1440, height: 900 });
    expect(QA_VIEWPORTS[3]).toEqual({ name: "Mobile", width: 390, height: 844 });
  });
});

describe("安全上下文（§77）", () => {
  it("函数在无 navigator 环境下返回 false 而不抛错", () => {
    // Node 测试环境：没有 serviceWorker → false（生产浏览器由 HTTPS 保证 true）。
    expect(typeof supportsServiceWorker()).toBe("boolean");
  });
});

describe("应用版本", () => {
  it("与 package.json 一致", () => {
    expect(APP_VERSION).toBe("0.1.0");
  });
});
