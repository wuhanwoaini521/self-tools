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
  resolve: {
    // 单一 React 实例：three/fiber 等 peer 依赖不得引入第二份 react/react-dom
    // （否则运行时抛 React #321 "Invalid hook call"）。
    dedupe: ["react", "react-dom", "react/jsx-runtime", "react/jsx-dev-runtime"],
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
