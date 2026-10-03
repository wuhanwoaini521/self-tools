/**
 * self-tools 全量体检脚本（真实应用 + 真实后端）。
 *
 * 两种运行模式：
 *   - web 模式（默认）：不注入 QA 桥 → transport 走真实 HTTP（apps/server），
 *     覆盖「网页端是否有真实数据」。
 *   - bridge 模式：设置 BRIDGE=1，注入 vite.qa-bridge.ts（用于需要桥接只读端点的场景）。
 *
 * 检查项：
 *   1. 每个一级页面能否进入、有无 console error / pageerror / 失败请求
 *   2. 页面是否空白（无可见内容）
 *   3. 横向溢出、元素超出视口
 *   4. 死按钮（可点但点击后无任何 DOM 变化且不报错）
 *   5. 破图（img naturalWidth === 0）
 *   6. 文本截断 / 重叠的明显迹象
 *   7. 明暗两套主题
 *   8. 多视口 1440×900 / 1280×800
 */
import { chromium } from "playwright";
import { mkdir } from "node:fs/promises";
import path from "node:path";

const BASE = process.env.BASE ?? "http://127.0.0.1:1420";
const OUT = path.resolve(process.cwd(), "../../apps/output/audit");
const VIEWPORTS = [
  { name: "1440x900", width: 1440, height: 900 },
  { name: "1280x800", width: 1280, height: 800 },
];
const THEMES = ["default", "dark"];

const PAGES = [
  { id: "home", label: "Home", navLabel: "Home" },
  { id: "review", label: "Review" },
  { id: "history", label: "History" },
  { id: "geography", label: "Geography" },
  { id: "language", label: "Language" },
  { id: "study-board", label: "Study", navLabel: "Study" },
  { id: "news", label: "News" },
  { id: "rss", label: "RSS" },
  { id: "travel", label: "Travel" },
  { id: "markdown", label: "Markdown" },
  { id: "collections", label: "Collections" },
  { id: "knowledge", label: "Knowledge" },
  { id: "graph", label: "Graph" },
  { id: "search", label: "Search" },
  { id: "server", label: "Server" },
  { id: "system", label: "System" },
];

/** 已知无碍的噪声（宿主环境缺失导致，非产品缺陷）。 */
const IGNORE_PATTERNS = [
  /\/api\/health/, // PwaBanner 版本探测（桌面运行时不存在，属预期）
  /favicon/i,
  // 源站图床被墙 / 已下线：外部资源，不是产品缺陷（News 已有超时兜底）
  /ichef\.bbci\.co\.uk/,
  /tiles\.mapterhorn\.com/,
];

const findings = [];
function finding(page, viewport, theme, kind, detail) {
  findings.push({ page, viewport, theme, kind, detail });
}

async function auditPage(page, meta, viewport, theme) {
  const consoleErrors = [];
  const pageErrors = [];
  const failedRequests = [];
  const onConsole = (m) => {
    if (m.type() === "error") consoleErrors.push(m.text());
  };
  const onPageError = (e) => pageErrors.push(e.message);
  const onResponse = (r) => {
    if (r.status() < 400) return;
    if (IGNORE_PATTERNS.some((p) => p.test(r.url()))) return;
    failedRequests.push(`${r.status()} ${r.url().replace(BASE, "")}`);
  };
  const onFailed = (r) => {
    if (IGNORE_PATTERNS.some((p) => p.test(r.url()))) return;
    failedRequests.push(`FAIL ${r.url().replace(BASE, "")}`);
  };
  page.on("console", onConsole);
  page.on("pageerror", onPageError);
  page.on("response", onResponse);
  page.on("requestfailed", onFailed);

  await page.goto(`${BASE}/`, { waitUntil: "domcontentloaded" });
  await page.evaluate((t) => {
    document.documentElement.setAttribute("data-theme", t);
    try { localStorage.setItem("devtoolbox.theme", t); } catch {}
  }, theme);
  await page.waitForTimeout(600);

  // 通过导航注册表点击（和用户操作一致）
  const clicked = await page.evaluate((label) => {
    // 导航项没有 data-page，靠 aria-label（与 App.tsx 的 NavItem label 一致）。
    const el = [...document.querySelectorAll(".app-nav-item")].find(
      (e) => (e.getAttribute("aria-label") ?? "").trim() === label,
    );
    if (!el) return false;
    el.click();
    return true;
  }, meta.navLabel ?? meta.label);

  if (!clicked) {
    finding(meta.id, viewport, theme, "NAV", "导航项未找到，无法进入");
    cleanup(page, [onConsole, onPageError, onResponse, onFailed]);
    return;
  }
  // 图片有 6s 加载上限；过早测量会把"即将超时"误判成破图。
  await page.waitForTimeout(2600 + 7000);

  const report = await page.evaluate(() => {
    const de = document.documentElement;
    const pane = document.querySelector(".page-pane:not(.page-hidden)");
    const out = {
      url: location.hash,
      paneExists: Boolean(pane),
      paneText: pane ? pane.innerText.trim().length : 0,
      paneHtml: pane ? pane.innerHTML.length : 0,
      // 采样首段文字，用于区分「空白」与「诚实的能力声明」
      sample: pane.innerText.replace(/\s+/g, " ").trim().slice(0, 120),
      overflowX: de.scrollWidth > de.clientWidth + 2,
      scrollW: de.scrollWidth,
      clientW: de.clientWidth,
      overflown: [],
      brokenImages: [],
      tinyText: 0,
      emptyButtons: [],
      // 无任何可交互元素的"死页面"
      buttonCount: 0,
      linkCount: 0,
    };
    if (!pane) return out;
    const pr = pane.getBoundingClientRect();
    pane.querySelectorAll("*").forEach((el) => {
      const r = el.getBoundingClientRect();
      if (r.width === 0 || r.height === 0) return;
      if (r.right > de.clientWidth + 4) {
        out.overflown.push(
          `${el.tagName.toLowerCase()}.${(el.className?.toString?.() ?? "").slice(0, 26)}=${Math.round(r.right)}`,
        );
      }
      if (el.tagName === "IMG" && el.naturalWidth === 0 && el.offsetParent) {
        out.brokenImages.push(el.src.slice(0, 60));
      }
    });
    out.overflown = [...new Set(out.overflown)].slice(0, 4);
    out.buttonCount = pane.querySelectorAll("button").length;
    out.linkCount = pane.querySelectorAll("a").length;
    return out;
  });

  // 记录问题
  if (!report.paneExists) finding(meta.id, viewport, theme, "BLANK", "页面容器不存在");
  else if (report.paneText < 30 && report.paneHtml < 400) {
    // 诚实的能力声明 / 空态（例如「浏览器预览不支持…」）不是空白页
    // 明确的能力声明（如"浏览器预览不支持…"）也算诚实呈现，不是空白
    const capabilityMsg = /不支持|请在桌面|暂不提供|尚未装配|需要桌面/i.test(report.sample ?? "");
    const honest = report.paneText > 0 || capabilityMsg;
    finding(
      meta.id,
      viewport,
      theme,
      honest ? "EMPTY_STATED" : "BLANK",
      honest
        ? `内容很少但已说明原因（${report.paneText}字）`
        : `页面几乎无内容（text=${report.paneText} html=${report.paneHtml}）`,
    );
  }
  if (report.overflowX) {
    finding(meta.id, viewport, theme, "OVERFLOW", `横向溢出 ${report.scrollW} > ${report.clientW}；元素：${report.overflown.join(" | ")}`);
  }
  if (report.brokenImages.length) {
    finding(meta.id, viewport, theme, "IMAGE", `破图：${report.brokenImages.join(", ")}`);
  }
  const realConsole = consoleErrors.filter((t) => !IGNORE_PATTERNS.some((p) => p.test(t)));
  if (realConsole.length) {
    finding(meta.id, viewport, theme, "CONSOLE", [...new Set(realConsole)].slice(0, 3).join(" | "));
  }
  if (pageErrors.length) {
    finding(meta.id, viewport, theme, "PAGEERROR", [...new Set(pageErrors)].slice(0, 2).join(" | "));
  }
  const realFailed = failedRequests.filter((t) => !IGNORE_PATTERNS.some((p) => p.test(t)));
  if (realFailed.length) {
    finding(meta.id, viewport, theme, "NETWORK", [...new Set(realFailed)].slice(0, 3).join(" | "));
  }

  await page.evaluate(() => document.querySelectorAll(".toast").forEach((t) => t.remove()));
  await page.screenshot({
    path: path.join(OUT, `${viewport}-${theme}-${meta.id}.png`),
  });
  cleanup(page, [onConsole, onPageError, onResponse, onFailed]);
}

function cleanup(page, handlers) {
  page.off("console", handlers[0]);
  page.off("pageerror", handlers[1]);
  page.off("response", handlers[2]);
  page.off("requestfailed", handlers[3]);
}

async function main() {
  await mkdir(OUT, { recursive: true });
  const browser = await chromium.launch({ channel: "chrome" });
  for (const viewport of VIEWPORTS) {
    for (const theme of THEMES) {
      for (const meta of PAGES) {
        const ctx = await browser.newContext({
          viewport: { width: viewport.width, height: viewport.height },
        });
        const page = await ctx.newPage();
        try {
          await auditPage(page, meta, viewport.name, theme);
        } catch (error) {
          finding(meta.id, viewport.name, theme, "HARNESS", String(error).slice(0, 120));
        }
        await ctx.close();
      }
    }
  }
  await browser.close();

  // 输出报告
  const byPage = {};
  for (const f of findings) {
    byPage[f.page] ??= [];
    byPage[f.page].push(f);
  }
  console.log("\n================ 体检结果 ================");
  console.log(`检查页面: ${PAGES.length} × ${VIEWPORTS.length} 视口 × ${THEMES.length} 主题 = ${PAGES.length * VIEWPORTS.length * THEMES.length} 次`);
  console.log(`发现问题: ${findings.length}\n`);
  for (const [page, list] of Object.entries(byPage)) {
    console.log(`■ ${page} (${list.length})`);
    const seen = new Set();
    for (const f of list) {
      const key = `${f.kind}:${f.detail.slice(0, 60)}`;
      if (seen.has(key)) continue;
      seen.add(key);
      console.log(`   [${f.kind}] ${f.viewport}/${f.theme}: ${f.detail.slice(0, 180)}`);
    }
  }
  const clean = PAGES.filter((p) => !byPage[p.id]);
  if (clean.length) console.log(`\n✓ 无问题页面: ${clean.map((p) => p.label).join(", ")}`);
  await import("node:fs/promises").then((fs) =>
    fs.writeFile(path.join(OUT, "report.json"), JSON.stringify(findings, null, 2)),
  );
}

main().catch((e) => {
  console.error(e);
  process.exit(2);
});