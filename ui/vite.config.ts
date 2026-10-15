import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";
import { CONTENT_SECURITY_POLICY } from "./src/csp";
/** `path`, relative to this file, as an absolute path. */
const here = (path: string) => decodeURIComponent(new URL(path, import.meta.url).pathname);

/** Puts the policy in the built page, as GitHub Pages cannot send it as a
 *  header. Not in development, where Vite's live reload needs inline code. */
const contentSecurityPolicy = (): Plugin => ({
  name: "content-security-policy",
  apply: "build",
  transformIndexHtml: () => [
    {
      tag: "meta",
      attrs: { "http-equiv": "Content-Security-Policy", content: CONTENT_SECURITY_POLICY },
      injectTo: "head-prepend",
    },
  ],
});

// Two builds of one frontend: the desktop app's (`vite build`, loaded by
// Tauri) and the web app's (`vite build --mode web`, a static site). They
// differ in `#backend`, the module that reaches the Rust app (see api.ts).
export default defineConfig(({ mode }) => {
  const web = mode === "web";
  return {
    plugins: [react(), ...(web ? [contentSecurityPolicy()] : [])],
    clearScreen: false,
    // Tauri loads the dev server from this fixed port (see tauri.conf.json).
    server: { port: 1420, strictPort: true },
    // Only the app's own page: not the copy in dist-web/ from a web build.
    optimizeDeps: { entries: ["index.html"] },
    // The site works from any folder, e.g. a GitHub Pages project path.
    base: web ? "./" : "/",
    resolve: {
      alias: {
        "#backend": here(`./src/backend/${web ? "web" : "desktop"}.ts`),
        // Made by `npm run wasm` (scripts/build-wasm.sh); web only.
        "#wasm": here("./wasm/json_diff_web.js"),
        "#notices": `${here("../THIRD_PARTY_NOTICES.txt")}?raw`,
      },
    },
    build: {
      target: "es2022",
      outDir: web ? "dist-web" : "dist",
      // Monaco is one large chunk by design; it is loaded from disk.
      chunkSizeWarningLimit: 8000,
    },
  };
});
