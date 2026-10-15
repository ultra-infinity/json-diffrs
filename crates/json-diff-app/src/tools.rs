//! The agent-facing tool catalogue: one tool per [`Command`], each with a
//! JSON schema generated from its typed arguments.

use crate::layout::Paper;
use crate::state::{Command, PanelKind, Side, Theme, ViewMode, WindowAction};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;

pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: Value,
    pub parse: fn(Value) -> Result<Command, serde_json::Error>,
}

/// The JSON schema of `T`, without the `$schema`/`title` noise providers
/// don't need.
fn parameters<T: JsonSchema>() -> Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(T)).unwrap_or_default();
    if let Some(object) = schema.as_object_mut() {
        object.remove("$schema");
        object.remove("title");
        object
            .entry("properties")
            .or_insert_with(|| Value::Object(Default::default()));
    }
    schema
}

/// Declares the argument-less and argument-carrying tools in one table.
macro_rules! tools {
    ($($name:literal: $description:literal, $args:ty => |$a:pat_param| $command:expr;)*) => {
        pub fn tool_specs() -> Vec<ToolSpec> {
            vec![$(ToolSpec {
                name: $name,
                description: $description,
                parameters: parameters::<$args>(),
                parse: |value| {
                    let $a: $args = serde_json::from_value(value)?;
                    Ok($command)
                },
            }),*]
        }
    };
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct NoArgs {}

#[derive(Deserialize, JsonSchema)]
struct SideArgs {
    /// `left` is the original document (top explorer, left pane); `right` is
    /// the modified document (bottom explorer, right pane).
    side: Side,
}

#[derive(Deserialize, JsonSchema)]
struct FolderArgs {
    /// `left` = original folder (top explorer), `right` = modified folder
    /// (bottom explorer).
    side: Side,
    /// Absolute folder path. `~` is not expanded.
    path: PathBuf,
}

#[derive(Deserialize, JsonSchema)]
struct InstructionsArgs {
    /// Absolute path of the instructions file. `~` is not expanded.
    path: PathBuf,
}

#[derive(Deserialize, JsonSchema)]
struct InstructionsToggleArgs {
    /// Path of the instructions file, as listed under `agents_md` by
    /// `get_state`.
    path: PathBuf,
    enabled: bool,
}

#[derive(Deserialize, JsonSchema)]
struct ExportArgs {
    /// Unchanged lines to show around each change in the side-by-side pages
    /// (default 3).
    context: Option<usize>,
    /// Show both documents in full instead of only the changes.
    #[serde(default)]
    full: bool,
    /// `letter` (default) or `a4`.
    paper: Option<Paper>,
}

#[derive(Deserialize, JsonSchema)]
struct IgnoreArgs {
    /// Ignore the node at this JSON pointer and everything inside it, e.g.
    /// `/services/0/port`.
    pointer: Option<String>,
    /// Or: ignore every key with this name, at any depth, e.g. `updatedAt`.
    key: Option<String>,
    /// The document `pointer` comes from (default `left`, the original).
    side: Option<Side>,
}

#[derive(Deserialize, JsonSchema)]
struct StopIgnoringArgs {
    /// A pointer passed to ignore_changes.
    pointer: Option<String>,
    /// A key name passed to ignore_changes.
    key: Option<String>,
    /// Count every ignored key's changes again.
    #[serde(default)]
    all: bool,
}

/// The rule `stop_ignoring` names; `None` for all of them.
fn stop_ignoring(args: StopIgnoringArgs) -> Result<Command, serde_json::Error> {
    use json_diff_core::IgnoreRule;
    use serde::de::Error;
    let rule = match (args.pointer, args.key, args.all) {
        (Some(pointer), None, false) => Some(IgnoreRule::Pointer { pointer }),
        (None, Some(key), false) => Some(IgnoreRule::Key { key }),
        (None, None, true) => None,
        _ => return Err(serde_json::Error::custom("give one of pointer, key or all")),
    };
    Ok(Command::StopIgnoring { rule })
}

#[derive(Deserialize, JsonSchema)]
struct FileArgs {
    side: Side,
    /// A path relative to that side's open folder (as listed by
    /// `list_files`), or an absolute file path.
    path: PathBuf,
}

#[derive(Deserialize, JsonSchema)]
struct PairArgs {
    /// A path relative to both folders, as listed by `list_pairs`.
    path: PathBuf,
}

#[derive(Deserialize, JsonSchema)]
struct TextArgs {
    side: Side,
    /// The full JSON text to show on that side.
    text: String,
}

#[derive(Deserialize, JsonSchema)]
struct OptionsArgs {
    /// Pretty print both documents before comparing (hides formatting-only
    /// differences and aligns lines by JSON structure).
    format: Option<bool>,
    /// Sort object keys when formatting (hides key-order differences).
    sort_keys: Option<bool>,
    /// Ignore whitespace when matching lines; only used when `format` is off.
    ignore_whitespace: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
struct IndexArgs {
    /// Zero-based change index.
    index: usize,
}

#[derive(Deserialize, JsonSchema)]
struct PageArgs {
    /// Index of the first change to return.
    #[serde(default)]
    offset: usize,
    /// Maximum number of changes to return (at most 500).
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    100
}

#[derive(Deserialize, JsonSchema)]
struct ViewModeArgs {
    /// `diff` (lines side by side), `tree` (one tree of keys with changes
    /// marked) or `edit` (two text editors).
    mode: ViewMode,
}

#[derive(Deserialize, JsonSchema)]
struct TreeOptionsArgs {
    /// List unchanged keys too, instead of only the paths to changes.
    show_unchanged: Option<bool>,
    /// true: open every path down to each change; false: fold everything to
    /// the top level (how the tree starts).
    open_changes: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
struct TreeNodeArgs {
    /// The node's `id`, as listed by get_tree.
    id: String,
}

#[derive(Deserialize, JsonSchema)]
struct ZoomArgs {
    /// UI scale factor between 0.25 and 5.0; 1.0 is the default.
    zoom: f32,
}

#[derive(Deserialize, JsonSchema)]
struct ThemeArgs {
    theme: Theme,
}

#[derive(Deserialize, JsonSchema)]
struct PanelArgs {
    panel: PanelKind,
    /// Omit to toggle.
    visible: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
struct WindowArgs {
    action: WindowAction,
}

#[derive(Deserialize, JsonSchema)]
struct NodeArgs {
    /// The document the pointer refers to. Structural changes give `path`
    /// in the modified (right) document, except removals, which are in the
    /// original (left) one.
    side: Side,
    /// JSON pointer of an object or array, e.g. `/settings` or `/items/3`.
    /// The same node on the other side follows automatically.
    pointer: String,
}

tools! {
    "get_state": "Describe what the app currently shows: open folders, the two documents being compared, change counts, parse errors, options and UI settings. Call this first.",
        NoArgs => |_| Command::GetState;
    "open_folder": "Open a folder in the explorer. side=left opens the original folder (top explorer), side=right the modified folder (bottom explorer). Same as the explorer's 'Open folder' button.",
        FolderArgs => |a| Command::OpenFolder { side: a.side, path: a.path };
    "close_folder": "Close the folder shown in one explorer.",
        SideArgs => |a| Command::CloseFolder { side: a.side };
    "refresh": "Rescan both folders and reload both documents from disk.",
        NoArgs => |_| Command::RefreshFolders;
    "list_files": "List the JSON files in one side's open folder (relative paths).",
        SideArgs => |a| Command::ListFiles { side: a.side };
    "list_pairs": "When both folders are open, list every relative path with its status: modified, identical, only_left or only_right.",
        NoArgs => |_| Command::ListPairs;
    "select_file": "Show a file on one side of the diff, like clicking it in that side's explorer. The other side keeps its file, so any original file can be compared with any modified one.",
        FileArgs => |a| Command::SelectFile { side: a.side, path: a.path };
    "open_pair": "Compare the files at the same relative path in both folders. A file missing on one side is shown as fully added or removed.",
        PairArgs => |a| Command::OpenPair { path: a.path };
    "set_text": "Replace one side's document with the given JSON text, like pasting into the editor.",
        TextArgs => |a| Command::SetText { side: a.side, text: a.text };
    "clear_side": "Empty one side's document.",
        SideArgs => |a| Command::ClearSide { side: a.side };
    "swap_sides": "Swap original and modified (folders and documents).",
        NoArgs => |_| Command::SwapSides;
    "set_options": "Change diff options. Omitted options keep their value.",
        OptionsArgs => |a| Command::SetOptions { format: a.format, sort_keys: a.sort_keys, ignore_whitespace: a.ignore_whitespace };
    "next_change": "Jump to the next changed block (wraps around) and return its lines.",
        NoArgs => |_| Command::NextChange;
    "previous_change": "Jump to the previous changed block (wraps around) and return its lines.",
        NoArgs => |_| Command::PreviousChange;
    "go_to_change": "Jump to a changed block by index and return its lines.",
        IndexArgs => |a| Command::GoToChange { index: a.index };
    "get_change": "Return the lines of one changed block, with each line's status (unchanged, added = green, removed = red), without moving the view.",
        IndexArgs => |a| Command::GetHunk { index: a.index };
    "get_structural_changes": "List semantic changes as JSON pointers (added / removed / modified) with old and new values. Needs both documents to be valid JSON. Changes to ignored keys (see get_state's `ignoring`) are left out.",
        PageArgs => |a| Command::GetChanges { offset: a.offset, limit: a.limit };
    "get_text": "Return the raw text of one side's document (truncated at 100 kB).",
        SideArgs => |a| Command::GetText { side: a.side };
    "set_view_mode": "Switch between the side-by-side diff, the tree view (both documents as one tree of keys, changes marked) and the two-pane editor.",
        ViewModeArgs => |a| Command::SetViewMode { mode: a.mode };
    "set_tree_options": "Tree view: show unchanged keys too, or only the paths that lead to changes (the default); and open every path down to each change (open_changes=true, the toolbar's Reveal changes) or fold all to the top level (false, Collapse all; how the tree starts).",
        TreeOptionsArgs => |a| Command::SetTreeOptions { show_unchanged: a.show_unchanged, open_changes: a.open_changes };
    "expand_tree_node": "Tree view: open a node to show its children.",
        TreeNodeArgs => |a| Command::SetTreeExpanded { id: a.id, expanded: true };
    "collapse_tree_node": "Tree view: close a node, hiding its children.",
        TreeNodeArgs => |a| Command::SetTreeExpanded { id: a.id, expanded: false };
    "get_tree": "The tree view's visible rows: each key with its id, depth, status (added, removed, modified, changed = changes inside, unchanged), change count and old/new values. Works in any view mode.",
        NoArgs => |_| Command::GetTree;
    "set_zoom": "Set the UI zoom factor.",
        ZoomArgs => |a| Command::SetZoom { zoom: a.zoom };
    "zoom_in": "Zoom in by 10%.",
        NoArgs => |_| Command::ZoomIn;
    "zoom_out": "Zoom out by 10%.",
        NoArgs => |_| Command::ZoomOut;
    "set_theme": "Switch between the dark and light theme.",
        ThemeArgs => |a| Command::SetTheme { theme: a.theme };
    "set_panel_visible": "Show, hide or toggle the folder explorer or the agent panel.",
        PanelArgs => |a| Command::SetPanelVisible { panel: a.panel, visible: a.visible };
    "window": "Minimize, maximize, restore or close the application window. Only close when the user asks to quit.",
        WindowArgs => |a| Command::Window { action: a.action };
    "expand_node": "Expand a folded object or array (shown as {…} or […]) in the diff, on both sides.",
        NodeArgs => |a| Command::SetExpanded { side: a.side, pointer: a.pointer, expanded: true };
    "collapse_node": "Fold an object or array to a single line, on both sides.",
        NodeArgs => |a| Command::SetExpanded { side: a.side, pointer: a.pointer, expanded: false };
    "expand_all": "Expand every object and array. Large documents become very long; prefer reveal_changes.",
        NoArgs => |_| Command::ExpandAll;
    "collapse_all": "Fold everything below the top level (the default view: one line per top-level key).",
        NoArgs => |_| Command::CollapseAll;
    "reveal_changes": "Fold everything except the objects and arrays that lead to a change, so every change is visible.",
        NoArgs => |_| Command::RevealChanges;
    "add_agents_md": "Add an instructions file (an AGENTS.md or any Markdown file) whose contents you will follow from the next message on. Same as the assistant panel's 'Add AGENTS.md…' button. Only when the user asks.",
        InstructionsArgs => |a| Command::AddAgentsMd { path: a.path };
    "remove_agents_md": "Remove an instructions file that was added with add_agents_md. Files found next to the opened folders cannot be removed, only switched off.",
        InstructionsArgs => |a| Command::RemoveAgentsMd { path: a.path };
    "set_agents_md_enabled": "Use (enabled=true) or ignore (enabled=false) one of the instruction files listed under agents_md in get_state. Only when the user asks.",
        InstructionsToggleArgs => |a| Command::SetAgentsMdEnabled { path: a.path, enabled: a.enabled };
    "ignore_changes": "Stop counting changes to a key: the node at a JSON pointer, or every key with a name (e.g. timestamps). Its changes leave the diff, the change list and PDF reports, and it stays ignored for later comparisons until stop_ignoring. Same as right-clicking a key. Only when the user asks.",
        IgnoreArgs => |a| Command::IgnoreChanges { side: a.side.unwrap_or(Side::Left), pointer: a.pointer, key: a.key };
    "stop_ignoring": "Count an ignored key's changes again (pass the pointer or key given to ignore_changes), or all of them with all=true.",
        StopIgnoringArgs => |a| stop_ignoring(a)?;
    "export_pdf": "Export the comparison as a PDF report: every change by JSON pointer with old and new values, then the side-by-side diff. Only call this when the user explicitly asks for a PDF. It opens a save dialog where the user chooses the file; if they cancel, nothing is written. Same as File > Export as PDF.",
        ExportArgs => |a| Command::ExportPdf {
            path: None,
            context: if a.full { None } else { Some(a.context.unwrap_or(crate::pdf::DEFAULT_CONTEXT)) },
            paper: a.paper.unwrap_or_default(),
        };
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn names_are_unique_and_schemas_are_objects() {
        let specs = tool_specs();
        let mut names: Vec<_> = specs.iter().map(|spec| spec.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), specs.len());
        for spec in &specs {
            assert_eq!(spec.parameters["type"], "object", "{}", spec.name);
            assert!(spec.parameters["properties"].is_object(), "{}", spec.name);
        }
    }

    #[test]
    fn parses_arguments() {
        let specs = tool_specs();
        let parse = |name: &str, args: Value| {
            let spec = specs.iter().find(|spec| spec.name == name).unwrap();
            (spec.parse)(args)
        };
        assert_eq!(
            parse("open_folder", json!({"side": "right", "path": "/tmp"})).unwrap(),
            Command::OpenFolder {
                side: Side::Right,
                path: "/tmp".into()
            }
        );
        assert_eq!(
            parse("get_structural_changes", json!({})).unwrap(),
            Command::GetChanges {
                offset: 0,
                limit: 100
            }
        );
        assert_eq!(
            parse("next_change", json!({})).unwrap(),
            Command::NextChange
        );
        assert!(parse("open_folder", json!({"side": "middle", "path": "/"})).is_err());
        assert_eq!(
            parse("ignore_changes", json!({"key": "updatedAt"})).unwrap(),
            Command::IgnoreChanges {
                side: Side::Left,
                pointer: None,
                key: Some("updatedAt".into())
            }
        );
        assert_eq!(
            parse("stop_ignoring", json!({"all": true})).unwrap(),
            Command::StopIgnoring { rule: None }
        );
        assert!(parse("stop_ignoring", json!({})).is_err(), "names nothing");
    }
}
