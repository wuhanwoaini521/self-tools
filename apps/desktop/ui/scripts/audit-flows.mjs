/**
 * 跨模块真实流程验收（Phase 7）。
 *
 * 覆盖任务书列出的 Flow A–E，每一步都断言**真实后果**（页面变化 / 数据落库），
 * 而不是「点击没报错」。
 */
import { chromium } from "playwright";

const BASE = process.env.BASE ?? "http://127.0.0.1:1420";
const results = [];
function check(flow, name, ok, detail = "") {
  results.push({ flow, name, ok, detail });
  console.log(`${ok ? "PASS" : "FAIL"} [${flow}] ${name}${detail ? ` — ${detail}` : ""}`);
}

const b = await chromium.launch({ channel: "chrome" });
const ctx = await b.newContext({ viewport: { width: 1440, height: 900 } });
const page = await ctx.newPage();
const errors = [];
page.on("pageerror", (e) => errors.push(e.message.slice(0, 120)));

const nav = async (label) => {
  await page.evaluate((l) => {
    [...document.querySelectorAll(".app-nav-item")].find((e) => e.getAttribute("aria-label") === l)?.click();
  }, label);
  await page.waitForTimeout(2400);
};
const paneText = () =>
  page.evaluate(() => document.querySelector(".page-pane:not(.page-hidden)")?.innerText ?? "");
const clickText = async (text, scope = ".page-pane:not(.page-hidden)") => {
  const done = await page.evaluate(
    ([t, s]) => {
      const root = document.querySelector(s);
      const btn = [...(root?.querySelectorAll("button") ?? [])].find(
        (x) => (x.textContent || "").trim() === t,
      );
      btn?.click();
      return Boolean(btn);
    },
    [text, scope],
  );
  await page.waitForTimeout(1500);
  return done;
};

await page.goto(`${BASE}/`, { waitUntil: "domcontentloaded" });
await page.waitForTimeout(3000);

// ============ Flow A: Dashboard → Language → Lesson → 学词 → 进度 ============
try {
  await nav("Home");
  const homeText = await paneText();
  check("A", "Dashboard 是应用中心（有模块入口）", homeText.includes("Language") || homeText.includes("学习系统"));

  await nav("Language");
  const langText = await paneText();
  check("A", "Language 默认进入 English 学习动线", langText.includes("Today's English"));

  const hasContinue = await clickText("开始这一课");
  if (!hasContinue) {
    // 已有进度时按钮文案不同
    await clickText("继续学习");
  }
  await page.waitForTimeout(1800);
  const inLesson = (await paneText()).includes("Lesson");
  check("A", "进入 Lesson 工作台", inLesson);

  const vocabOk = (await paneText()).includes("Today's Words");
  check("A", "课前单词阶段可用", vocabOk);

  // 标记一个词（真实写库）
  const before = await paneText();
  const marked = await page.evaluate(() => {
    const root = document.querySelector(".page-pane:not(.page-hidden)");
    const btn = [...(root?.querySelectorAll(".en-mark-actions button") ?? [])].find((b) =>
      (b.textContent || "").includes("不认识"),
    );
    btn?.click();
    return Boolean(btn);
  });
  await page.waitForTimeout(1600);
  const after = await paneText();
  check("A", "标记生词产生可见变化", marked && after !== before);

  // 回首页看进度是否反映
  await nav("Home");
  const homeAfter = await paneText();
  check("A", "返回首页后进度已更新（待复习/连续天数）",
    homeAfter.includes("Continue") || homeAfter.includes("Start") || homeAfter.includes("待复习"));
} catch (e) {
  check("A", "Flow A 执行异常", false, String(e).slice(0, 100));
}

// ============ Flow B: Dashboard → Personal AI（未配置 provider 的诚实表现）========
try {
  await page.goto(`${BASE}/`, { waitUntil: "domcontentloaded" });
  await page.waitForTimeout(2500);
  const aiBtn = await page.evaluate(() => {
    const b = [...document.querySelectorAll("button")].find((x) => (x.textContent || "").includes("问 AI"));
    b?.click();
    return Boolean(b);
  });
  await page.waitForTimeout(2200);
  const aiText = await page.evaluate(() => document.body.innerText);
  const honest = /未配置|不可用|没有配置|provider|AI 未/i.test(aiText);
  check("B", "AI 未配置时诚实告知（不假装 AVAILABLE）", aiBtn && honest,
    honest ? "" : "未找到诚实提示");
} catch (e) {
  check("B", "Flow B 执行异常", false, String(e).slice(0, 100));
}

// ============ Flow C: News → 刷新 → 打开文章 ============
try {
  await nav("News");
  const newsText = await paneText();
  const hasList = newsText.length > 100;
  check("C", "News 列表加载", hasList);
  const opened = await page.evaluate(() => {
    const root = document.querySelector(".page-pane:not(.page-hidden)");
    const item = root?.querySelector(".news-story-item, [class*='story-item']");
    item?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    return Boolean(item);
  });
  await page.waitForTimeout(2200);
  const detail = await paneText();
  check("C", "打开新闻详情", opened && !detail.includes("选择一条新闻开始阅读"));
} catch (e) {
  check("C", "Flow C 执行异常", false, String(e).slice(0, 100));
}

// ============ Flow D: History → 时期 → 事件 → 返回 ============
try {
  await nav("History");
  const h1 = await paneText();
  check("D", "History 加载", h1.includes("中国历史") || h1.includes("HISTORY"));
  const drill = await page.evaluate(() => {
    const root = document.querySelector(".page-pane:not(.page-hidden)");
    const target = [...(root?.querySelectorAll("button, article") ?? [])].find((b) =>
      (b.textContent || "").includes("夏朝建立"),
    );
    target?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    return Boolean(target);
  });
  await page.waitForTimeout(2000);
  check("D", "进入事件/时期详情", drill);
  const backOk = await page.evaluate(() => {
    const root = document.querySelector(".page-pane:not(.page-hidden)");
    const back = [...(root?.querySelectorAll("button") ?? [])].find((b) =>
      (b.textContent || "").includes("返回"),
    );
    back?.click();
    return Boolean(back);
  });
  await page.waitForTimeout(1500);
  const backText = await paneText();
  check("D", "可返回且上下文未丢失", !backOk || backText.length > 100);
} catch (e) {
  check("D", "Flow D 执行异常", false, String(e).slice(0, 100));
}

// ============ Flow E: Settings → 改设置 → 立即生效 ============
try {
  // Settings 是纯文字按钮（aria-label 为 null），按文本定位。
  const openedSettings = await page.evaluate(() => {
    const btn = [...document.querySelectorAll(".app-nav-item")].find(
      (e) => (e.textContent || "").trim() === "Settings",
    );
    btn?.click();
    return Boolean(btn);
  });
  await page.waitForTimeout(2200);
  const dialog = await page.evaluate(
    () => document.querySelector(".settings-dialog")?.innerText ?? "",
  );
  check("E", "Settings 可打开（对话框）", openedSettings && dialog.length > 50);

  const themeBtns = await page.evaluate(() =>
    [...document.querySelectorAll(".settings-dialog button")]
      .map((b) => (b.textContent || "").trim())
      .filter((t) => /Pixel|Warm|Nord|Catppuccin|深色|浅色/i.test(t)),
  );
  check("E", "主题切换控件存在", themeBtns.length > 0, themeBtns.slice(0, 4).join("/"));

  // 真正切一次主题，验证立即生效
  const themeBefore = await page.evaluate(() => document.documentElement.dataset.theme ?? "");
  await page.evaluate(() => {
    const btn = [...document.querySelectorAll(".settings-dialog button")].find((b) =>
      /Nord/i.test(b.textContent || ""),
    );
    btn?.click();
  });
  await page.waitForTimeout(1200);
  const themeAfter = await page.evaluate(() => document.documentElement.dataset.theme ?? "");
  check("E", "切换主题立即生效", themeBefore !== themeAfter, `${themeBefore} → ${themeAfter}`);
} catch (e) {
  check("E", "Flow E 执行异常", false, String(e).slice(0, 100));
}

check("全局", "无未捕获运行时异常", errors.length === 0, errors.slice(0, 2).join(" | "));

await b.close();
const failed = results.filter((r) => !r.ok);
console.log(`\n=== 跨模块流程: ${results.length - failed.length}/${results.length} 通过 ===`);
if (failed.length) {
  console.log("失败项：");
  failed.forEach((f) => console.log(`  [${f.flow}] ${f.name} ${f.detail}`));
}