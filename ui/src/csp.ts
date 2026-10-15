// The web app's Content Security Policy, shared by the build (vite.config.ts
// puts it in the page) and Help > Privacy & Security, which shows it.
//
// The browser lets the page load and connect to its own site only, so
// nothing the page reads can be sent anywhere. WebAssembly needs
// 'wasm-unsafe-eval' (it allows compiling the module, not running text as
// code); Monaco adds its own <style> elements. The desktop app has its own
// policy in crates/json-diff/tauri.conf.json.

export const CONTENT_SECURITY_POLICY = [
  "default-src 'self'",
  "script-src 'self' 'wasm-unsafe-eval'",
  "style-src 'self' 'unsafe-inline'",
  "img-src 'self' data:",
  "font-src 'self' data:",
  "connect-src 'self'",
  "worker-src 'self'",
  "object-src 'none'",
  "base-uri 'none'",
  "form-action 'none'",
].join("; ");
