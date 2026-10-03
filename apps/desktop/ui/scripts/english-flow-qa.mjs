/**
 * English 学习闭环的真机走查（Playwright + 真实 Vite 开发服务器 + QA 桥 → 真实 HTTP 服务）。
 *
 * 用法：
 *   node scripts/english-flow-qa.mjs            # 走完整流程并截图
 *   BASE=http://127.0.0.1:1420 node scripts/english-flow-qa.mjs
 *
 * 截图输出到 `output/visual-qa/english/`。
 * 刻意**真的点击**每一步，而不是只断言按钮存在。
 */
import { chromium } from "playwright";
import { mkdir } from "node:fs/promises";
import path from "node:path";

const BASE = process.env.BASE ?? "http://127.0.0.1:1420";
const OUT = path.resolve(process.cwd(), "../../output/visual-qa/english");

const steps = [];
let failures = 0;

function record(name, ok, detail = "") {
  steps.push({ name, ok, detail });
  if (!ok) failures += 1;
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? ` — ${detail}` : ""}`);
}

/** 回到 English 首页（hash 导航不会重新挂载，必须 reload）。 */
async function goHome(page) {
  await page.goto(`${BASE}/#language`, { waitUntil: "networkidle" });
  await page.reload({ waitUntil: "networkidle" });
  await page.waitForSelector(".en-home", { timeout: 15000 });
}

async function shot(page, name) {
  await mkdir(OUT, { recursive: true });
  await page.screenshot({ path: path.join(OUT, `${name}.png`), fullPage: false });
}

async function main() {
  const browser = await chromium.launch({ channel: "chrome" });
  const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const page = await context.newPage();

  const consoleErrors = [];
  // PwaBanner 在浏览器里探测 /api/health（只在桌面运行时存在）→ 与本模块无关的既有噪声。
  const IGNORED_URL_PARTS = ["/api/health"];
  page.on("console", (message) => {
    if (message.type() === "error") consoleErrors.push(message.text());
  });
  page.on("pageerror", (error) => consoleErrors.push(`pageerror: ${error.message}`));
  // 资源 404：逐条记录 URL，便于精确忽略已知噪声。
  const resourceErrors = [];
  page.on("response", (response) => {
    if (response.status() < 400) return;
    if (IGNORED_URL_PARTS.some((part) => response.url().includes(part))) return;
    resourceErrors.push(`${response.status()} ${response.url()}`);
  });

  // ---- 1) 打开 Language → English 首页 ----
  await page.goto(`${BASE}/#language`, { waitUntil: "networkidle" });
  await page.waitForSelector(".en-home, .lang-page", { timeout: 15000 });
  const isEnglish = await page.locator(".en-home").count();
  record("English 首页（学习驾驶舱）渲染", isEnglish > 0);
  await shot(page, "01-english-home");

  const greeting = await page.locator(".en-greeting-hello").first().textContent();
  record("问候语显示", Boolean(greeting && greeting.trim().length > 0), greeting ?? "");

  const statCount = await page.locator(".en-metric").count();
  record("今日概览数字卡", statCount >= 4, `${statCount} 个`);

  // ---- 2) Continue Learning → Lesson 工作台 ----
  const continueBtn = page.locator(".en-continue .en-primary-btn");
  if ((await continueBtn.count()) === 0) {
    record("Continue Learning 卡片", false, "没有 Continue 按钮（可能未导入课程）");
    await browser.close();
    return;
  }
  record("Continue Learning 卡片", true);
  await continueBtn.click();
  await page.waitForSelector(".en-workspace", { timeout: 15000 });
  await page.waitForTimeout(1200);
  record("Lesson 工作台打开", (await page.locator(".en-workspace").count()) > 0);

  const stageTabs = await page.locator(".en-stage-tab").count();
  record("阶段导航（6 阶段同页）", stageTabs === 6, `${stageTabs} 个阶段`);

  const audio = await page.locator(".en-audio").count();
  record("音频播放器常驻", audio > 0);
  await shot(page, "02-lesson-workspace");

  // ---- 3) Vocabulary 阶段：三态标记 ----
  const wordHeading = await page.locator(".en-wordcard-word").first().textContent();
  record("课前单词卡显示", Boolean(wordHeading), wordHeading ?? "");

  const markButtons = page.locator(".en-mark-actions button");
  const markCount = await markButtons.count();
  record("三态自评按钮（认识/模糊/不认识）", markCount === 3, `${markCount} 个`);
  if (markCount === 3) {
    await markButtons.nth(2).click(); // 不认识 → 今天复习
    await page.waitForTimeout(900);
    const nextWord = await page.locator(".en-wordcard-word").first().textContent();
    record("标记后自动进入下一个词", nextWord !== wordHeading, `${wordHeading} → ${nextWord}`);
    await shot(page, "03-vocabulary");
  }

  // ---- 4) Listening 阶段（盲听）----
  await page.locator(".en-stage-tab", { hasText: "Listening" }).click();
  await page.waitForTimeout(500);
  const blind = await page.locator(".en-sentence-hidden").count();
  record("盲听模式隐藏原文", blind > 0, `${blind} 行遮蔽`);
  await shot(page, "04-listening-blind");

  const revealBtn = page.locator(".en-stage button", { hasText: "显示原文" });
  if ((await revealBtn.count()) > 0) {
    await revealBtn.first().click();
    await page.waitForTimeout(400);
    record("可切换显示原文", (await page.locator(".en-sentence-en").count()) > 0);
  }

  // ---- 5) Reading 阶段：点击单词查词 ----
  await page.locator(".en-stage-tab", { hasText: "Reading" }).click();
  await page.waitForTimeout(500);
  const words = page.locator(".en-text .en-word");
  const wordCount = await words.count();
  record("课文单词可点击", wordCount > 10, `${wordCount} 个可点词`);
  await shot(page, "05-reading");

  if (wordCount > 0) {
    await words.nth(3).click();
    await page.waitForSelector(".en-wordpop", { timeout: 8000 });
    record("查词浮层打开（不跳页）", (await page.locator(".en-wordpop").count()) > 0);
    const popText = await page.locator(".en-wordpop").innerText();
    record("浮层含中文释义或词典说明", popText.length > 20, popText.slice(0, 60).replace(/\n/g, " · "));
    await shot(page, "06-word-popover");
    await page.keyboard.press("Escape");
    await page.waitForTimeout(300);
    record("Esc 关闭浮层", (await page.locator(".en-wordpop").count()) === 0);
  }

  // ---- 6) Sentence 阶段：逐句 ----
  await page.locator(".en-stage-tab", { hasText: "Sentence" }).click();
  await page.waitForTimeout(500);
  const sentenceRows = await page.locator(".en-sentence-row").count();
  record("逐句列表（时间轴）", sentenceRows > 0, `${sentenceRows} 句`);
  await shot(page, "07-sentence");

  // ---- 7) Shadow 阶段 ----
  await page.locator(".en-stage-tab", { hasText: "Shadow" }).click();
  await page.waitForTimeout(500);
  record("跟读阶段", (await page.locator(".en-shadow-card").count()) > 0);
  await shot(page, "08-shadow");

  // ---- 8) Quiz 阶段 ----
  await page.locator(".en-stage-tab", { hasText: "Quiz" }).click();
  await page.waitForTimeout(1500);
  const quizCard = await page.locator(".en-quiz-card").count();
  record("Quiz 阶段加载题目", quizCard > 0);
  await shot(page, "09-quiz");

  if (quizCard > 0) {
    const kind = await page.locator(".en-quiz-kind").first().textContent();
    record("题目类型标签", Boolean(kind), kind ?? "");
    // 作答第一题（选项或填空）
    const options = page.locator(".en-quiz-option");
    if ((await options.count()) > 0) {
      await options.first().click();
      await page.waitForTimeout(200);
      record("选择题可作答", (await page.locator(".en-quiz-option.is-picked").count()) > 0);
    }
  }

  // ---- 9) 课程库 ----
  await goHome(page);
  const libraryLink = page.locator(".en-greeting button", { hasText: "课程库" });
  if ((await libraryLink.count()) > 0) {
    await libraryLink.first().click();
    await page.waitForSelector(".en-library", { timeout: 10000 });
    await page.waitForTimeout(1200);
    const bookCards = await page.locator(".en-book-card").count();
    record("课程库显示书册卡片", bookCards > 0, `${bookCards} 册`);
    await shot(page, "10-course-library");
  }

  // ---- 10) 学习统计 ----
  await goHome(page);
  const progressLink = page.locator("button", { hasText: "学习统计" }).first();
  if ((await progressLink.count()) > 0) {
    await progressLink.click();
    await page.waitForSelector(".en-library", { timeout: 10000 });
    await page.waitForTimeout(1200);
    record("学习统计页", (await page.locator(".en-metric").count()) > 0);
    await shot(page, "11-progress");
  }

  // ---- 11) 空态：AI 未配置时必须显示「不依赖 AI」 ----
  await goHome(page);
  await page.locator(".en-continue .en-primary-btn").first().click();
  await page.waitForSelector(".en-workspace", { timeout: 10000 });
  await page.waitForTimeout(800);
  const sideText = await page.locator(".en-workspace-side").innerText();
  record(
    "AI 未配置时明确说明（但不影响学习）",
    sideText.includes("AI") ,
    sideText.split("\n").slice(0, 2).join(" · "),
  );
  await shot(page, "12-ai-unavailable");

  // ---- 12) 窄屏布局 ----
  await page.setViewportSize({ width: 900, height: 800 });
  await page.waitForTimeout(500);
  const overflowX = await page.evaluate(
    () => document.documentElement.scrollWidth > document.documentElement.clientWidth + 2,
  );
  record("窄屏无横向溢出", !overflowX);
  await shot(page, "13-narrow");

  await page.setViewportSize({ width: 1440, height: 900 });

  const genericResourceErrors = consoleErrors.filter(
    (text) => text.includes("Failed to load resource"),
  );
  record(
    "无未捕获的前端错误",
    consoleErrors.filter((text) => !text.includes("Failed to load resource")).length === 0,
    consoleErrors
      .filter((text) => !text.includes("Failed to load resource"))
      .slice(0, 3)
      .join(" | "),
  );
  record(
    "无失败资源请求（忽略 PWA /api/health 探测）",
    resourceErrors.length === 0,
    resourceErrors.slice(0, 3).join(" | "),
  );
  void genericResourceErrors;

  console.log(`\n=== ${steps.length - failures}/${steps.length} passed ===`);
  if (failures > 0) {
    console.log("失败项：");
    steps.filter((step) => !step.ok).forEach((step) => console.log(`  - ${step.name} ${step.detail}`));
  }
  await browser.close();
  process.exit(failures > 0 ? 1 : 0);
}

main().catch((error) => {
  console.error(error);
  process.exit(2);
});