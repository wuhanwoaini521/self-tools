import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { spawn } from "node:child_process";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "../..");
const reportDir = join(repoRoot, "output", "desktop-e2e");
const testDataDir = mkdtempSync(join(tmpdir(), "devtoolbox-e2e-"));
const fixtureDir = join(testDataDir, "fixtures");
const markdownFixture = join(fixtureDir, "keyboard.md");
const uiDir = resolve(here, "ui");
const viteCli = join(uiDir, "node_modules", "vite", "bin", "vite.js");
let previewServer;

// Offline developer machines can pin an already installed, matching EdgeDriver.
// The service discovers drivers through PATH; keep the override opt-in.
if (process.env.DEVTOOLBOX_EDGEDRIVER_PATH) {
  process.env.PATH = `${dirname(process.env.DEVTOOLBOX_EDGEDRIVER_PATH)};${process.env.PATH}`;
}

mkdirSync(fixtureDir, { recursive: true });
writeFileSync(markdownFixture, "# Keyboard fixture\n\n- [ ] persist this task\n", "utf8");
mkdirSync(reportDir, { recursive: true });
process.env.DEVTOOLBOX_E2E_DATA_DIR = testDataDir;
process.env.DEVTOOLBOX_E2E_MARKDOWN_FIXTURE = markdownFixture;
// Isolate WebView2's HTTP/custom-scheme cache as well as Tauri app data so an
// older frontend bundle from a developer run cannot leak into E2E sessions.
process.env.WEBVIEW2_USER_DATA_FOLDER = join(testDataDir, "webview2");
process.env.TAURI_WEBDRIVER_PORT ??= "4445";

export const config = {
  runner: "local",
  specs: ["./e2e/**/*.e2e.mjs"],
  maxInstances: 1,
  maxInstancesPerCapability: 1,
  specFileParallelism: false,
  capabilities: [{
    browserName: "tauri",
    "tauri:options": {
      application: join(repoRoot, "target", "debug", "devtoolbox-desktop.exe"),
    },
  }],
  logLevel: "warn",
  bail: 0,
  baseUrl: "tauri://localhost",
  waitforTimeout: 8_000,
  connectionRetryTimeout: 120_000,
  connectionRetryCount: 1,
  services: [["@wdio/tauri-service", {
    appBinaryPath: join(repoRoot, "target", "debug", "devtoolbox-desktop.exe"),
    driverProvider: "embedded",
    embeddedPort: Number(process.env.TAURI_WEBDRIVER_PORT),
    captureBackendLogs: true,
    captureFrontendLogs: true,
  }]],
  framework: "mocha",
  reporters: [["spec", { writeStream: join(reportDir, "spec.log") }]],
  onPrepare: async () => {
    previewServer = spawn(process.execPath, [viteCli, "preview", "--host", "127.0.0.1", "--port", "1421", "--strictPort"], {
      cwd: uiDir,
      stdio: "ignore",
      windowsHide: true,
    });
    const deadline = Date.now() + 30_000;
    while (Date.now() < deadline) {
      if (previewServer.exitCode !== null) throw new Error(`E2E preview server exited with ${previewServer.exitCode}`);
      try {
        if ((await fetch("http://127.0.0.1:1421/")).ok) return;
      } catch {
        // Vite is still starting.
      }
      await new Promise((resolvePromise) => setTimeout(resolvePromise, 250));
    }
    throw new Error("E2E Vite preview did not become ready on port 1421");
  },
  mochaOpts: {
    ui: "bdd",
    timeout: 90_000,
  },
  afterTest: async (test, _context, { error }) => {
    if (!error) return;
    const safeName = String(test.fullTitle ?? test.title ?? "failed-test")
      .replace(/[^a-z0-9-_]+/gi, "_").slice(0, 100);
    try {
      await browser.saveScreenshot(join(reportDir, `${Date.now()}-${safeName}.png`));
    } catch (screenshotError) {
      process.stderr.write(`[e2e] screenshot failed for ${safeName}: ${screenshotError}\n`);
    }
  },
  onComplete: () => {
    previewServer?.kill();
    try {
      rmSync(testDataDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
    } catch (error) {
      process.stderr.write(`[e2e] temporary test data remains at ${testDataDir}: ${error}\n`);
    }
  },
};
