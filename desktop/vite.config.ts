import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

// Tauri serves the dev build from a fixed port and reads the production build
// from ./dist. See src-tauri/tauri.conf.json.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // The brand fonts live at the repository root, next to the website.
    fs: { allow: [".."] },
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    target: "es2022",
    outDir: "dist",
    emptyOutDir: true,
  },
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.{ts,tsx}"],
    setupFiles: ["src/test/setup.ts"],
    clearMocks: true,
  },
});
