import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { mkdir, writeFile } from "node:fs/promises";
import { spawn, spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { delimiter } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "../../..");
const uiDir = join(repoRoot, "apps", "desktop", "ui");
const viteCli = join(uiDir, "node_modules", "vite", "bin", "vite.js");
const nodeExecutable = [
  process.execPath,
  ...String(process.env.PATH ?? "").split(delimiter).map((directory) => join(directory, process.platform === "win32" ? "node.exe" : "node")),
].find((candidate) => existsSync(candidate));
if (!nodeExecutable) throw new Error("Could not find a runnable Node.js executable for Vite");
const viewports = [
  { name: "desktop", width: 1440, height: 900 },
  { name: "tablet-landscape", width: 1024, height: 768 },
  { name: "tablet-portrait", width: 768, height: 1024 },
  { name: "mobile", width: 390, height: 844 },
  { name: "small-mobile", width: 360, height: 800 },
];
const pages = [
  ["home", "Home"], ["markdown", "Markdown"], ["rss", "RSS"], ["news", "News"],
  ["travel", "Travel"], ["geography", "Geography"], ["history", "History"],
  ["language", "Language"], ["knowledge", "Knowledge"], ["study-board", "Study"],
  ["server", "Server"], ["system", "System"], ["search", "Search"],
];
let baseUrl = process.env.QA_BASE_URL ?? "";
const outDir = process.env.QA_OUT_DIR ?? join(repoRoot, "output", "visual-qa");
const report = [];
let viteProcess;
let browser;

async function startViteIfNeeded() {
  if (process.env.QA_BASE_URL) return;
  viteProcess = spawn(nodeExecutable, [viteCli, "--host", "127.0.0.1", "--port", "0"], {
    cwd: uiDir,
    stdio: ["ignore", "pipe", "pipe"],
    windowsHide: true,
  });
  let spawnError;
  viteProcess.on("error", (error) => { spawnError = error; });
  viteProcess.stdout.on("data", (chunk) => {
    const text = chunk.toString();
    process.stdout.write(text);
    const localUrl = text.match(/Local:\s+(https?:\/\/\S+)/)?.[1];
    if (localUrl) baseUrl = localUrl.replace(/\/$/, "");
  });
  viteProcess.stderr.on("data", (chunk) => process.stderr.write(chunk));

  const deadline = Date.now() + 30_000;
  while (!baseUrl && Date.now() < deadline) {
    if (spawnError) throw spawnError;
    if (viteProcess.exitCode !== null) throw new Error(`Vite exited with ${viteProcess.exitCode}`);
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 250));
  }
  if (!baseUrl) throw new Error("Vite did not report its local URL within 30 seconds");
}

async function inspectPage(page, viewport, route, label) {
  await page.goto(`${baseUrl}/#${route}`, { waitUntil: "domcontentloaded" });
  const pane = page.locator(".page-pane:not(.page-hidden)");
  await pane.waitFor({ state: "visible", timeout: 10_000 });
  const metrics = await page.evaluate(() => {
    const root = document.documentElement;
    const pane = document.querySelector(".page-pane:not(.page-hidden)");
    const paneButtons = [...(pane?.querySelectorAll("button:disabled") ?? [])].length;
    const unnamedButtons = [...(pane?.querySelectorAll("button") ?? [])]
      .filter((button) => !button.disabled && !button.innerText.trim() &&
        !button.getAttribute("aria-label") && !button.title)
      .map((button) => button.outerHTML.slice(0, 180));
    return {
      horizontalOverflow: root.scrollWidth - root.clientWidth,
      activePaneText: pane?.innerText.slice(0, 120) ?? "",
      visibleButtons: [...(pane?.querySelectorAll("button") ?? [])]
        .filter((button) => button.getClientRects().length > 0).length,
      disabledButtons: paneButtons,
      unnamedButtons,
      device: root.dataset.device ?? "unknown",
    };
  });

  assert.equal(metrics.horizontalOverflow, 0, `${viewport.name}/${route}: horizontal overflow`);
  assert.notEqual(metrics.activePaneText, "", `${viewport.name}/${route}: empty page pane`);
  assert.deepEqual(metrics.unnamedButtons, [], `${viewport.name}/${route}: unnamed enabled buttons`);
  if (viewport.width < 768) {
    const navTargets = await page.locator(".app-bottom-nav button").evaluateAll((buttons) =>
      buttons.map((button) => {
        const { width, height } = button.getBoundingClientRect();
        return { width, height, text: button.innerText };
      }),
    );
    assert.ok(navTargets.length > 0, `${viewport.name}/${route}: missing bottom navigation`);
    assert.ok(navTargets.every((target) => target.width >= 44 && target.height >= 44),
      `${viewport.name}/${route}: bottom navigation target smaller than 44px`);
  }

  const screenshot = join(outDir, viewport.name, `${route}.png`);
  await mkdir(dirname(screenshot), { recursive: true });
  await page.screenshot({ path: screenshot, fullPage: false });
  report.push({ viewport: viewport.name, route, label, screenshot, ...metrics });
}

try {
  await mkdir(outDir, { recursive: true });
  await startViteIfNeeded();
  browser = await chromium.launch({ headless: true });

  for (const viewport of viewports) {
    const context = await browser.newContext({
      viewport: { width: viewport.width, height: viewport.height },
      deviceScaleFactor: 1,
      hasTouch: viewport.width < 1180,
      isMobile: viewport.width < 768,
      serviceWorkers: "block",
    });
    const page = await context.newPage();
    const errors = [];
    await page.route("**/api/health", (route) => route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ status: "ok", version: "0.1.0" }),
    }));
    page.on("pageerror", (error) => errors.push(`${page.url()}: ${error.message}`));
    page.on("console", (message) => {
      if (message.type() === "error") errors.push(`${page.url()}: ${message.text()}`);
    });
    page.on("response", (response) => {
      if (response.status() >= 400) {
        errors.push(`${page.url()}: ${response.status()} ${response.url()}`);
      }
    });

    for (const [route, label] of pages) await inspectPage(page, viewport, route, label);

    await page.goto(`${baseUrl}/#home`, { waitUntil: "domcontentloaded" });
    await page.locator("button[title='Settings']").click();
    await page.locator(".settings-dialog").waitFor({ state: "visible" });
    const settingsShot = join(outDir, viewport.name, "settings.png");
    await page.screenshot({ path: settingsShot, fullPage: false });
    report.push({ viewport: viewport.name, route: "settings", label: "Settings dialog", screenshot: settingsShot });
    await page.locator("button[title='关闭设置']").click();

    await page.locator("button[title='Ask AI']").click();
    await page.locator(".ai-panel").waitFor({ state: "visible" });
    const aiShot = join(outDir, viewport.name, "ai-panel.png");
    await page.screenshot({ path: aiShot, fullPage: false });
    report.push({ viewport: viewport.name, route: "ai-panel", label: "AI panel", screenshot: aiShot });
    await page.locator(".ai-panel button[title='关闭']").click();
    assert.deepEqual(errors, [], `${viewport.name}: browser errors`);
    await context.close();
  }

  await writeFile(join(outDir, "qa-report.json"), `${JSON.stringify({ generated_at: new Date().toISOString(), report }, null, 2)}\n`, "utf8");
  console.log(`[visual-qa] PASS: ${report.length} rendered states across ${viewports.length} viewports → ${outDir}`);
} catch (error) {
  await writeFile(join(outDir, "qa-report.json"), `${JSON.stringify({ generated_at: new Date().toISOString(), error: String(error), report }, null, 2)}\n`, "utf8");
  console.error(`[visual-qa] FAILED: ${error}`);
  process.exitCode = 1;
} finally {
  await browser?.close();
  if (viteProcess && viteProcess.exitCode === null) {
    if (process.platform === "win32" && viteProcess.pid) {
      spawnSync("taskkill", ["/PID", String(viteProcess.pid), "/T", "/F"], { stdio: "ignore" });
    } else {
      viteProcess.kill();
    }
  }
}
