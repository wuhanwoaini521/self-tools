import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const desktopDir = resolve(here, "..");
const uiDir = resolve(desktopDir, "ui");
const tauriConfig = JSON.stringify({
  build: { devUrl: "http://127.0.0.1:1421" },
  app: { withGlobalTauri: true, security: { capabilities: ["default", "webdriver"] } },
});
const uiBuild = spawnSync(process.execPath, [resolve(here, "build-ui-e2e.mjs")], {
  cwd: desktopDir,
  env: { ...process.env, VITE_TAURI_E2E: "1" },
  stdio: "inherit",
});
if (uiBuild.error) throw uiBuild.error;
if (uiBuild.status !== 0) process.exit(uiBuild.status ?? 1);

// Cargo directly keeps the E2E context in dev mode, so WebView loads this
// freshly built UI from the private Vite preview below instead of stale assets.
const result = spawnSync("cargo", ["build", "--manifest-path", "../../Cargo.toml", "-p", "devtoolbox-desktop", "--features", "e2e"], {
  cwd: desktopDir,
  env: { ...process.env, TAURI_CONFIG: tauriConfig },
  stdio: "inherit",
});

if (result.error) throw result.error;
process.exit(result.status ?? 1);
