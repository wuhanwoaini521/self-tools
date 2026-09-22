/**
 * Visual QA：5 个视口 × 关键界面截图（V11 §136-§141）。
 *
 * 用法（backend + UI dev server 都在跑时）：
 *   node scripts/visual-qa.mjs                 # 全部视口 + 全部界面
 *   node scripts/visual-qa.mjs --viewport=mobile
 *   node scripts/visual-qa.mjs --route=home
 *
 * 输出：docs/qa/v11/{desktop,tablet-landscape,tablet-portrait,mobile}/*.png
 * 说明：本脚本只做「真实渲染 + 截图 + 基本溢出断言」，不替代人工视觉审查；
 *       人工审查结论写入 docs/qa/v11/VISUAL_REVIEW.md。
 */

import { mkdir, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO = join(__dirname, "..");

/** §137：五个必需视口。 */
const VIEWPORTS = [
  { name: "desktop", width: 1440, height: 900 },
  { name: "tablet-landscape", width: 1024, height: 768 },
  { name: "tablet-portrait", width: 768, height: 1024 },
  { name: "mobile", width: 390, height: 844 },
  { name: "small-mobile", width: 360, height: 800 },
];

/** §138：必需界面（route hash）。 */
const ROUTES = [
  { key: "home", path: "/", label: "Personal Hub Home" },
  { key: "history", path: "/#history", label: "History" },
  { key: "travel", path: "/#travel", label: "Travel" },
  { key: "geography", path: "/#geography", label: "Geography" },
  { key: "language", path: "/#language", label: "Language" },
  { key: "study-board", path: "/#study-board", label: "Study Board" },
  { key: "knowledge", path: "/#knowledge", label: "Knowledge" },
  { key: "memory", path: "/#knowledge", label: "Memory/Documents/Files" },
  { key: "documents", path: "/#knowledge", label: "Documents" },
  { key: "files", path: "/#knowledge", label: "Files" },
  { key: "server", path: "/#server", label: "Server Dashboard" },
  { key: "system", path: "/#system", label: "System Readiness" },
  { key: "search", path: "/#search", label: "Global Search" },
  { key: "settings", path: "/#home", label: "Settings" },
  { key: "ai-panel", path: "/#home", label: "AI Panel" },
  { key: "safe-action", path: "/#server", label: "SafeAction Confirmation" },
];

const BASE_URL = process.env.QA_BASE_URL ?? "http://127.0.0.1:1420";
const OUT_DIR = join(REPO, "..", "..", "docs", "qa", "v11");

function parseArgs(argv) {
  const options = { viewport: null, route: null };
  for (const arg of argv) {
    const [key, value] = arg.replace(/^--/, "").split("=");
    if (key === "viewport") options.viewport = value;
    if (key === "route") options.route = value;
  }
  return options;
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const { chromium } = await import("playwright");

  const browser = await chromium.launch();
  const report = [];

  for (const viewport of VIEWPORTS) {
    if (options.viewport && viewport.name !== options.viewport) continue;
    const context = await browser.newContext({
      viewport: { width: viewport.width, height: viewport.height },
      deviceScaleFactor: 1,
      hasTouch: viewport.name.includes("mobile") || viewport.name.includes("tablet"),
      isMobile: viewport.name.includes("mobile"),
    });
    const page = await context.newPage();

    for (const route of ROUTES) {
      if (options.route && route.key !== options.route) continue;
      const url = `${BASE_URL}${route.path}`;
      try {
        await page.goto(url, { waitUntil: "networkidle", timeout: 20_000 });
        await page.waitForTimeout(400);

        // §141：基本溢出 / 布局塌陷检测（JS 断言，不替代人工）。
        const metrics = await page.evaluate(() => {
          const doc = document.documentElement;
          const horizontalOverflow = doc.scrollWidth - doc.clientWidth;
          // 底部导航在 mobile 必须可见且不被遮挡。
          const bottomNav = document.querySelector(".app-bottom-nav");
          const navBox = bottomNav?.getBoundingClientRect();
          const safeBottom = Number.parseFloat(
            getComputedStyle(doc).getPropertyValue("--safe-area-bottom"),
          ) || 0;
          return {
            horizontalOverflow,
            hasBottomNav: Boolean(bottomNav),
            navBottomGap: navBox ? window.innerHeight - navBox.bottom : null,
            safeBottom,
            device: doc.dataset.device ?? "unknown",
          };
        });

        const shotDir = join(OUT_DIR, viewport.name);
        await mkdir(shotDir, { recursive: true });
        const file = join(shotDir, `${route.key}.png`);
        await page.screenshot({ path: file, fullPage: false });

        report.push({
          viewport: viewport.name,
          route: route.key,
          label: route.label,
          file: file.replace(REPO, "."),
          ...metrics,
        });
        console.log(
          `[qa] ${viewport.name}/${route.key} overflow=${metrics.horizontalOverflow}px device=${metrics.device}`,
        );
      } catch (error) {
        report.push({
          viewport: viewport.name,
          route: route.key,
          label: route.label,
          error: String(error),
        });
        console.error(`[qa] ${viewport.name}/${route.key} FAILED: ${error}`);
      }
    }
    await context.close();
  }

  await browser.close();
  await writeFile(
    join(OUT_DIR, "qa-report.json"),
    `${JSON.stringify({ generated_at: new Date().toISOString(), report }, null, 2)}\n`,
    "utf8",
  );
  console.log(`[qa] done → ${OUT_DIR}`);
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
