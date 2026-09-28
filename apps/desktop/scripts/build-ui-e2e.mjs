import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const desktopDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const uiDir = resolve(desktopDir, "ui");
const viteCli = resolve(uiDir, "node_modules", "vite", "bin", "vite.js");
const result = spawnSync(process.execPath, [viteCli, "build"], {
  cwd: uiDir,
  env: { ...process.env, VITE_TAURI_E2E: "1" },
  stdio: "inherit",
});

if (result.error) throw result.error;
process.exit(result.status ?? 1);
