//! Application state and the command set that drives it.
//!
//! Every user-facing action (buttons, menus, keyboard shortcuts) and every
//! agent tool goes through [`AppState::apply`], so the UI and the agent always
//! have exactly the same capabilities. Nothing here touches egui; actions that
//! need the window are queued in [`AppState::window_requests`].

use crate::agents_md::AgentsMd;
use crate::layout::Paper;
use crate::pdf;
use json_diff_core::{
    DiffOptions, Folds, IgnoreRule, JsonDiff, PairEntry, PairStatus, compare_folders,
    diff_json_texts_ignoring, scan_json_files,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// Files larger than this are refused rather than freezing the UI.
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
/// Upper bound on list lengths returned by query commands.
const MAX_LISTED: usize = 500;
/// Upper bound on lines per side returned by `GetHunk`.
const MAX_HUNK_LINES: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// The original document: top folder explorer, left diff pane.
    Left,
    /// The modified document: bottom folder explorer, right diff pane.
    Right,
}

impl Side {
    pub const BOTH: [Side; 2] = [Side::Left, Side::Right];

    pub fn index(self) -> usize {
        match self {
            Side::Left => 0,
            Side::Right => 1,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Side::Left => "Original",
            Side::Right => "Modified",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ViewMode {
    /// Side-by-side diff.
    Diff,
    /// Two editable text panes.
    Edit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PanelKind {
    /// The left folder explorer.
    Explorer,
    /// The agent chat panel.
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WindowAction {
    Minimize,
    Maximize,
    Restore,
    Close,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    OpenFolder {
        side: Side,
        path: PathBuf,
    },
    CloseFolder {
        side: Side,
    },
    RefreshFolders,
    /// `path` is relative to that side's folder, or absolute.
    SelectFile {
        side: Side,
        path: PathBuf,
    },
    /// Opens the same relative path from both folders.
    OpenPair {
        path: PathBuf,
    },
    SetText {
        side: Side,
        text: String,
    },
    ClearSide {
        side: Side,
    },
    SwapSides,
    SetOptions {
        format: Option<bool>,
        sort_keys: Option<bool>,
        ignore_whitespace: Option<bool>,
    },
    NextChange,
    PreviousChange,
    GoToChange {
        index: usize,
    },
    SetViewMode {
        mode: ViewMode,
    },
    SetZoom {
        zoom: f32,
    },
    ZoomIn,
    ZoomOut,
    SetTheme {
        theme: Theme,
    },
    /// `visible: None` toggles.
    SetPanelVisible {
        panel: PanelKind,
        visible: Option<bool>,
    },
    Window {
        action: WindowAction,
    },
    GetState,
    ListFiles {
        side: Side,
    },
    ListPairs,
    GetChanges {
        offset: usize,
        limit: usize,
    },
    GetHunk {
        index: usize,
    },
    GetText {
        side: Side,
    },
    /// Expand or collapse the container at `pointer` on `side`, and its
    /// counterpart on the other side.
    SetExpanded {
        side: Side,
        pointer: String,
        expanded: bool,
    },
    ExpandAll,
    /// Back to the default: only top-level keys shown.
    CollapseAll,
    /// Collapse everything except the containers leading to a change.
    RevealChanges,
    /// Give the assistant an extra instructions file (an AGENTS.md).
    AddAgentsMd {
        path: PathBuf,
    },
    /// Forget an instructions file added with `AddAgentsMd`.
    RemoveAgentsMd {
        path: PathBuf,
    },
    /// Use or ignore one instructions file, found or added.
    SetAgentsMdEnabled {
        path: PathBuf,
        enabled: bool,
    },
    /// Stop treating a key's changes as changes: the node at `pointer` (as
    /// `side` has it), or every key named `key`. Exactly one is given.
    IgnoreChanges {
        side: Side,
        pointer: Option<String>,
        key: Option<String>,
    },
    /// Count an ignored key's changes again; `None`: every ignored key.
    StopIgnoring {
        rule: Option<IgnoreRule>,
    },
    /// Writes a PDF report: the list of changes, then the side-by-side diff
    /// with `context` unchanged lines around each change (`None`: whole
    /// documents). `path: None` means the user has yet to choose the file;
    /// the app asks before anything is written.
    ExportPdf {
        path: Option<PathBuf>,
        context: Option<usize>,
        paper: Paper,
    },
}

#[derive(Debug, Clone)]
pub struct FolderState {
    pub root: PathBuf,
    /// Relative to `root`.
    pub files: Vec<PathBuf>,
}

#[derive(Debug, Clone, Default)]
pub struct Document {
    /// Absolute path of the file the text came from, if any.
    pub path: Option<PathBuf>,
    pub text: String,
}

impl Document {
    pub fn label(&self) -> String {
        match &self.path {
            Some(path) => path.file_name().map_or_else(
                || path.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            ),
            None if self.text.is_empty() => "(empty)".to_owned(),
            None => "(unsaved text)".to_owned(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Status {
    pub text: String,
    pub is_error: bool,
}

pub struct AppState {
    pub folders: [Option<FolderState>; 2],
    /// Pairing of both folders; empty unless both are open.
    pub pairs: Vec<PairEntry>,
    pub docs: [Document; 2],
    pub options: DiffOptions,
    pub diff: JsonDiff,
    /// Which containers are expanded; collapsed below the top level by default.
    pub folds: Folds,
    pub current_hunk: Option<usize>,
    /// Set when the view should scroll to `current_hunk`.
    pub scroll_to_current: bool,
    pub view_mode: ViewMode,
    pub theme: Theme,
    pub zoom: f32,
    pub show_explorer: bool,
    pub show_agent: bool,
    pub status: Option<Status>,
    pub window_requests: Vec<WindowAction>,
    /// Instruction files for the assistant.
    pub agents_md: AgentsMd,
    /// Keys whose changes are ignored, in the diff and in reports.
    pub ignored: Vec<IgnoreRule>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            folders: [None, None],
            pairs: Vec::new(),
            docs: Default::default(),
            options: DiffOptions::default(),
            diff: JsonDiff::default(),
            folds: Folds::default(),
            current_hunk: None,
            scroll_to_current: false,
            view_mode: ViewMode::Diff,
            theme: Theme::Dark,
            zoom: 1.0,
            show_explorer: true,
            show_agent: true,
            status: None,
            window_requests: Vec::new(),
            agents_md: AgentsMd::default(),
            ignored: Vec::new(),
        }
    }
}

impl AppState {
    /// Runs `command`, recording a failure in the status bar. Returns the
    /// command's result as JSON for the agent.
    pub fn apply(&mut self, command: Command) -> Result<Value, String> {
        let result = self.run(command);
        if let Err(error) = &result {
            self.set_status(error.clone(), true);
        }
        // Opened folders and files decide which AGENTS.md files apply.
        let side = |side: Side| {
            (
                self.folders[side.index()]
                    .as_ref()
                    .map(|folder| folder.root.as_path()),
                self.docs[side.index()].path.as_deref(),
            )
        };
        let sides = [side(Side::Left), side(Side::Right)];
        self.agents_md.refresh(sides);
        result
    }

    /// The assistant's instruction files, for `get_state` and tool results.
    fn agents_md_files(&self) -> Vec<Value> {
        self.agents_md
            .files()
            .into_iter()
            .map(|file| {
                json!({
                    "path": file.path,
                    "source": file.source.as_str(),
                    "enabled": file.enabled,
                })
            })
            .collect()
    }

    fn run(&mut self, command: Command) -> Result<Value, String> {
        match command {
            Command::OpenFolder { side, path } => {
                let root = std::fs::canonicalize(&path)
                    .map_err(|error| format!("cannot open folder {}: {error}", path.display()))?;
                if !root.is_dir() {
                    return Err(format!("{} is not a folder", root.display()));
                }
                let files = scan_json_files(&root)
                    .map_err(|error| format!("cannot read folder {}: {error}", root.display()))?;
                self.set_status(
                    format!(
                        "{}: {} JSON files in {}",
                        side.title(),
                        files.len(),
                        root.display()
                    ),
                    false,
                );
                let result = json!({ "root": root, "json_files": files.len() });
                self.folders[side.index()] = Some(FolderState { root, files });
                self.update_pairs();
                Ok(result)
            }
            Command::CloseFolder { side } => {
                self.folders[side.index()] = None;
                self.update_pairs();
                Ok(json!("closed"))
            }
            Command::RefreshFolders => {
                for side in Side::BOTH {
                    if let Some(folder) = &mut self.folders[side.index()] {
                        folder.files = scan_json_files(&folder.root).map_err(|error| {
                            format!("cannot read {}: {error}", folder.root.display())
                        })?;
                    }
                }
                self.update_pairs();
                for side in Side::BOTH {
                    if let Some(path) = self.docs[side.index()].path.clone() {
                        self.load(side, &path)?;
                    }
                }
                self.recompute(false);
                Ok(self.summary())
            }
            Command::SelectFile { side, path } => {
                let path = self.resolve(side, &path)?;
                self.load(side, &path)?;
                self.new_documents();
                Ok(self.summary())
            }
            Command::OpenPair { path } => {
                let (Some(left), Some(right)) = (&self.folders[0], &self.folders[1]) else {
                    return Err("open_pair needs both an original and a modified folder".into());
                };
                let paths = [left.root.join(&path), right.root.join(&path)];
                if !paths.iter().any(|path| path.is_file()) {
                    return Err(format!("{} exists in neither folder", path.display()));
                }
                for (side, path) in Side::BOTH.into_iter().zip(paths) {
                    if path.is_file() {
                        self.load(side, &path)?;
                    } else {
                        self.docs[side.index()] = Document::default();
                    }
                }
                self.new_documents();
                Ok(self.summary())
            }
            Command::SetText { side, text } => {
                self.docs[side.index()] = Document { path: None, text };
                self.new_documents();
                Ok(self.summary())
            }
            Command::ClearSide { side } => {
                self.docs[side.index()] = Document::default();
                self.new_documents();
                Ok(self.summary())
            }
            Command::SwapSides => {
                self.folders.swap(0, 1);
                self.docs.swap(0, 1);
                self.update_pairs();
                self.new_documents();
                Ok(self.summary())
            }
            Command::SetOptions {
                format,
                sort_keys,
                ignore_whitespace,
            } => {
                if let Some(format) = format {
                    self.options.format = format;
                }
                if let Some(sort_keys) = sort_keys {
                    self.options.sort_keys = sort_keys;
                }
                if let Some(ignore_whitespace) = ignore_whitespace {
                    self.options.ignore_whitespace = ignore_whitespace;
                }
                self.recompute(true);
                Ok(json!({ "options": self.options, "summary": self.summary() }))
            }
            Command::NextChange => self.step_hunk(1),
            Command::PreviousChange => self.step_hunk(-1),
            Command::GoToChange { index } => {
                if index >= self.diff.view.hunks.len() {
                    return Err(format!(
                        "change {index} does not exist; there are {} changes",
                        self.diff.view.hunks.len()
                    ));
                }
                self.focus_hunk(index);
                self.hunk(index)
            }
            Command::SetViewMode { mode } => {
                self.view_mode = mode;
                Ok(json!({ "view_mode": mode }))
            }
            Command::SetZoom { zoom } => {
                if !(0.25..=5.0).contains(&zoom) {
                    return Err("zoom must be between 0.25 and 5.0".into());
                }
                self.zoom = zoom;
                Ok(json!({ "zoom": self.zoom }))
            }
            Command::ZoomIn => self.run(Command::SetZoom {
                zoom: (self.zoom + 0.1).min(5.0),
            }),
            Command::ZoomOut => self.run(Command::SetZoom {
                zoom: (self.zoom - 0.1).max(0.25),
            }),
            Command::SetTheme { theme } => {
                self.theme = theme;
                Ok(json!({ "theme": theme }))
            }
            Command::SetPanelVisible { panel, visible } => {
                let shown = match panel {
                    PanelKind::Explorer => &mut self.show_explorer,
                    PanelKind::Agent => &mut self.show_agent,
                };
                *shown = visible.unwrap_or(!*shown);
                Ok(json!({ "panel": panel, "visible": *shown }))
            }
            Command::Window { action } => {
                self.window_requests.push(action);
                Ok(json!({ "window": action }))
            }
            Command::GetState => Ok(self.describe()),
            Command::AddAgentsMd { path } => {
                let path = self.agents_md.add(&path)?;
                self.set_status(
                    format!("Assistant instructions: added {}", path.display()),
                    false,
                );
                Ok(json!({ "added": path, "agents_md": self.agents_md_files() }))
            }
            Command::RemoveAgentsMd { path } => {
                self.agents_md.remove(&path)?;
                Ok(json!({ "removed": path, "agents_md": self.agents_md_files() }))
            }
            Command::SetAgentsMdEnabled { path, enabled } => {
                self.agents_md.set_enabled(&path, enabled)?;
                Ok(json!({ "agents_md": self.agents_md_files() }))
            }
            Command::ExportPdf {
                path,
                context,
                paper,
            } => {
                let path =
                    path.ok_or("the user chooses where the PDF is saved; nothing was written")?;
                let path = pdf::with_pdf_extension(path);
                let report = pdf::export(self, context, paper, std::time::SystemTime::now())?;
                std::fs::write(&path, &report.bytes)
                    .map_err(|error| format!("could not write {}: {error}", path.display()))?;
                self.set_status(
                    format!("Exported a {}-page PDF to {}", report.pages, path.display()),
                    false,
                );
                Ok(json!({ "path": path, "pages": report.pages }))
            }
            Command::ListFiles { side } => {
                let folder = self.folders[side.index()]
                    .as_ref()
                    .ok_or_else(|| format!("no {} folder is open", side.title().to_lowercase()))?;
                Ok(json!({
                    "root": folder.root,
                    "total": folder.files.len(),
                    "files": folder.files.iter().take(MAX_LISTED).collect::<Vec<_>>(),
                }))
            }
            Command::ListPairs => {
                if self.pairs.is_empty() {
                    return Err("open both an original and a modified folder first".into());
                }
                let count = |status| {
                    self.pairs
                        .iter()
                        .filter(|pair| pair.status == status)
                        .count()
                };
                Ok(json!({
                    "total": self.pairs.len(),
                    "modified": count(PairStatus::Modified),
                    "identical": count(PairStatus::Identical),
                    "only_left": count(PairStatus::OnlyLeft),
                    "only_right": count(PairStatus::OnlyRight),
                    "pairs": self.pairs.iter().take(MAX_LISTED).collect::<Vec<_>>(),
                }))
            }
            Command::GetChanges { offset, limit } => {
                let changes = self.diff.changes.as_ref().ok_or(
                    "structural changes need two valid JSON documents; see get_state for parse errors",
                )?;
                let page: Vec<Value> = changes
                    .iter()
                    .skip(offset)
                    .take(limit.min(MAX_LISTED))
                    .map(|change| {
                        json!({
                            "path": change.path,
                            "kind": change.kind,
                            "old": change.old.as_ref().map(preview),
                            "new": change.new.as_ref().map(preview),
                        })
                    })
                    .collect();
                let mut result =
                    json!({ "total": changes.len(), "offset": offset, "changes": page });
                if self.diff.ignored_changes > 0 {
                    result["ignored"] = json!({
                        "changes": self.diff.ignored_changes,
                        "keys": self.ignored_labels(),
                        "note": "These changes are left out because the user ignores these keys. Tell the user they were left out.",
                    });
                }
                Ok(result)
            }
            Command::GetHunk { index } => self.hunk(index),
            Command::GetText { side } => {
                let text = &self.docs[side.index()].text;
                const LIMIT: usize = 100_000;
                let end = text.floor_char_boundary(LIMIT.min(text.len()));
                Ok(json!({
                    "label": self.docs[side.index()].label(),
                    "truncated": end < text.len(),
                    "text": &text[..end],
                }))
            }
            Command::SetExpanded {
                side,
                pointer,
                expanded,
            } => {
                self.require_foldable()?;
                let left = side == Side::Left;
                self.folds.set_expanded(left, &pointer, expanded);
                if let Some(other) = self.diff.counterpart(&pointer, left) {
                    self.folds.set_expanded(!left, &other, expanded);
                }
                self.refold();
                Ok(json!({ "pointer": pointer, "expanded": expanded, "summary": self.summary() }))
            }
            Command::ExpandAll => {
                self.require_foldable()?;
                self.folds = Folds::expanded();
                self.refold();
                Ok(self.summary())
            }
            Command::CollapseAll => {
                self.require_foldable()?;
                self.folds = Folds::collapsed();
                self.refold();
                Ok(self.summary())
            }
            Command::RevealChanges => {
                self.require_foldable()?;
                self.folds = self.diff.reveal_changes();
                self.refold();
                Ok(self.summary())
            }
            Command::IgnoreChanges { side, pointer, key } => {
                let rule = match (pointer, key) {
                    (Some(pointer), None) if pointer.is_empty() => {
                        return Err("the whole document cannot be ignored".into());
                    }
                    (Some(pointer), None) => {
                        // Kept as the original document has it, where it exists.
                        let pointer = match side {
                            Side::Left => pointer,
                            Side::Right => {
                                self.diff.counterpart(&pointer, false).unwrap_or(pointer)
                            }
                        };
                        IgnoreRule::Pointer { pointer }
                    }
                    (None, Some(key)) => IgnoreRule::Key { key },
                    _ => return Err("give either a pointer or a key name, not both".into()),
                };
                if !self.ignored.contains(&rule) {
                    self.ignored.push(rule.clone());
                }
                self.recompute(false);
                self.update_pairs();
                self.set_status(
                    format!(
                        "Ignoring changes to {rule}: {} ignored in this comparison",
                        self.diff.ignored_changes
                    ),
                    false,
                );
                Ok(json!({ "ignoring": rule.to_string(), "summary": self.summary() }))
            }
            Command::StopIgnoring { rule } => {
                match rule {
                    Some(rule) => {
                        let before = self.ignored.len();
                        self.ignored.retain(|ignored| *ignored != rule);
                        if self.ignored.len() == before {
                            return Err(format!("{rule} is not ignored"));
                        }
                        self.set_status(format!("Changes to {rule} count again"), false);
                    }
                    None => {
                        self.ignored.clear();
                        self.set_status("Every change counts again", false);
                    }
                }
                self.recompute(false);
                self.update_pairs();
                Ok(json!({ "ignoring": self.ignored_labels(), "summary": self.summary() }))
            }
        }
    }

    /// The assistant's instructions about ignored keys, sent with each
    /// message; empty when nothing is ignored.
    pub fn ignored_prompt_section(&self) -> String {
        if self.ignored.is_empty() {
            return String::new();
        }
        let keys: String = self
            .ignored_labels()
            .iter()
            .map(|label| format!("- {label}\n"))
            .collect();
        format!(
            "\n\n## Ignored keys\n\n\
             The user chose to ignore changes to these keys:\n\n{keys}\n\
             Their changes are left out of the diff, get_structural_changes, the changed \
             blocks and PDF reports ({count} in the current comparison). Do not report or \
             analyse changes to them, even where you see them in get_text or get_change. \
             Whenever you summarise or list changes, say that changes to ignored keys were \
             left out, and that the user can count them again by right-clicking the key or \
             asking you (stop_ignoring).\n",
            count = self.diff.ignored_changes,
        )
    }

    /// The ignore rules as people read them.
    pub fn ignored_labels(&self) -> Vec<String> {
        self.ignored.iter().map(ToString::to_string).collect()
    }

    pub fn set_status(&mut self, text: impl Into<String>, is_error: bool) {
        self.status = Some(Status {
            text: text.into(),
            is_error,
        });
    }

    /// Recomputes the diff after either document or the options changed.
    pub fn recompute(&mut self, reset_position: bool) {
        self.diff = diff_json_texts_ignoring(
            &self.docs[0].text,
            &self.docs[1].text,
            &self.options,
            &self.folds,
            &self.ignored,
        );
        self.clamp_current(reset_position);
    }

    /// A file handed over as text, without a path to read it from again
    /// (the web app's file picker or a dropped file): `name` labels the side.
    pub fn open_text(&mut self, side: Side, name: &str, text: String) {
        self.docs[side.index()] = Document {
            path: Some(PathBuf::from(name)),
            text,
        };
        self.new_documents();
        self.set_status(format!("{}: {name}", side.title()), false);
    }

    /// Text typed into one of the editors: unlike `SetText`, the document
    /// keeps its file name and the diff keeps its folds.
    pub fn edit_text(&mut self, side: Side, text: String) {
        self.docs[side.index()].text = text;
        self.recompute(false);
    }

    /// Recomputes after new documents were loaded: folds start over.
    fn new_documents(&mut self) {
        self.folds = Folds::default();
        self.recompute(true);
    }

    /// Re-renders after the folds changed, without re-parsing.
    fn refold(&mut self) {
        self.diff.refold(&self.folds);
        self.clamp_current(false);
    }

    fn require_foldable(&self) -> Result<(), String> {
        if self.diff.is_foldable() {
            Ok(())
        } else {
            Err("folding needs formatted JSON: turn on Format JSON and fix any parse errors".into())
        }
    }

    fn clamp_current(&mut self, reset_position: bool) {
        let hunks = self.diff.view.hunks.len();
        self.current_hunk = match self.current_hunk {
            _ if hunks == 0 => None,
            Some(index) if !reset_position => Some(index.min(hunks - 1)),
            _ => Some(0),
        };
        self.scroll_to_current = reset_position && self.current_hunk.is_some();
    }

    pub fn focus_hunk(&mut self, index: usize) {
        self.current_hunk = Some(index);
        self.scroll_to_current = true;
    }

    /// The pair entry for `path` (relative), if both folders are open.
    pub fn pair_status(&self, path: &Path) -> Option<PairStatus> {
        self.pairs
            .binary_search_by(|pair| pair.path.as_path().cmp(path))
            .ok()
            .map(|index| self.pairs[index].status)
    }

    /// Whether `relative` in `side`'s folder is the file shown on that side.
    pub fn is_selected(&self, side: Side, relative: &Path) -> bool {
        match (&self.folders[side.index()], &self.docs[side.index()].path) {
            (Some(folder), Some(path)) => path.strip_prefix(&folder.root).ok() == Some(relative),
            _ => false,
        }
    }

    fn resolve(&self, side: Side, path: &Path) -> Result<PathBuf, String> {
        if path.is_absolute() {
            return Ok(path.to_path_buf());
        }
        let folder = self.folders[side.index()].as_ref().ok_or_else(|| {
            format!(
                "{} is relative but no {} folder is open",
                path.display(),
                side.title().to_lowercase()
            )
        })?;
        Ok(folder.root.join(path))
    }

    fn load(&mut self, side: Side, path: &Path) -> Result<(), String> {
        let metadata = std::fs::metadata(path)
            .map_err(|error| format!("cannot open {}: {error}", path.display()))?;
        if metadata.len() > MAX_FILE_BYTES {
            return Err(format!(
                "{} is {} MB; the limit is {} MB",
                path.display(),
                metadata.len() / (1024 * 1024),
                MAX_FILE_BYTES / (1024 * 1024)
            ));
        }
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        self.docs[side.index()] = Document {
            path: Some(path.to_path_buf()),
            text,
        };
        Ok(())
    }

    fn update_pairs(&mut self) {
        self.pairs = match (&self.folders[0], &self.folders[1]) {
            (Some(left), Some(right)) => compare_folders(
                &left.root,
                &left.files,
                &right.root,
                &right.files,
                &self.ignored,
            ),
            _ => Vec::new(),
        };
    }

    fn step_hunk(&mut self, delta: isize) -> Result<Value, String> {
        let count = self.diff.view.hunks.len();
        if count == 0 {
            return Err("there are no changes".into());
        }
        let next = match self.current_hunk {
            None => 0,
            Some(index) => (index as isize + delta).rem_euclid(count as isize) as usize,
        };
        self.focus_hunk(next);
        self.hunk(next)
    }

    fn hunk(&self, index: usize) -> Result<Value, String> {
        let view = &self.diff.view;
        let hunk = view.hunks.get(index).ok_or_else(|| {
            format!(
                "change {index} does not exist; there are {} changes",
                view.hunks.len()
            )
        })?;
        let lines = |lines: &[String], range: &std::ops::Range<usize>| {
            lines[range.clone()]
                .iter()
                .take(MAX_HUNK_LINES)
                .cloned()
                .collect::<Vec<_>>()
        };
        let statuses = |statuses: &[json_diff_core::LineStatus], range: &std::ops::Range<usize>| {
            statuses[range.clone()]
                .iter()
                .take(MAX_HUNK_LINES)
                .copied()
                .collect::<Vec<_>>()
        };
        // Folded containers in the block: expand them to see inside.
        let collapsed = |folds: &[Option<json_diff_core::FoldMark>],
                         range: &std::ops::Range<usize>| {
            folds[range.clone()]
                .iter()
                .flatten()
                .filter(|mark| mark.collapsed)
                .map(|mark| mark.pointer.clone())
                .collect::<Vec<_>>()
        };
        Ok(json!({
            "index": index,
            "of": view.hunks.len(),
            "kind": hunk.kind,
            "left_first_line": hunk.left.start + 1,
            "left_lines": lines(&view.left, &hunk.left),
            "left_status": statuses(&view.left_status, &hunk.left),
            "right_first_line": hunk.right.start + 1,
            "right_lines": lines(&view.right, &hunk.right),
            "right_status": statuses(&view.right_status, &hunk.right),
            "collapsed": {
                "left": collapsed(&view.left_folds, &hunk.left),
                "right": collapsed(&view.right_folds, &hunk.right),
            },
        }))
    }

    /// Short description of the current comparison.
    pub fn summary(&self) -> Value {
        let doc = |side: Side| {
            let doc = &self.docs[side.index()];
            let error = match side {
                Side::Left => &self.diff.left_error,
                Side::Right => &self.diff.right_error,
            };
            json!({
                "label": doc.label(),
                "path": doc.path,
                "lines": match side {
                    Side::Left => self.diff.view.left.len(),
                    Side::Right => self.diff.view.right.len(),
                },
                "parse_error": error.as_ref().map(ToString::to_string),
            })
        };
        json!({
            "left": doc(Side::Left),
            "right": doc(Side::Right),
            "changes": self.diff.view.hunks.len(),
            "current_change": self.current_hunk,
            "added_lines": self.diff.view.added_lines(),
            "removed_lines": self.diff.view.removed_lines(),
            "structural_changes": self.diff.changes.as_ref().map(Vec::len),
            "ignored_changes": self.diff.ignored_changes,
            "ignoring": self.ignored_labels(),
        })
    }

    /// Everything visible in the UI, for the agent's `get_state`.
    pub fn describe(&self) -> Value {
        let folder = |side: Side| {
            self.folders[side.index()]
                .as_ref()
                .map(|folder| json!({ "root": folder.root, "json_files": folder.files.len() }))
        };
        json!({
            "original_folder": folder(Side::Left),
            "modified_folder": folder(Side::Right),
            "paired_files": self.pairs.len(),
            "comparison": self.summary(),
            "options": self.options,
            "view_mode": self.view_mode,
            "theme": self.theme,
            "zoom": self.zoom,
            "explorer_visible": self.show_explorer,
            "agent_panel_visible": self.show_agent,
            "folding": {
                "available": self.diff.is_foldable(),
                "default": if self.folds.expands_by_default() { "expanded" } else { "collapsed" },
                "exceptions": self.folds.exceptions(),
            },
            "agents_md": self.agents_md_files(),
            "status": self.status.as_ref().map(|status| &status.text),
        })
    }
}

/// A JSON value for the agent, abbreviated when large.
fn preview(value: &Value) -> Value {
    const LIMIT: usize = 400;
    let text = value.to_string();
    if text.len() <= LIMIT {
        value.clone()
    } else {
        let end = text.floor_char_boundary(LIMIT);
        Value::String(format!("{}… ({} bytes)", &text[..end], text.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_folders() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let (left, right) = (root.path().join("a"), root.path().join("b"));
        std::fs::create_dir_all(&left).unwrap();
        std::fs::create_dir_all(&right).unwrap();
        std::fs::write(left.join("x.json"), r#"{"a": 1, "b": 2}"#).unwrap();
        std::fs::write(right.join("x.json"), r#"{"a": 1, "b": 3, "c": 4}"#).unwrap();
        std::fs::write(right.join("new.json"), r#"[1]"#).unwrap();
        (root, left, right)
    }

    #[test]
    fn folder_workflow() {
        let (_root, left, right) = temp_folders();
        let mut state = AppState::default();
        state
            .apply(Command::OpenFolder {
                side: Side::Left,
                path: left,
            })
            .unwrap();
        state
            .apply(Command::OpenFolder {
                side: Side::Right,
                path: right,
            })
            .unwrap();
        assert_eq!(state.pairs.len(), 2);
        assert_eq!(
            state.pair_status(Path::new("x.json")),
            Some(PairStatus::Modified)
        );
        assert_eq!(
            state.pair_status(Path::new("new.json")),
            Some(PairStatus::OnlyRight)
        );

        let summary = state
            .apply(Command::OpenPair {
                path: "x.json".into(),
            })
            .unwrap();
        assert_eq!(summary["structural_changes"], 2);
        assert!(state.is_selected(Side::Left, Path::new("x.json")));
        assert_eq!(state.current_hunk, Some(0));

        state
            .apply(Command::OpenPair {
                path: "new.json".into(),
            })
            .unwrap();
        assert_eq!(state.docs[0].text, "");
        assert_eq!(state.diff.view.added_lines(), 3);
    }

    #[test]
    fn navigation_wraps_and_errors_are_reported() {
        let mut state = AppState::default();
        assert!(state.apply(Command::NextChange).is_err());
        assert!(state.status.as_ref().unwrap().is_error);

        state
            .apply(Command::SetText {
                side: Side::Left,
                text: "[1, 2, 3, 4, 5]".into(),
            })
            .unwrap();
        state
            .apply(Command::SetText {
                side: Side::Right,
                text: "[0, 2, 3, 4, 6]".into(),
            })
            .unwrap();
        assert_eq!(state.diff.view.hunks.len(), 2);
        assert_eq!(state.current_hunk, Some(0));
        state.apply(Command::NextChange).unwrap();
        assert_eq!(state.current_hunk, Some(1));
        state.apply(Command::NextChange).unwrap();
        assert_eq!(state.current_hunk, Some(0));
        state.apply(Command::PreviousChange).unwrap();
        assert_eq!(state.current_hunk, Some(1));
    }

    #[test]
    fn swap_and_options() {
        let mut state = AppState::default();
        state
            .apply(Command::SetText {
                side: Side::Left,
                text: r#"{"b":1,"a":2}"#.into(),
            })
            .unwrap();
        state
            .apply(Command::SetText {
                side: Side::Right,
                text: r#"{"a":2,"b":1}"#.into(),
            })
            .unwrap();
        // Same value, different key order: lines differ until keys are sorted.
        assert!(!state.diff.view.hunks.is_empty());
        state
            .apply(Command::SetOptions {
                format: None,
                sort_keys: Some(true),
                ignore_whitespace: None,
            })
            .unwrap();
        assert!(state.diff.view.hunks.is_empty());

        state.apply(Command::SwapSides).unwrap();
        assert_eq!(state.docs[0].text, r#"{"a":2,"b":1}"#);
    }

    #[test]
    fn folding_commands_keep_both_sides_in_step() {
        let mut state = AppState::default();
        let left = r#"{"items": [{"id": 1, "tags": ["a"]}], "meta": {"v": 1}}"#;
        let right = r#"{"items": [{"id": 0}, {"id": 1, "tags": ["a", "b"]}], "meta": {"v": 1}}"#;
        state
            .apply(Command::SetText {
                side: Side::Left,
                text: left.into(),
            })
            .unwrap();
        state
            .apply(Command::SetText {
                side: Side::Right,
                text: right.into(),
            })
            .unwrap();

        // Opens with one line per top-level key.
        assert_eq!(state.diff.view.right.len(), 4);

        // Expanding the shifted element on the left expands its counterpart
        // (index 1) on the right.
        state
            .apply(Command::SetExpanded {
                side: Side::Left,
                pointer: "/items".into(),
                expanded: true,
            })
            .unwrap();
        state
            .apply(Command::SetExpanded {
                side: Side::Left,
                pointer: "/items/0".into(),
                expanded: true,
            })
            .unwrap();
        assert!(state.folds.is_expanded(false, "/items"));
        assert!(state.folds.is_expanded(false, "/items/1"));
        assert!(!state.folds.is_expanded(false, "/items/0"));

        state.apply(Command::RevealChanges).unwrap();
        assert!(state.folds.is_expanded(false, "/items/1/tags"));
        assert!(!state.folds.is_expanded(false, "/meta"));

        state.apply(Command::ExpandAll).unwrap();
        assert!(
            state
                .diff
                .view
                .right
                .iter()
                .any(|line| line.contains("\"b\""))
        );
        state.apply(Command::CollapseAll).unwrap();
        assert_eq!(state.diff.view.right.len(), 4);

        // New documents start folded again.
        state.apply(Command::ExpandAll).unwrap();
        state
            .apply(Command::SetText {
                side: Side::Right,
                text: left.into(),
            })
            .unwrap();
        assert_eq!(state.folds, Folds::default());

        // Folding needs formatted JSON.
        state
            .apply(Command::SetText {
                side: Side::Right,
                text: "{".into(),
            })
            .unwrap();
        assert!(state.apply(Command::ExpandAll).is_err());
    }

    #[test]
    fn opened_folders_bring_their_agents_md() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::write(root.join("AGENTS.md"), "Version bumps are expected.").unwrap();
        std::fs::write(root.join("a.json"), "{}").unwrap();

        let mut state = AppState::default();
        assert_eq!(state.describe()["agents_md"], json!([]));
        state
            .apply(Command::OpenFolder {
                side: Side::Left,
                path: root.clone(),
            })
            .unwrap();
        let path = root.join("AGENTS.md");
        assert_eq!(
            state.describe()["agents_md"],
            json!([{ "path": path, "source": "found", "enabled": true }])
        );
        assert!(state.agents_md.prompt_section().contains("Version bumps"));

        // The assistant's tools switch it off like the panel's checkbox.
        state
            .apply(Command::SetAgentsMdEnabled {
                path: path.clone(),
                enabled: false,
            })
            .unwrap();
        assert_eq!(state.agents_md.prompt_section(), "");
        assert!(state.apply(Command::RemoveAgentsMd { path }).is_err());
    }
}
