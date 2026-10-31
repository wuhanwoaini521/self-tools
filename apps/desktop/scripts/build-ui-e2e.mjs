import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const desktopDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const uiDir = resolve(desktopDir, "ui");
const viteCli = resolve(uiDir, "node_modules", "vite", "bin", "vite.js");
const buildArgs = [viteCli, "build"];
if (process.env.DEVTOOLBOX_E2E_UI_OUT_DIR) {
  buildArgs.push("--outDir", resolve(process.env.DEVTOOLBOX_E2E_UI_OUT_DIR));
}
const result = spawnSync(process.execPath, buildArgs, {
  cwd: uiDir,
  env: { ...process.env, VITE_TAURI_E2E: "1" },
  stdio: "inherit",
});

if (result.error) throw result.error;
process.exit(result.status ?? 1);
