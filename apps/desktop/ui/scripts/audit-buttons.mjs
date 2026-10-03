/**
 * 死按钮审计：对每个页面的可见按钮，模拟点击，检查
 *   1. 是否抛出运行时错误
 *   2. 是否导致页面导航（意外跳转）
 *   3. DOM 是否完全无变化且没有任何可见反馈（= 无反应按钮）
 *
 * 只点「安全」的按钮：跳过会删除数据 / 触发原生对话框的（标记为 skip）。
 */
import { chromium } from "playwright";

const BASE = process.env.BASE ?? "http://127.0.0.1:1420";
const PAGES = [
  ["Home", "Home"], ["Review", "Review"], ["History", "History"],
  ["Geography", "Geography"], ["Language", "Language"], ["Study", "Study"],
  ["News", "News"], ["RSS", "RSS"], ["Travel", "Travel"],
  ["Markdown", "Markdown"], ["Collections", "Collections"],
  ["Knowledge", "Knowledge"], ["Graph", "Graph"], ["Search", "Search"],
  ["System", "System"],
];

/** 危险/不可自动点的按钮（会删数据、开原生文件框、发网络请求）。 */
const DANGEROUS = /删除|remove|delete|清空|退出|reset|清缓存|uninstall|停止|取消导入/i;

const results = [];

const b = await chromium.launch({ channel: "chrome" });
const ctx = await b.newContext({ viewport: { width: 1440, height: 900 } });
const page = await ctx.newPage();
await page.goto(`${BASE}/`, { waitUntil: "domcontentloaded" });
await page.waitForTimeout(3000);

for (const [id, label] of PAGES) {
  await page.evaluate((l) => {
    [...document.querySelectorAll(".app-nav-item")].find((e) => e.getAttribute("aria-label") === l)?.click();
  }, label);
  await page.waitForTimeout(2600);

  const buttons = await page.evaluate(() => {
    const pane = document.querySelector(".page-pane:not(.page-hidden)");
    if (!pane) return [];
    return [...pane.querySelectorAll("button")]
      .filter((b) => b.offsetParent && !b.disabled && (b.textContent || "").trim().length > 0)
      .map((b, i) => ({
        i,
        text: (b.textContent || "").trim().slice(0, 22),
        cls: (b.className || "").toString().slice(0, 40),
      }));
  });

  let dead = 0;
  let errored = 0;
  let clicked = 0;
  for (const btn of buttons) {
    if (DANGEROUS.test(btn.text)) continue;
    const before = await page.evaluate(() => {
      const pane = document.querySelector(".page-pane:not(.page-hidden)");
      return pane ? pane.innerHTML.length : 0;
    });
    const urlBefore = page.url();
    const errs = [];
    const onErr = (e) => errs.push(e.message.slice(0, 100));
    page.on("pageerror", onErr);
    let ok = true;
    try {
      await page.evaluate(
        (idx) => {
          const pane = document.querySelector(".page-pane:not(.page-hidden)");
          const list = [...pane.querySelectorAll("button")].filter(
            (b) => b.offsetParent && !b.disabled && (b.textContent || "").trim().length > 0,
          );
          list[idx]?.click();
        },
        buttons.indexOf(btn),
      );
      await page.waitForTimeout(700);
    } catch {
      ok = false;
    }
    page.off("pageerror", onErr);
    clicked++;
    const after = await page.evaluate(() => {
      const pane = document.querySelector(".page-pane:not(.page-hidden)");
      return pane ? pane.innerHTML.length : 0;
    });
    const toast = await page.evaluate(() => document.querySelectorAll(".toast").length);
    const navigated = page.url() !== urlBefore;
    if (errs.length) {
      errored++;
      results.push({ page: label, button: btn.text, issue: "ERROR", detail: errs[0] });
    } else if (!navigated && before === after && toast === 0) {
      dead++;
      results.push({ page: label, button: btn.text, issue: "NO_REACTION", detail: `DOM 无变化 (${before}→${after})` });
    }
    // 点完回到该页，避免测试漂移
    if (page.url() !== urlBefore) {
      await page.goto(`${BASE}/`, { waitUntil: "domcontentloaded" });
      await page.waitForTimeout(1200);
      await page.evaluate((l) => {
        [...document.querySelectorAll(".app-nav-item")].find((e) => e.getAttribute("aria-label") === l)?.click();
      }, label);
      await page.waitForTimeout(1800);
    }
  }
  console.log(`${label.padEnd(12)} 按钮 ${String(buttons.length).padStart(3)} 已点 ${String(clicked).padStart(3)} 无反应 ${dead} 报错 ${errored}`);
}

await b.close();
console.log("\n================ 问题汇总 ================");
if (results.length === 0) console.log("✓ 未发现无反应按钮");
else {
  const grouped = {};
  for (const r of results) {
    grouped[r.page] ??= [];
    grouped[r.page].push(r);
  }
  for (const [page, list] of Object.entries(grouped)) {
    console.log(`■ ${page}`);
    list.forEach((r) => console.log(`   [${r.issue}] "${r.button}" — ${r.detail}`));
  }
}
console.log(`\n合计: ${results.length} 个问题`);