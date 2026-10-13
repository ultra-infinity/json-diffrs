# Changelog

What changed in each json-diff release. The app shows the new entries once
after an update (and all of them under **Help → What's New**), and each
GitHub release uses its entry as its notes.

Write new entries under a `## Unreleased` heading at the top. When releasing,
rename it to `## <version> - <date>`; `scripts/release.sh` refuses a version
with no entry.
Keep entries to `-` bullets, with `**bold**` and `` `code` `` at most.

## 0.7.0 - 2026-10-13

- **Tree view, now the default:** json-diff opens on **Tree**, which shows
  both documents as one tree of keys instead of lines. A changed value reads
  old → new, added and removed keys are green and red, and each object or
  array says how many changes it holds. It opens folded to the top-level keys
  that changed; **Reveal changes** opens every path to a change, and **Show
  unchanged** adds the rest. Right-click a key to ignore it. The line diff is
  one click away under **Diff**.

## 0.6.1 - 2026-10-13

- json-diff is now released under the MIT License. Versions up to 0.6.0 were
  released under the GNU GPL, version 3 or later.

## 0.6.0 - 2026-10-13

- **json-diff on the web:** the same diff, folding, ignored keys and PDF
  export in a browser, with nothing to install. Open or drop two JSON files;
  they never leave your computer, since the comparison runs in the page. The
  web app compares single files only and has no assistant.
- Exported PDFs no longer repeat a file's name as its path.

## 0.5.0 - 2026-10-13

- **Ignore keys:** right-click a key in the diff to ignore its changes, either
  that key alone or every key with its name, such as a timestamp. Ignored
  changes leave the diff, the change count, folder comparisons, PDF reports
  and the assistant's answers; the assistant says when it left some out. The
  **N ignored** button in the toolbar lists them and counts them again, and
  they are remembered between runs.
- **Resizable diff:** drag the ribbon gutter between the two sides to give one
  of them more room. Double-click it to split evenly again. The split is
  remembered.
- **Scroll past the end:** the diff now scrolls until the last line reaches
  the top, so the last keys of a long document no longer sit against the
  bottom edge.
- **What's new:** after an update, json-diff shows what changed, as here. The
  update bar's **What's new** shows the new version's changes before you
  install it, and **Help → What's New** shows them all.

## 0.4.1 - 2026-10-13

- Fixed: a key added at the end of an object no longer stretches the ribbon
  over the line above it, which only gained a comma.

## 0.4.0 - 2026-10-13

- **Export as PDF:** **File → Export as PDF…** writes a report with a summary
  of every change and the side-by-side diff, on US Letter or A4.
- **Remembered settings:** theme, zoom, panels, panel widths, diff options and
  the assistant's model are kept between runs.
- **Keychain:** the assistant's API key can be kept in the system keychain.
- **Help → Licenses…** lists the licenses of every bundled library.
- The app icon shows in the Windows Start menu and taskbar, and in Linux
  launchers.

## 0.3.0 - 2026-10-13

- **In-app updates:** json-diff checks for new versions. On Windows,
  **Update now** downloads, verifies and installs it.
- Fixed: gaps and missing ribbons where the two sides didn't line up.

## 0.2.0 - 2026-10-13

- Rebuilt on Tauri, with the documents shown in Monaco editors, as in
  VS Code.
- A Windows installer (MSI).
- The assistant follows `AGENTS.md` files next to your documents, and you can
  add your own.
- Provider and model are picked from searchable lists.
