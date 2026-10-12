//! Structural (semantic) JSON diff: walks both documents and reports every
//! leaf or subtree that was added, removed or modified, addressed by its
//! JSON pointer (RFC 6901).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use similar::{Algorithm, DiffOp, capture_diff_slices};
use std::collections::BTreeSet;
use std::fmt::Display;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Added,
    Removed,
    Modified,
}

impl Display for ChangeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ChangeKind::Added => "added",
            ChangeKind::Removed => "removed",
            ChangeKind::Modified => "modified",
        })
    }
}

/// One semantic difference between the two documents.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Change {
    /// JSON pointer of the changed node; `""` is the document root. For
    /// removals it points into the original document, otherwise into the
    /// modified one.
    pub path: String,
    /// Where a modified node sits in the original document, when array
    /// insertions or removals before it moved it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
    pub kind: ChangeKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new: Option<Value>,
}

impl Change {
    /// The pointer of this change in the original document, if it has one.
    pub fn left_path(&self) -> Option<&str> {
        match self.kind {
            ChangeKind::Added => None,
            ChangeKind::Removed => Some(&self.path),
            ChangeKind::Modified => Some(self.old_path.as_deref().unwrap_or(&self.path)),
        }
    }

    /// The pointer of this change in the modified document, if it has one.
    pub fn right_path(&self) -> Option<&str> {
        match self.kind {
            ChangeKind::Removed => None,
            _ => Some(&self.path),
        }
    }
}

/// Every difference between `old` and `new`, in document order.
///
/// Objects are compared key by key. Array elements are aligned by longest
/// common subsequence, so inserting an element reports one addition rather
/// than modifying every element after it; elements that were replaced in place
/// are compared recursively. A node whose type changed is reported once as
/// `Modified` rather than descended into.
pub fn structural_diff(old: &Value, new: &Value) -> Vec<Change> {
    let mut changes = Vec::new();
    let mut paths = Paths::default();
    diff_value(&mut paths, old, new, &mut changes);
    changes
}

/// The current pointer in each document; they differ once array elements
/// have been inserted or removed before the current position.
#[derive(Default)]
struct Paths {
    old: String,
    new: String,
}

impl Paths {
    fn push(&mut self, old: &str, new: &str) -> (usize, usize) {
        let lengths = (self.old.len(), self.new.len());
        self.old.push('/');
        push_escaped(&mut self.old, old);
        self.new.push('/');
        push_escaped(&mut self.new, new);
        lengths
    }

    fn pop(&mut self, (old, new): (usize, usize)) {
        self.old.truncate(old);
        self.new.truncate(new);
    }

    fn change(&self, kind: ChangeKind, old: Option<&Value>, new: Option<&Value>) -> Change {
        let (path, old_path) = match kind {
            ChangeKind::Removed => (self.old.clone(), None),
            _ => (
                self.new.clone(),
                (self.old != self.new).then(|| self.old.clone()),
            ),
        };
        Change {
            path,
            old_path: if kind == ChangeKind::Modified {
                old_path
            } else {
                None
            },
            kind,
            old: old.cloned(),
            new: new.cloned(),
        }
    }
}

fn diff_value(paths: &mut Paths, old: &Value, new: &Value, out: &mut Vec<Change>) {
    match (old, new) {
        (Value::Object(old), Value::Object(new)) => diff_object(paths, old, new, out),
        (Value::Array(old), Value::Array(new)) => diff_array(paths, old, new, out),
        (old, new) if old == new => {}
        (old, new) => out.push(paths.change(ChangeKind::Modified, Some(old), Some(new))),
    }
}

fn diff_object(
    paths: &mut Paths,
    old: &Map<String, Value>,
    new: &Map<String, Value>,
    out: &mut Vec<Change>,
) {
    // Old keys first in their own order, then keys only present in `new`.
    let mut seen = BTreeSet::new();
    let keys = old
        .keys()
        .chain(new.keys())
        .filter(|key| seen.insert(key.as_str()));

    for key in keys {
        let mark = paths.push(key, key);
        match (old.get(key), new.get(key)) {
            (Some(old), Some(new)) => diff_value(paths, old, new, out),
            (Some(old), None) => out.push(paths.change(ChangeKind::Removed, Some(old), None)),
            (None, Some(new)) => out.push(paths.change(ChangeKind::Added, None, Some(new))),
            (None, None) => unreachable!("key came from one of the two maps"),
        }
        paths.pop(mark);
    }
}

fn diff_array(paths: &mut Paths, old: &[Value], new: &[Value], out: &mut Vec<Change>) {
    let old_keys: Vec<String> = old.iter().map(Value::to_string).collect();
    let new_keys: Vec<String> = new.iter().map(Value::to_string).collect();

    for op in capture_diff_slices(Algorithm::Patience, &old_keys, &new_keys) {
        let (old_range, new_range) = (op.old_range(), op.new_range());
        // Replaced elements are compared pairwise; the surplus on either
        // side is a plain removal or addition.
        let paired = match op {
            DiffOp::Equal { .. } => continue,
            DiffOp::Replace { .. } => old_range.len().min(new_range.len()),
            _ => 0,
        };
        for offset in 0..paired {
            let (i, j) = (old_range.start + offset, new_range.start + offset);
            let mark = paths.push(&i.to_string(), &j.to_string());
            diff_value(paths, &old[i], &new[j], out);
            paths.pop(mark);
        }
        for (i, value) in old
            .iter()
            .enumerate()
            .take(old_range.end)
            .skip(old_range.start + paired)
        {
            // The element's position in `new` does not matter for a removal.
            let mark = paths.push(&i.to_string(), &i.to_string());
            out.push(paths.change(ChangeKind::Removed, Some(value), None));
            paths.pop(mark);
        }
        for (j, value) in new
            .iter()
            .enumerate()
            .take(new_range.end)
            .skip(new_range.start + paired)
        {
            let mark = paths.push(&j.to_string(), &j.to_string());
            out.push(paths.change(ChangeKind::Added, None, Some(value)));
            paths.pop(mark);
        }
    }
}

pub(crate) fn push_escaped(path: &mut String, segment: &str) {
    for c in segment.chars() {
        match c {
            '~' => path.push_str("~0"),
            '/' => path.push_str("~1"),
            c => path.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn summary(changes: &[Change]) -> Vec<(&str, ChangeKind)> {
        changes.iter().map(|c| (c.path.as_str(), c.kind)).collect()
    }

    #[test]
    fn reports_added_removed_and_modified() {
        let old = json!({"a": 1, "b": {"c": true}, "list": [1, 2, 3], "gone": null});
        let new = json!({"a": 2, "b": {"c": true, "d": "x"}, "list": [1, 2], "a/b": 0});
        assert_eq!(
            summary(&structural_diff(&old, &new)),
            vec![
                ("/a", ChangeKind::Modified),
                ("/b/d", ChangeKind::Added),
                ("/list/2", ChangeKind::Removed),
                ("/gone", ChangeKind::Removed),
                ("/a~1b", ChangeKind::Added),
            ]
        );
    }

    #[test]
    fn type_change_is_single_modification() {
        let changes = structural_diff(&json!({"x": [1]}), &json!({"x": {"0": 1}}));
        assert_eq!(summary(&changes), vec![("/x", ChangeKind::Modified)]);
    }

    #[test]
    fn identical_documents_have_no_changes() {
        let value = json!({"a": [1, {"b": null}]});
        assert!(structural_diff(&value, &value).is_empty());
    }

    #[test]
    fn array_insertion_is_one_addition() {
        let changes = structural_diff(&json!([1, 2, 3]), &json!([0, 1, 2, 3]));
        assert_eq!(summary(&changes), vec![("/0", ChangeKind::Added)]);
    }

    #[test]
    fn modified_element_after_insertion_keeps_both_paths() {
        let old = json!([{"a": 1}, {"b": 1}]);
        let new = json!([{"z": 0}, {"a": 1}, {"b": 2}]);
        let changes = structural_diff(&old, &new);
        assert_eq!(
            summary(&changes),
            vec![("/0", ChangeKind::Added), ("/2/b", ChangeKind::Modified)]
        );
        assert_eq!(changes[1].left_path(), Some("/1/b"));
        assert_eq!(changes[1].right_path(), Some("/2/b"));
    }
}
