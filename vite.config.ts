import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // Cargo rebuilds churn src-tauri/target during `tauri dev`; watching it
      // exhausts the filesystem watcher on Windows.
      ignored: ["**/src-tauri/target/**", "**/binaries/pi-sidecar/node_modules/**"],
    },
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  test: {
    environment: "jsdom",
    setupFiles: ["./vitest.setup.ts"],
    restoreMocks: true,
  },
});
