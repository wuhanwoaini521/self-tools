/**
 * 数据充实度审计：逐页看「有没有真实内容」，并区分
 *   - 真实数据
 *   - 诚实空态（组件自己说明为什么空）
 *   - 能力声明（本环境不支持，明确告知）
 *   - 空白/异常（真坏了）
 */
import { chromium } from "playwright";

const BASE = process.env.BASE ?? "http://127.0.0.1:1420";
const PAGES = [
  ["Home", "Home"], ["Review", "Review"], ["History", "History"],
  ["Geography", "Geography"], ["Language", "Language"], ["Study", "Study"],
  ["News", "News"], ["RSS", "RSS"], ["Travel", "Travel"],
  ["Markdown", "Markdown"], ["Collections", "Collections"],
  ["Knowledge", "Knowledge"], ["Graph", "Graph"], ["Search", "Search"],
  ["Server", "Server"], ["System", "System"],
];

const b = await chromium.launch({ channel: "chrome" });
const ctx = await b.newContext({ viewport: { width: 1440, height: 900 } });
const page = await ctx.newPage();
await page.goto(`${BASE}/`, { waitUntil: "domcontentloaded" });
await page.waitForTimeout(3000);

const rows = [];
for (const [id, label] of PAGES) {
  await page.evaluate((l) => {
    [...document.querySelectorAll(".app-nav-item")].find((e) => e.getAttribute("aria-label") === l)?.click();
  }, label);
  await page.waitForTimeout(2800);
  const r = await page.evaluate(() => {
    const pane = document.querySelector(".page-pane:not(.page-hidden)");
    if (!pane) return { kind: "NO_PANE" };
    const text = pane.innerText.trim();
    const html = pane.innerHTML;
    // 诚实空态/能力声明的典型标记
    const emptyNodes = pane.querySelectorAll(
      ".knowledge-empty, .lang-empty, .en-empty, [class*='empty'], [class*='unsupported']",
    );
    const emptyText = [...emptyNodes].map((n) => n.textContent.trim()).filter(Boolean).slice(0, 2);
    const cards = pane.querySelectorAll("article, li, [class*='card'], [class*='row']").length;
    const images = pane.querySelectorAll("img").length;
    const buttons = pane.querySelectorAll("button").length;
    return {
      kind: text.length < 30 ? "BLANK" : emptyText.length ? "EMPTY_STATED" : "HAS_DATA",
      chars: text.length,
      cards, images, buttons,
      emptyText: emptyText.join(" / ").slice(0, 80),
      head: text.replace(/\s+/g, " ").slice(0, 90),
    };
  });
  rows.push([label, r]);
  const mark = { HAS_DATA: "●", EMPTY_STATED: "○", BLANK: "✗", NO_PANE: "✗" }[r.kind] ?? "?";
  console.log(`${mark} ${label.padEnd(12)} ${String(r.kind).padEnd(13)} 卡片${String(r.cards ?? 0).padStart(3)} 图${String(r.images ?? 0).padStart(2)} 钮${String(r.buttons ?? 0).padStart(3)}  ${r.emptyText || r.head || ""}`);
}
await b.close();

const blanks = rows.filter(([, r]) => r.kind === "BLANK" || r.kind === "NO_PANE");
const empties = rows.filter(([, r]) => r.kind === "EMPTY_STATED");
const withData = rows.filter(([, r]) => r.kind === "HAS_DATA");
console.log(`\n有数据 ${withData.length} / 诚实空态 ${empties.length} / 空白 ${blanks.length}`);
if (blanks.length) console.log("空白页:", blanks.map(([l]) => l).join(", "));
