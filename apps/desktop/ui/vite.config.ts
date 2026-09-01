import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  // MapLibre bundles its own ES module worker. Keep it out of Vite's
  // dependency optimizer so the worker remains resolvable in dev mode.
  optimizeDeps: {
    exclude: ["maplibre-gl"],
  },
  server: {
    strictPort: true,
  },
});
