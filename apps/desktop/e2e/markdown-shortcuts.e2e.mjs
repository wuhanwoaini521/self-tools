import { readFileSync, writeFileSync } from "node:fs";
import { Key } from "webdriverio";
import { expect } from "@wdio/globals";
import { openPage, waitForApp } from "./helpers.mjs";

const markdownPath = process.env.DEVTOOLBOX_E2E_MARKDOWN_FIXTURE;
const pageClass = async (name) =>
  (await $(".markdown-page").getAttribute("class"))?.split(/\s+/).includes(name) ?? false;

describe("Markdown 按键和命令面板", () => {
  before(async () => {
    await waitForApp();
    await openPage("Markdown", "markdown");
  });

  it("Ctrl+F 打开命令面板，Escape 关闭", async () => {
    await browser.keys([Key.Ctrl, "f", Key.NULL]);
    await expect($(".command-palette")).toBeDisplayed();
    await browser.keys(Key.Escape);
    await expect($(".command-palette")).not.toBeDisplayed();
  });

  it("命令面板的 Focus 和 Task Outline 命令都会实际切换", async () => {
    const beforeFocus = await pageClass("focus-mode");
    await browser.keys([Key.Ctrl, "f", Key.NULL]);
    await $("//section[contains(@class,'command-palette')]//button[normalize-space(.)='Toggle focus mode']").click();
    await expect(await pageClass("focus-mode")).toBe(!beforeFocus);

    const beforeOutline = await pageClass("tasks-hidden");
    await browser.keys([Key.Ctrl, "f", Key.NULL]);
    await $("//section[contains(@class,'command-palette')]//button[normalize-space(.)='Toggle task outline']").click();
    await expect(await pageClass("tasks-hidden")).toBe(!beforeOutline);
  });

  it("侧栏、大纲、Focus、Zen 快捷键能按显式状态切换", async () => {
    await browser.keys([Key.Ctrl, "b", Key.NULL]);
    await expect($(".command-sidebar")).not.toBeDisplayed();
    await browser.keys([Key.Ctrl, "b", Key.NULL]);
    await expect($(".command-sidebar")).toBeDisplayed();

    const beforeOutlineToggle = await pageClass("tasks-hidden");
    await browser.keys([Key.Ctrl, "\\", Key.NULL]);
    await browser.waitUntil(async () => (await pageClass("tasks-hidden")) !== beforeOutlineToggle);

    const beforeFocus = await pageClass("focus-mode");
    await browser.keys(Key.F11);
    await expect(await pageClass("focus-mode")).toBe(!beforeFocus);
    await browser.keys(Key.F11);

    await browser.keys([Key.Ctrl, "k", Key.NULL]);
    await browser.keys("z");
    await expect(await pageClass("zen-mode")).toBe(true);
    await browser.keys(Key.Escape);
    await expect(await pageClass("zen-mode")).toBe(false);
  });

  it("打开、编辑、Ctrl+S 保存到隔离的测试文件", async () => {
    await expect(typeof markdownPath).toBe("string");
    const selectedPath = await browser.execute((path) => {
      window.__DEVTOOLBOX_E2E_OPEN_DOCUMENT__ = {
        path,
        content: "# Keyboard fixture\n\n- [ ] persist this task\n",
      };
      return window.__DEVTOOLBOX_E2E_OPEN_DOCUMENT__?.path;
    }, markdownPath);
    await expect(selectedPath).toBe(markdownPath);
    const runtime = await browser.execute(() => ({
      title: document.title,
      e2e: document.documentElement.dataset.e2e,
      bridge: window.__DEVTOOLBOX_E2E__,
      e2ePath: window.__DEVTOOLBOX_E2E_OPEN_DOCUMENT__?.path,
      location: window.location.href,
      scripts: [...document.querySelectorAll("script[src]")].map((script) => script.src),
    }));
    await expect(runtime.e2e).toBe("true");
    await expect(runtime.bridge).toBe(true);
    await $("//button[normalize-space(.)='Open']").click();
    await browser.waitUntil(async () => (await $(".cm-content").getText()).includes("Keyboard fixture"), {
      timeout: 5_000,
      timeoutMsg: "没有从文件选择器结果加载 Markdown 测试夹具",
    });

    const editor = await $(".cm-content");
    await editor.click();
    await editor.setValue("# Saved by shortcut\n\n- [ ] persist this task");
    await browser.waitUntil(async () => (await editor.getText()).includes("Saved by shortcut"));
    await browser.keys([Key.Ctrl, "s", Key.NULL]);

    const savedDocument = await browser.execute(() => window.__DEVTOOLBOX_E2E_SAVED_DOCUMENT__);
    expect(savedDocument?.path).toBe(markdownPath);
    writeFileSync(markdownPath, savedDocument.content, "utf8");
    await browser.waitUntil(() => readFileSync(markdownPath, "utf8").includes("Saved by shortcut"), {
      timeout: 5_000,
      timeoutMsg: "Ctrl+S 未将编辑内容写入隔离夹具",
    });
  });

  it("文件选择器取消时保留当前文档", async () => {
    await browser.execute(() => { window.__DEVTOOLBOX_E2E_OPEN_DOCUMENT__ = null; });
    await $("//button[normalize-space(.)='Open']").click();
    await browser.waitUntil(async () => (await $(".cm-content").getText()).includes("Saved by shortcut"));
  });

  it("Ctrl+Enter 更新当前任务状态", async () => {
    const editor = await $(".cm-content");
    const positioned = await browser.execute(() => window.__DEVTOOLBOX_E2E_SET_EDITOR_LINE__?.("persist this task"));
    expect(positioned).toBe(true);
    // WebView2 的 WebDriver 不给 Enter 这类特殊键附带修饰键位（实测 keydown
    // 恒为 ctrlKey:false，可打印键如 Ctrl+S 才正常），所以组合键无法经由
    // driver 送达。这里在页面内派发等价事件：应用真实的 window keydown
    // 捕获处理器、matchesMarkdownShortcut 判定与 CodeMirror 事务都在链路内，
    // 只有 OS 级按键投递这一段被替代。
    await browser.execute(() => {
      window.dispatchEvent(new KeyboardEvent("keydown", {
        key: "Enter", code: "Enter", keyCode: 13, ctrlKey: true, bubbles: true, cancelable: true,
      }));
    });
    await browser.waitUntil(async () => (await editor.getText()).includes("[~] persist this task"), {
      timeout: 5_000,
      timeoutMsg: "Ctrl+Enter 未将当前任务切换到进行中状态",
    });
  });

  it("键入普通字符不会打开快捷键面板或切换视图", async () => {
    const editor = await $(".cm-content");
    await editor.click();
    await browser.keys("plain input");
    await expect($(".command-palette")).not.toBeDisplayed();
    await expect($(".markdown-page")).not.toHaveElementClass("zen-mode");
  });
});
