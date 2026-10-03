import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { qaBridge } from "./vite.qa-bridge";

export default defineConfig({
  plugins: [react(), qaBridge()],
  clearScreen: false,
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
    proxy: {
      // 网页端数据服务：/api/** → 本地只读 History HTTP 服务（真实 DuckDB）。
      // 走同源代理而不是直连 127.0.0.1:8080，就不必给服务端开 CORS；
      // 生产构建由同一台服务器同源提供 /api，行为一致。
      "/api": {
        target: "http://127.0.0.1:8080",
        changeOrigin: true,
      },
      // Visual QA 代理：/__qa/** → 同一服务。
      "/__qa": {
        target: "http://127.0.0.1:8080",
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/__qa/, ""),
      },
    },
  },
});
