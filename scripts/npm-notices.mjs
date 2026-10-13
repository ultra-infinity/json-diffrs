// Prints the licenses of the npm packages bundled into json-diff's frontend
// (production dependencies only, not build tools), for THIRD_PARTY_NOTICES.txt.
// Run by scripts/notices.sh after `npm ci` in ui/.

import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const ui = new URL("../ui/", import.meta.url).pathname;
const lock = JSON.parse(readFileSync(join(ui, "package-lock.json"), "utf8"));

const rule = "=".repeat(80);
const out = ["", "JavaScript libraries", "--------------------", ""];

const packages = Object.entries(lock.packages)
  .filter(([path, info]) => path.startsWith("node_modules/") && !info.dev)
  .map(([path, info]) => ({ path, name: path.replace(/^.*node_modules\//, ""), ...info }))
  .sort((a, b) => a.name.localeCompare(b.name));

for (const pkg of packages) {
  const dir = join(ui, pkg.path);
  if (!existsSync(dir)) throw new Error(`${pkg.name} is not installed; run npm ci in ui/ first`);
  const license = pkg.license ?? JSON.parse(readFileSync(join(dir, "package.json"), "utf8")).license;
  out.push(rule, `${pkg.name} ${pkg.version} (${license})`, "", licenseText(pkg.name, dir), "");
  // Monaco bundles code from other projects and lists their licenses here.
  const notices = join(dir, "ThirdPartyNotices.txt");
  if (existsSync(notices)) {
    out.push(`Third-party notices included with ${pkg.name}:`, "", readFileSync(notices, "utf8").trim(), "");
  }
}

process.stdout.write(out.join("\n") + "\n");

/** The license text a package ships: LICENSE, or for dual-licensed packages
 *  LICENSE-MIT (the license json-diff uses them under). */
function licenseText(name, dir) {
  const files = readdirSync(dir);
  const file =
    files.find((file) => /^(licen[cs]e|copying)(\.(md|txt))?$/i.test(file)) ??
    files.find((file) => /^licen[cs]e-mit(\.(md|txt))?$/i.test(file));
  if (file) return readFileSync(join(dir, file), "utf8").trim();
  // The Tauri plugins' npm packages carry only an SPDX tag; they are MIT OR
  // Apache-2.0 from the same authors as @tauri-apps/api, whose MIT text
  // applies.
  if (name.startsWith("@tauri-apps/plugin-")) {
    return readFileSync(join(ui, "node_modules/@tauri-apps/api/LICENSE-MIT"), "utf8").trim();
  }
  throw new Error(`${name} ships no license file; add it to scripts/npm-notices.mjs`);
}
