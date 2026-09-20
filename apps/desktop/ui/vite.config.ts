import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { qaBridge } from "./vite.qa-bridge";

export default defineConfig({
  plugins: [react(), qaBridge()],
  clearScreen: false,
  // MapLibre bundles its own ES module worker. Keep it out of Vite's
  // dependency optimizer so the worker remains resolvable in dev mode.
  optimizeDeps: {
    exclude: ["maplibre-gl"],
  },
  server: {
    strictPort: true,
    // Visual QA 代理：/__qa/** → 本地只读 History HTTP 服务（真实 DuckDB）。
    proxy: {
      "/__qa": {
        target: "http://127.0.0.1:8080",
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/__qa/, ""),
      },
    },
  },
});
