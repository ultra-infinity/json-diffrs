//! The merged tree: both documents as one tree, each node marked as added,
//! removed, modified, or holding changes further down, for a view that reads
//! like the JSON instead of like its lines.
//!
//! Array elements are matched exactly as the structural diff matches them
//! (through its [`PathMap`]), and ignored keys count as unchanged, so the
//! tree always agrees with the list of changes.

use crate::{IgnoreRule, PathMap, ignored_on};
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

/// Values in a row are cut to this many characters.
const PREVIEW_CHARS: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TreeStatus {
    Unchanged,
    /// Only in the modified document.
    Added,
    /// Only in the original document.
    Removed,
    /// A value replaced by another (including by one of another type).
    Modified,
    /// An object or array with changes inside.
    Changed,
}

/// One visible node, in display order; `depth` gives the nesting.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeRow {
    /// Names the node for expanding and collapsing: its pointer in the
    /// original document, or `+` and its pointer in the modified one if it
    /// was added.
    pub id: String,
    pub depth: usize,
    /// The key, or `[index]` for an array element.
    pub label: String,
    pub left_pointer: Option<String>,
    pub right_pointer: Option<String>,
    pub status: TreeStatus,
    /// The rule that makes this node count as unchanged, if one does.
    pub ignored_by: Option<IgnoreRule>,
    /// Changes at or below this node, as counted in the change list.
    pub changes: usize,
    /// The original value, compact and cut short; for an object or array a
    /// summary such as `{3 keys}`. `None` where the node is absent.
    pub old: Option<String>,
    /// The modified value, likewise.
    pub new: Option<String>,
    /// For a non-empty object or array: whether its children are shown.
    pub expanded: Option<bool>,
}

pub struct TreeOptions<'a> {
    /// Include unchanged nodes; otherwise only the paths to changes.
    pub show_unchanged: bool,
    /// Objects and arrays with changes inside start open, down to every
    /// change; otherwise everything starts closed, so only the top level
    /// shows.
    pub open_changes: bool,
    /// Nodes (by [`TreeRow::id`]) whose expansion the user flipped from
    /// how they start.
    pub toggled: &'a HashSet<String>,
}

/// The rows for `left` and `right`, the two parsed documents.
/// Object keys are listed alphabetically when `sort_keys` is set.
pub(crate) fn rows(
    left: &Value,
    right: &Value,
    paths: &PathMap,
    ignore: &[IgnoreRule],
    sort_keys: bool,
    options: &TreeOptions,
) -> Vec<TreeRow> {
    let mut builder = Builder {
        paths,
        ignore,
        sort_keys,
        options,
        rows: Vec::new(),
    };
    match (left, right) {
        // The document itself has no row: its members are the top level.
        (Value::Object(_), Value::Object(_)) | (Value::Array(_), Value::Array(_)) => {
            builder.children(Some(left), Some(right), "", "", 0, true);
        }
        _ => {
            let node = Node {
                label: "(the whole document)".into(),
                left: Some(left),
                right: Some(right),
                left_pointer: Some(String::new()),
                right_pointer: Some(String::new()),
            };
            builder.visit(node, 0, true);
        }
    }
    builder.rows
}

struct Builder<'a> {
    paths: &'a PathMap,
    ignore: &'a [IgnoreRule],
    sort_keys: bool,
    options: &'a TreeOptions<'a>,
    rows: Vec<TreeRow>,
}

struct Node<'v> {
    label: String,
    left: Option<&'v Value>,
    right: Option<&'v Value>,
    left_pointer: Option<String>,
    right_pointer: Option<String>,
}

impl Builder<'_> {
    /// Adds the rows of `node` and what is shown inside it (when `emit`),
    /// and returns how many changes it holds.
    fn visit(&mut self, node: Node, depth: usize, emit: bool) -> usize {
        let ignored_by = self
            .ignore
            .iter()
            .find(|rule| {
                let rule = std::slice::from_ref(*rule);
                let on = |value: Option<&Value>, pointer: &Option<String>, left: bool| {
                    value.is_some()
                        && pointer
                            .as_deref()
                            .is_some_and(|pointer| ignored_on(rule, self.paths, left, pointer))
                };
                on(node.left, &node.left_pointer, true)
                    || on(node.right, &node.right_pointer, false)
            })
            .cloned();
        let ignored = ignored_by.is_some();

        let same_kind_containers = matches!(
            (node.left, node.right),
            (Some(Value::Object(_)), Some(Value::Object(_)))
                | (Some(Value::Array(_)), Some(Value::Array(_)))
        );
        let (status, own_changes) = match (node.left, node.right) {
            _ if ignored => (TreeStatus::Unchanged, 0),
            (None, Some(_)) => (TreeStatus::Added, 1),
            (Some(_), None) => (TreeStatus::Removed, 1),
            (Some(left), Some(right)) if left == right => (TreeStatus::Unchanged, 0),
            // Decided below, from what is inside.
            _ if same_kind_containers => (TreeStatus::Changed, 0),
            _ => (TreeStatus::Modified, 1),
        };

        // What the node's children are: the side(s) it exists on, unless it
        // was replaced by a value of another kind (shown as old → new).
        let openable = match status {
            TreeStatus::Modified => false,
            _ => node.left.or(node.right).is_some_and(has_children),
        };
        let id = match (&node.left, &node.left_pointer, &node.right_pointer) {
            (Some(_), Some(pointer), _) => pointer.clone(),
            (_, _, Some(pointer)) => format!("+{pointer}"),
            _ => String::new(),
        };

        if status == TreeStatus::Changed && !ignored {
            // Count first: an object whose only differences are ignored is
            // unchanged, and then is not shown in the changes-only tree.
            let changes = self.count_children(&node);
            let status = if changes == 0 {
                TreeStatus::Unchanged
            } else {
                TreeStatus::Changed
            };
            if !emit || !self.shown(status) {
                return changes;
            }
            let open = status == TreeStatus::Changed && self.options.open_changes;
            let expanded = open != self.options.toggled.contains(&id);
            self.push(&node, id, depth, status, None, changes, Some(expanded));
            if expanded {
                self.children(
                    node.left,
                    node.right,
                    node.left_pointer.as_deref().unwrap_or_default(),
                    node.right_pointer.as_deref().unwrap_or_default(),
                    depth + 1,
                    true,
                );
            }
            return changes;
        }

        if emit && self.shown(status) {
            let expanded = openable.then(|| self.options.toggled.contains(&id));
            self.push(&node, id, depth, status, ignored_by, own_changes, expanded);
            if expanded == Some(true) {
                // An added or removed container: its contents on its one side.
                self.children(
                    node.left,
                    node.right,
                    node.left_pointer.as_deref().unwrap_or_default(),
                    node.right_pointer.as_deref().unwrap_or_default(),
                    depth + 1,
                    true,
                );
            }
        }
        own_changes
    }

    fn shown(&self, status: TreeStatus) -> bool {
        self.options.show_unchanged || status != TreeStatus::Unchanged
    }

    fn count_children(&mut self, node: &Node) -> usize {
        self.children(
            node.left,
            node.right,
            node.left_pointer.as_deref().unwrap_or_default(),
            node.right_pointer.as_deref().unwrap_or_default(),
            0,
            false,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn push(
        &mut self,
        node: &Node,
        id: String,
        depth: usize,
        status: TreeStatus,
        ignored_by: Option<IgnoreRule>,
        changes: usize,
        expanded: Option<bool>,
    ) {
        self.rows.push(TreeRow {
            id,
            depth,
            label: node.label.clone(),
            left_pointer: node.left.and(node.left_pointer.clone()),
            right_pointer: node.right.and(node.right_pointer.clone()),
            status,
            ignored_by,
            changes,
            old: node.left.map(preview),
            new: node.right.map(preview),
            expanded,
        });
    }

    /// Visits the children of the containers `left` and `right` (either may
    /// be absent), merged into one list; returns their changes.
    fn children(
        &mut self,
        left: Option<&Value>,
        right: Option<&Value>,
        left_pointer: &str,
        right_pointer: &str,
        depth: usize,
        emit: bool,
    ) -> usize {
        let mut changes = 0;
        match (left, right) {
            (Some(Value::Object(_)) | None, Some(Value::Object(_)) | None)
                if left.or(right).is_some_and(Value::is_object) =>
            {
                let empty = Map::new();
                let left_map = left.and_then(Value::as_object).unwrap_or(&empty);
                let right_map = right.and_then(Value::as_object).unwrap_or(&empty);
                for key in merged_keys(left_map, right_map, self.sort_keys) {
                    let token = escape(key);
                    let node = Node {
                        label: key.to_owned(),
                        left: left_map.get(key),
                        right: right_map.get(key),
                        left_pointer: Some(format!("{left_pointer}/{token}")),
                        right_pointer: Some(format!("{right_pointer}/{token}")),
                    };
                    changes += self.visit(node, depth, emit);
                }
            }
            _ => {
                let empty = Vec::new();
                let left_items = left.and_then(Value::as_array).unwrap_or(&empty);
                let right_items = right.and_then(Value::as_array).unwrap_or(&empty);
                let pairs = match (left, right) {
                    (Some(_), Some(_)) => self.paths.pairs(left_pointer),
                    _ => None,
                };
                for (i, j) in merged_items(left_items.len(), right_items.len(), pairs) {
                    let node = Node {
                        label: format!("[{}]", j.or(i).unwrap_or_default()),
                        left: i.map(|i| &left_items[i]),
                        right: j.map(|j| &right_items[j]),
                        left_pointer: i.map(|i| format!("{left_pointer}/{i}")),
                        right_pointer: j.map(|j| format!("{right_pointer}/{j}")),
                    };
                    changes += self.visit(node, depth, emit);
                }
            }
        }
        changes
    }
}

fn has_children(value: &Value) -> bool {
    match value {
        Value::Object(map) => !map.is_empty(),
        Value::Array(items) => !items.is_empty(),
        _ => false,
    }
}

/// Keys of both objects in the modified document's order, with removed keys
/// after the key they followed in the original; alphabetical if `sorted`.
fn merged_keys<'a>(
    left: &'a Map<String, Value>,
    right: &'a Map<String, Value>,
    sorted: bool,
) -> Vec<&'a str> {
    if sorted {
        let mut keys: Vec<&str> = left
            .keys()
            .chain(right.keys())
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        keys.dedup();
        return keys;
    }
    let left_keys: Vec<&str> = left.keys().map(String::as_str).collect();
    let position: HashMap<&str, usize> = left_keys
        .iter()
        .enumerate()
        .map(|(index, key)| (*key, index))
        .collect();
    let mut keys = Vec::with_capacity(left_keys.len() + right.len());
    let mut next = 0;
    let removed_up_to = |end: usize, next: &mut usize, keys: &mut Vec<&'a str>| {
        while *next < end {
            if !right.contains_key(left_keys[*next]) {
                keys.push(left_keys[*next]);
            }
            *next += 1;
        }
    };
    for key in right.keys() {
        if let Some(&index) = position.get(key.as_str()) {
            removed_up_to(index, &mut next, &mut keys);
            next = next.max(index + 1);
        }
        keys.push(key.as_str());
    }
    removed_up_to(left_keys.len(), &mut next, &mut keys);
    keys
}

/// Elements of both arrays in order, as (original index, modified index):
/// matched pairs, removals and additions where the structural diff placed
/// them. Without a pairing (the arrays were equal) elements pair by index.
fn merged_items(
    left: usize,
    right: usize,
    pairs: Option<&HashMap<usize, usize>>,
) -> Vec<(Option<usize>, Option<usize>)> {
    let mut items = Vec::with_capacity(left.max(right));
    let mut j = 0;
    for i in 0..left {
        let partner = match pairs {
            Some(pairs) => pairs.get(&i).copied(),
            None => (i < right).then_some(i),
        };
        match partner {
            Some(target) => {
                items.extend((j..target).map(|added| (None, Some(added))));
                items.push((Some(i), Some(target)));
                j = target + 1;
            }
            None => items.push((Some(i), None)),
        }
    }
    items.extend((j..right).map(|added| (None, Some(added))));
    items
}

/// A value for one row: scalars as compact JSON, containers summarised.
fn preview(value: &Value) -> String {
    match value {
        Value::Object(map) => match map.len() {
            0 => "{}".into(),
            1 => "{1 key}".into(),
            count => format!("{{{count} keys}}"),
        },
        Value::Array(items) => match items.len() {
            0 => "[]".into(),
            1 => "[1 item]".into(),
            count => format!("[{count} items]"),
        },
        scalar => {
            let text = scalar.to_string();
            match text.char_indices().nth(PREVIEW_CHARS) {
                Some((end, _)) => format!("{}…", &text[..end]),
                None => text,
            }
        }
    }
}

fn escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structural_diff_mapped;
    use serde_json::json;

    fn tree(
        left: Value,
        right: Value,
        show_unchanged: bool,
        toggled: &[&str],
        ignore: &[IgnoreRule],
    ) -> Vec<TreeRow> {
        let (_, paths) = structural_diff_mapped(&left, &right);
        let toggled = toggled.iter().map(|id| id.to_string()).collect();
        let options = TreeOptions {
            show_unchanged,
            open_changes: true,
            toggled: &toggled,
        };
        rows(&left, &right, &paths, ignore, false, &options)
    }

    #[test]
    fn it_starts_folded_to_the_top_level() {
        let (left, right) = sample();
        let (_, paths) = structural_diff_mapped(&left, &right);
        let toggled = HashSet::from(["/list".to_string()]);
        let options = TreeOptions {
            show_unchanged: false,
            open_changes: false,
            toggled: &toggled,
        };
        assert_eq!(
            lines(&rows(&left, &right, &paths, &[], false, &options)),
            [
                "runtime Changed 2 {3 keys} → {3 keys}",
                "features Changed 2 [3 items] → [3 items]",
                "legacy Removed 1 {1 key} → -",
                "list Changed 2 [2 items] → [3 items]",
                "  [0] Added 1 - → {1 key}",
                "  [2] Changed 1 {2 keys} → {2 keys}",
                "tracing Added 1 - → {1 key}",
            ],
            "only `list`, opened by hand"
        );
    }

    /// `depth label status changes old → new`, one per row.
    fn lines(rows: &[TreeRow]) -> Vec<String> {
        rows.iter()
            .map(|row| {
                format!(
                    "{}{} {:?} {} {} → {}",
                    "  ".repeat(row.depth),
                    row.label,
                    row.status,
                    row.changes,
                    row.old.as_deref().unwrap_or("-"),
                    row.new.as_deref().unwrap_or("-"),
                )
            })
            .collect()
    }

    fn sample() -> (Value, Value) {
        (
            json!({
                "name": "app",
                "runtime": {"replicas": 3, "timeout": 30, "region": "eu"},
                "features": ["a", "b", "c"],
                "legacy": {"x": 1},
                "list": [{"id": 1, "v": 1}, {"id": 2, "v": 2}]
            }),
            json!({
                "name": "app",
                "runtime": {"replicas": 5, "timeout": "30s", "region": "eu"},
                "features": ["a", "c", "d"],
                "list": [{"id": 0}, {"id": 1, "v": 1}, {"id": 2, "v": 3}],
                "tracing": {"on": true}
            }),
        )
    }

    #[test]
    fn only_the_paths_to_changes_are_shown() {
        let (left, right) = sample();
        assert_eq!(
            lines(&tree(left, right, false, &[], &[])),
            [
                "runtime Changed 2 {3 keys} → {3 keys}",
                "  replicas Modified 1 3 → 5",
                "  timeout Modified 1 30 → \"30s\"",
                "features Changed 2 [3 items] → [3 items]",
                "  [1] Removed 1 \"b\" → -",
                "  [2] Added 1 - → \"d\"",
                "legacy Removed 1 {1 key} → -",
                "list Changed 2 [2 items] → [3 items]",
                "  [0] Added 1 - → {1 key}",
                "  [2] Changed 1 {2 keys} → {2 keys}",
                "    v Modified 1 2 → 3",
                "tracing Added 1 - → {1 key}",
            ]
        );
    }

    #[test]
    fn unchanged_nodes_appear_closed_on_request_and_open_when_toggled() {
        let (left, right) = sample();
        let rows = tree(left.clone(), right.clone(), true, &[], &[]);
        assert_eq!(rows[0].label, "name");
        assert_eq!(rows[0].status, TreeStatus::Unchanged);
        let list_one = rows.iter().find(|row| row.id == "/list/0").unwrap();
        assert_eq!(
            (list_one.label.as_str(), list_one.expanded),
            ("[1]", Some(false))
        );

        // Toggling opens a closed node and closes an open one.
        let rows = tree(left, right, false, &["/runtime", "+/tracing"], &[]);
        let runtime = rows.iter().position(|row| row.id == "/runtime").unwrap();
        assert_eq!(rows[runtime].expanded, Some(false));
        assert_eq!(
            rows[runtime + 1].label,
            "features",
            "its children are hidden"
        );
        assert_eq!(rows[runtime].changes, 2, "but still counted");
        let tracing = rows.iter().position(|row| row.id == "+/tracing").unwrap();
        assert_eq!(rows[tracing + 1].label, "on");
        assert_eq!(rows[tracing + 1].status, TreeStatus::Added);
    }

    #[test]
    fn ignored_keys_are_unchanged() {
        let (left, right) = sample();
        let ignore = [
            IgnoreRule::Key {
                key: "timeout".into(),
            },
            IgnoreRule::Pointer {
                pointer: "/list".into(),
            },
        ];
        let rows = tree(left.clone(), right.clone(), false, &[], &ignore);
        assert!(
            rows.iter()
                .all(|row| row.label != "timeout" && row.label != "list")
        );
        assert_eq!(rows[0].changes, 1, "runtime: just replicas");

        let all = tree(left, right, true, &[], &ignore);
        let list = all.iter().find(|row| row.label == "list").unwrap();
        assert_eq!(list.ignored_by, Some(ignore[1].clone()));
        assert_eq!(list.status, TreeStatus::Unchanged);
    }

    #[test]
    fn keys_keep_the_modified_order_with_removals_in_place() {
        let left = json!({"a": 1, "gone": 2, "b": 3});
        let right = json!({"new": 0, "a": 1, "b": 3});
        let labels: Vec<_> = tree(left, right, true, &[], &[])
            .into_iter()
            .map(|row| row.label)
            .collect();
        assert_eq!(labels, ["new", "a", "gone", "b"]);
    }

    #[test]
    fn a_whole_document_of_another_kind_is_one_row() {
        let rows = tree(json!([1]), json!({"a": 1}), false, &[], &[]);
        assert_eq!(
            lines(&rows),
            ["(the whole document) Modified 1 [1 item] → {1 key}"]
        );
    }
}
