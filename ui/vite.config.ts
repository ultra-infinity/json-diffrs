import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri loads the dev server from this fixed port (see tauri.conf.json).
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    target: "es2022",
    // Monaco is one large chunk by design; it is loaded from disk.
    chunkSizeWarningLimit: 8000,
  },
});
