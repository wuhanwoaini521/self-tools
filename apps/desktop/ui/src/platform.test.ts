/**
 * platform 能力适配层：外链 / 本机文件 / 设置写入。
 *
 * 这层存在的意义就是「组件不再自己判断运行时」，所以测试必须同时覆盖
 * 桌面（有 `__TAURI_INTERNALS__`）与网页（没有）两种分支，以及失败路径。
 */
import { afterEach, describe, expect, it, vi } from "vitest";

const openUrlMock = vi.fn<() => Promise<void>>();
const openPathMock = vi.fn<() => Promise<void>>();

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: () => openUrlMock(),
  openPath: () => openPathMock(),
}));

async function platformModule() {
  return await import("./platform");
}

function stubWindow(desktop: boolean) {
  const open = vi.fn(() => ({}) as Window);
  const windowStub: Record<string, unknown> = { open };
  if (desktop) windowStub.__TAURI_INTERNALS__ = {};
  vi.stubGlobal("window", windowStub);
  return open;
}

afterEach(() => {
  vi.unstubAllGlobals();
  openUrlMock.mockReset();
  openPathMock.mockReset();
});

describe("platform 能力适配", () => {
  it("网页端外链开新标签，本机文件明确不可用", async () => {
    const open = stubWindow(false);
    const platform = await platformModule();

    expect(platform.canOpenExternalUrl()).toBe(true);
    expect(platform.canOpenLocalPath()).toBe(false);

    await platform.openExternalUrl("https://example.test/a");
    expect(open).toHaveBeenCalledWith(
      "https://example.test/a",
      "_blank",
      "noopener,noreferrer",
    );
    expect(openUrlMock).not.toHaveBeenCalled();

    await expect(platform.revealLocalPath("E:/notes/a.md")).rejects.toThrow(
      platform.LOCAL_PATH_UNAVAILABLE,
    );
    expect(openPathMock).not.toHaveBeenCalled();
  });

  it("弹窗被拦截时给出可诊断的错误而不是静默失败", async () => {
    const open = stubWindow(false);
    open.mockReturnValue(null as unknown as Window);
    const platform = await platformModule();

    await expect(platform.openExternalUrl("https://example.test/b")).rejects.toThrow(
      /弹窗/,
    );
  });

  it("桌面端外链与本机文件都交给系统插件", async () => {
    const open = stubWindow(true);
    const platform = await platformModule();

    expect(platform.canOpenLocalPath()).toBe(true);
    openUrlMock.mockResolvedValue();
    openPathMock.mockResolvedValue();

    await platform.openExternalUrl("https://example.test/c");
    await platform.revealLocalPath("E:/notes/b.md");

    expect(openUrlMock).toHaveBeenCalledTimes(1);
    expect(openPathMock).toHaveBeenCalledTimes(1);
    expect(open).not.toHaveBeenCalled();
  });

  it("设置写入两端都具备能力（权限由后端判定）", async () => {
    stubWindow(false);
    const web = await platformModule();
    expect(web.canWriteSettings()).toBe(true);

    stubWindow(true);
    const desktop = await platformModule();
    expect(desktop.canWriteSettings()).toBe(true);
  });
});
