import { expect } from "@wdio/globals";
import { openPage, waitForApp } from "./helpers.mjs";

describe("桌面导航与主要页面", () => {
  before(async () => waitForApp());

  it("每个主导航入口都能打开对应页面", async () => {
    const pages = [
      ["Home", "home"],
      ["Markdown", "markdown"],
      ["RSS", "rss"],
      ["News", "news"],
      ["Travel", "travel"],
      ["Geography", "geography"],
      ["History", "history"],
      ["Language", "language"],
      ["Knowledge", "knowledge"],
      ["Study", "study-board"],
      ["Server", "server"],
      ["System", "system"],
      ["Search", "search"],
    ];

    for (const [label, id] of pages) {
      await openPage(label, id);
      await expect($(".app-nav-item.active")).toBeDisplayed();
      await expect($(".page-pane:not(.page-hidden)")).toBeDisplayed();
    }
  });

  it("设置对话框打开、修改主题并在关闭后保留选择", async () => {
    await openPage("Home", "home");
    await $("button[title='Settings']").click();
    const settings = await $(".settings-dialog");
    await settings.waitForDisplayed();

    const theme = await $("#ui-theme-select");
    await theme.selectByIndex(1);
    const selectedTheme = await theme.getValue();
    await $("button[title='关闭设置']").click();
    await expect(settings).not.toBeDisplayed();
    await $("button[title='Settings']").click();
    await expect($("#ui-theme-select")).toHaveValue(selectedTheme);
    await $("button[title='关闭设置']").click();
  });

  it("AI 面板在未配置模型时仍能显示受控空态", async () => {
    await openPage("Home", "home");
    await $("button[title='Ask AI']").click();
    await expect($(".ai-panel")).toBeDisplayed();
    await expect($(".ai-panel-unconfigured")).toBeDisplayed();
    await $(".ai-panel button[title='关闭']").click();
    await expect($(".ai-panel")).not.toBeDisplayed();
  });
});
