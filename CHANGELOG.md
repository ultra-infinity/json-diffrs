# Changelog

What changed in each json-diff release. The app shows the new entries once
after an update (and all of them under **Help → What's New**), and each
GitHub release uses its entry as its notes.

Write new entries under a `## Unreleased` heading at the top. When releasing,
rename it to `## <version> - <date>`; `scripts/release.sh` refuses a version
with no entry.
Keep entries to `-` bullets, with `**bold**` and `` `code` `` at most.

## 0.9.0 - 2026-10-15

- **A home of its own:** the web app now lives at **https://json-diffrs.site**.
  The old GitHub address forwards there.
- **Opening folders on the web, without the scare:** in Chrome and Edge the
  browser now asks to *view* the folder's files instead of to "upload" them
  (Firefox and Safari can only say "upload"). Before the browser asks,
  json-diff explains that the files stay on your computer and nothing is
  sent anywhere. The explorer also says that files are read in the browser
  and never uploaded.
- **Privacy you can check:** the web app now carries a Content Security
  Policy, a rule the browser enforces that lets the page load from and
  connect to its own site only, so nothing it reads can be sent anywhere.
  **Help → Privacy & Security** explains where your files go (nowhere) and
  shows the policy.
- **Host the web app yourself:** each release now includes
  `json-diff-web-<version>.zip`, the web app as a folder of static files to
  put on any web server, such as one inside your organisation. The README
  explains how.
- If the browser cannot read a picked folder, the status bar now says so
  instead of nothing happening.

## 0.8.0 - 2026-10-15

- **Folders on the web:** the web app can now compare folders. Pick one with
  **Open Folder** in each half of the explorer, or drop folders on the page;
  the files are read in the page and never leave your computer. Pick a folder
  again to see changes made to it since.
- **Compare any two files across folders:** with both folders open, a click
  now puts the file in its own pane only, so any original file can be
  compared with any modified one, whatever their names. Comparing a file with
  its same-named twin moved to right-click.
- The explorer's two folders are colour-coded like the diff's sides: red for
  **Original** (opens in the left pane) and green for **Modified** (the right
  pane).
- Fixed: since 0.7.0 the tree view's styles also applied to the explorer's
  file list, giving it a monospaced font and a large empty space at the end.

## 0.7.1 - 2026-10-14

- json-diff opens on the line diff again, and the toolbar reads **Diff |
  Tree | Edit**. The tree view stays one click away under **Tree**, for an
  overview of what changed.

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
