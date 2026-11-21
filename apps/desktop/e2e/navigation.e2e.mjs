import { expect } from "@wdio/globals";
import { openPage, waitForApp } from "./helpers.mjs";

describe("桌面导航与主要页面", () => {
  before(async () => waitForApp());

  it("一级入口、分区子导航与更多页面都能打开对应页面", async () => {
    const primaryLink = (label) =>
      $(
        `//nav[@aria-label='主导航']//button[contains(@class,'app-bar-primary-link') and contains(.,'${label}')]`,
      );

    // 首页由品牌进入（顶栏不含单独的「首页」链接）。
    await $(".brand").click();
    await expect($(".page-pane:not(.page-hidden)")).toBeDisplayed();

    for (const label of ["学习", "旅行", "资讯"]) {
      const link = await primaryLink(label);
      await link.click();
      await expect(link).toHaveAttribute("aria-current", "page");
      await expect($(".page-pane:not(.page-hidden)")).toBeDisplayed();
    }

    // 学习分区：四个学科通过子导航打开，语言页仍保留标题断言。
    await (await primaryLink("学习")).click();
    for (const [label, id] of [
      ["历史", "history"],
      ["地理", "geography"],
      ["语言", "language"],
      ["画板", "study-board"],
    ]) {
      const link = await $(`//nav[@aria-label='学习分区']//button[contains(.,'${label}')]`);
      await link.click();
      await expect(link).toHaveAttribute("aria-current", "page");
      await expect($(".page-pane:not(.page-hidden)")).toBeDisplayed();
      if (id === "language") await expect($(".lang-page h1")).toHaveText("Language");
    }

    // 资讯分区：新闻 / RSS 分库，各自是独立页面。
    await (await primaryLink("资讯")).click();
    for (const label of ["新闻", "RSS"]) {
      const link = await $(`//nav[@aria-label='资讯分区']//button[contains(.,'${label}')]`);
      await link.click();
      await expect(link).toHaveAttribute("aria-current", "page");
      await expect($(".page-pane:not(.page-hidden)")).toBeDisplayed();
    }

    // 「更多页面」抽屉：复习中心 / 收藏 / 搜索，选择后应自动收起。
    for (const label of ["复习中心", "收藏", "搜索"]) {
      await $("button[aria-label='更多页面']").click();
      const item = await $(`//div[@id='app-more-sheet']//button[contains(.,'${label}')]`);
      await item.click();
      await expect($("#app-more-sheet")).not.toBeDisplayed();
      await expect($(".page-pane:not(.page-hidden)")).toBeDisplayed();
    }

    // 已移出一级导航的页面仍必须能通过旧深链打开（productNav 契约）。
    for (const id of ["markdown", "knowledge", "server", "system", "graph"]) {
      await openPage(id);
    }
  });

  it("Home refreshes persisted learning progress after returning from Review", async () => {
    await openPage("Home", "home");
    const prompt = `Sprint 10 progress refresh ${Date.now()}`;
    const cardId = await browser.execute(async (question) => {
      const invoke = window.__TAURI_INTERNALS__?.invoke;
      if (typeof invoke !== "function") throw new Error("Tauri invoke bridge unavailable");
      const card = await invoke("learning_save_knowledge_review_card", {
        draft: {
          module: "history",
          entityType: "event",
          entityId: `sprint10-home-refresh-${Date.now()}`,
          sourceTitle: "Isolated Sprint 10 review fixture",
          prompt: question,
          answer: "The isolated E2E database.",
          userNote: "Temporary native E2E fixture.",
        },
      });
      return card.id;
    }, prompt);

    await openPage("Review", "review");
    await browser.waitUntil(async () => (await $(".review-card-shell h2").getText()) === prompt);
    await $("textarea[placeholder*='写下你的答案']").setValue("The isolated E2E database.");
    await $(`//button[contains(normalize-space(.), '直接看答案')]`).click();
    await $(".review-rating-grid button:nth-child(3)").click();
    await browser.waitUntil(async () => await $(".review-receipt").isDisplayed());

    await openPage("Home", "home");
    // Personal Hub 的完成数文案与 class 都随改版变化；按可见文本定位，不绑 DOM 结构。
    const learningFooter = () => $("//span[contains(normalize-space(.), '今日学习') and strong]");
    await browser.waitUntil(async () =>
      (await learningFooter().getText()).includes("今日学习 1 个主题"),
    );
    expect(cardId).toContain("knowledge:history:event:sprint10-home-refresh-");
    expect(await learningFooter().getText()).toContain("今日学习 1 个主题");
  });

  it("设置对话框打开、修改主题并在关闭后保留选择", async () => {
    await openPage("Home", "home");
    await $("button[aria-label='Settings']").click();
    const settings = await $(".settings-dialog");
    await settings.waitForDisplayed();

    // 主题控件是 radiogroup（button.theme-card-option[role=radio]），
    // 不再是 <select id="ui-theme-select">；选中态由 aria-checked 表达。
    const themes = await $$(".theme-card-option");
    expect(themes.length).toBeGreaterThan(1);
    await themes[1].click();
    await expect(themes[1]).toHaveAttribute("aria-checked", "true");

    await $("button[title='关闭设置']").click();
    await expect(settings).not.toBeDisplayed();

    await $("button[aria-label='Settings']").click();
    const reopened = await $$(".theme-card-option");
    await expect(reopened[1]).toHaveAttribute("aria-checked", "true");
    await $("button[title='关闭设置']").click();
  });

  it("provider 密钥只写入、普通设置读取脱敏且配置更新保留凭据", async () => {
    await openPage("Home", "home");
    await $("button[aria-label='Settings']").click();
    await $("#ai-key").setValue("phase1-secret-regression-marker");
    await $("//button[normalize-space(.)='保存 AI Key']").click();
    await browser.waitUntil(async () =>
      (await $(".settings-dialog").getText()).includes("凭据已安全保存到本机后端"),
    );

    const result = await browser.execute(async (marker) => {
      const invoke = window.__TAURI_INTERNALS__?.invoke;
      if (typeof invoke !== "function") throw new Error("Tauri invoke bridge unavailable");
      const before = await invoke("get_settings");
      const beforeText = JSON.stringify(before);
      await invoke("put_settings", { settings: before });
      const after = JSON.stringify(await invoke("get_settings"));
      return {
        configured: before.secrets?.ai_api_key_configured === true,
        leakedBefore: beforeText.includes(marker),
        leakedAfterConfigWrite: after.includes(marker),
      };
    }, "phase1-secret-regression-marker");
    expect(result).toEqual({ configured: true, leakedBefore: false, leakedAfterConfigWrite: false });

    await $("button[title='关闭设置']").click();
  });

  it("Travel 行程通过后端契约持久化并忽略客户端时间戳", async () => {
    const result = await browser.execute(async () => {
      const invoke = window.__TAURI_INTERNALS__?.invoke;
      if (typeof invoke !== "function") throw new Error("Tauri invoke bridge unavailable");
      const context = {
        city: "E2E 行程测试城市",
        days: 2,
        date_range: { start: "2026-10-10", end: "2026-10-11" },
      };
      const plan = {
        context,
        selected_keys: ["e2e-poi-1"],
        assigned_days: { "e2e-poi-1": 2 },
        must_visit_keys: ["e2e-poi-1"],
        updated_at: 1,
      };
      const saved = await invoke("travel_plan_save", { plan });
      const loaded = await invoke("travel_plan_get", { context });
      return {
        serverTimestamp: saved.updated_at > 1,
        selectedKeys: loaded?.selected_keys,
        assignedDay: loaded?.assigned_days?.["e2e-poi-1"],
      };
    });
    expect(result).toEqual({
      serverTimestamp: true,
      selectedKeys: ["e2e-poi-1"],
      assignedDay: 2,
    });
  });

  it("AI 面板在未配置模型时仍能显示受控空态", async () => {
    // 首页把 AI 引导放在页面内，浮动气泡在其它页面才可见。
    await openPage("travel");
    await $(".ai-bubble-launch").click();
    await expect($(".ai-panel")).toBeDisplayed();
    await expect($(".ai-panel-unconfigured")).toBeDisplayed();
    await $(".ai-panel button[title='关闭']").click();
    await expect($(".ai-panel")).not.toBeDisplayed();
  });
});
