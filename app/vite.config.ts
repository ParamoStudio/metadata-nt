import { defineConfig } from "vite";

// Tauri expects a fixed dev server port and no automatic browser opening.
export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: "127.0.0.1",
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    // Tauri ships a system WebView; keep the bundle conservative.
    target: "safari16",
  },
});
