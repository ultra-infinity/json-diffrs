//! Structural (semantic) JSON diff: walks both documents and reports every
//! leaf or subtree that was added, removed or modified, addressed by its
//! JSON pointer (RFC 6901).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use similar::{Algorithm, DiffOp, capture_diff_slices};
use std::collections::{BTreeSet, HashMap};
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
    structural_diff_mapped(old, new).0
}

/// [`structural_diff`], plus the [`PathMap`] that translates pointers between
/// the two documents.
pub fn structural_diff_mapped(old: &Value, new: &Value) -> (Vec<Change>, PathMap) {
    let mut changes = Vec::new();
    let mut paths = Paths::default();
    diff_value(&mut paths, old, new, &mut changes);
    (changes, paths.map)
}

/// Which array element in one document corresponds to which in the other.
/// Objects need no mapping: a key is the same on both sides.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PathMap {
    /// Keyed by the pointer of an array in the original document.
    left_to_right: HashMap<String, HashMap<usize, usize>>,
    /// Keyed by the pointer of an array in the modified document.
    right_to_left: HashMap<String, HashMap<usize, usize>>,
}

impl PathMap {
    /// The pointer of the node at `pointer` in the other document, or `None`
    /// when an array element on the way has no counterpart. Nodes that only
    /// exist on one side translate to a pointer the other side lacks.
    pub fn translate(&self, pointer: &str, from_left: bool) -> Option<String> {
        let maps = if from_left {
            &self.left_to_right
        } else {
            &self.right_to_left
        };
        let (mut source, mut target) = (String::new(), String::new());
        for segment in pointer.split('/').skip(1) {
            let mapped = match maps.get(&source) {
                Some(indices) => indices.get(&segment.parse().ok()?)?.to_string(),
                None => segment.to_owned(),
            };
            source.push('/');
            source.push_str(segment);
            target.push('/');
            target.push_str(&mapped);
        }
        Some(target)
    }

    fn pair(&mut self, old_array: &str, new_array: &str, i: usize, j: usize) {
        self.left_to_right
            .entry(old_array.to_owned())
            .or_default()
            .insert(i, j);
        self.right_to_left
            .entry(new_array.to_owned())
            .or_default()
            .insert(j, i);
    }
}

/// The current pointer in each document; they differ once array elements
/// have been inserted or removed before the current position.
#[derive(Default)]
struct Paths {
    old: String,
    new: String,
    map: PathMap,
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
        if let DiffOp::Equal { .. } = op {
            for offset in 0..old_range.len() {
                let (i, j) = (old_range.start + offset, new_range.start + offset);
                paths.map.pair(&paths.old, &paths.new, i, j);
            }
            continue;
        }
        let block = align_replaced(&old[old_range.clone()], &new[new_range.clone()]);
        for (old_offset, new_offset) in block {
            let i = old_offset.map(|offset| old_range.start + offset);
            let j = new_offset.map(|offset| new_range.start + offset);
            match (i, j) {
                (Some(i), Some(j)) => {
                    paths.map.pair(&paths.old, &paths.new, i, j);
                    let mark = paths.push(&i.to_string(), &j.to_string());
                    diff_value(paths, &old[i], &new[j], out);
                    paths.pop(mark);
                }
                (Some(i), None) => {
                    // The element's position in `new` does not matter for a removal.
                    let mark = paths.push(&i.to_string(), &i.to_string());
                    out.push(paths.change(ChangeKind::Removed, Some(&old[i]), None));
                    paths.pop(mark);
                }
                (None, Some(j)) => {
                    let mark = paths.push(&j.to_string(), &j.to_string());
                    out.push(paths.change(ChangeKind::Added, None, Some(&new[j])));
                    paths.pop(mark);
                }
                (None, None) => {}
            }
        }
    }
}

/// Blocks larger than this (old × new elements) are paired by position only.
const ALIGN_MAX_CELLS: usize = 250_000;

/// Decides which elements of a replaced block correspond, in order: each
/// entry is (old, new), with `None` for a removal or an addition.
///
/// Elements are first matched by similarity (see [`similarity`]), keeping the
/// order on both sides and maximising the total; whatever falls between two
/// matches is paired by position, the surplus being added or removed. So an
/// element inserted next to a modified one is reported as one addition and
/// one modification, not two modifications.
fn align_replaced(old: &[Value], new: &[Value]) -> Vec<(Option<usize>, Option<usize>)> {
    let matches = if old.len() * new.len() <= ALIGN_MAX_CELLS {
        best_matches(old, new)
    } else {
        Vec::new()
    };

    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    for (mi, mj) in matches.into_iter().chain([(old.len(), new.len())]) {
        // The gap before this match: pair by position, then the surplus.
        let paired = (mi - i).min(mj - j);
        out.extend((0..paired).map(|k| (Some(i + k), Some(j + k))));
        out.extend((i + paired..mi).map(|k| (Some(k), None)));
        out.extend((j + paired..mj).map(|k| (None, Some(k))));
        if mi < old.len() && mj < new.len() {
            out.push((Some(mi), Some(mj)));
        }
        (i, j) = (mi + 1, mj + 1);
    }
    out
}

/// The order-preserving set of (old, new) pairs with the highest total
/// similarity, counting only pairs that share something.
fn best_matches(old: &[Value], new: &[Value]) -> Vec<(usize, usize)> {
    let (m, n) = (old.len(), new.len());
    let mut score = vec![vec![0usize; n + 1]; m + 1];
    for i in 1..=m {
        for j in 1..=n {
            let similar = similarity(&old[i - 1], &new[j - 1]);
            let diagonal = if similar > 0 {
                score[i - 1][j - 1] + similar
            } else {
                0
            };
            score[i][j] = score[i - 1][j].max(score[i][j - 1]).max(diagonal);
        }
    }
    let mut pairs = Vec::new();
    let (mut i, mut j) = (m, n);
    while i > 0 && j > 0 {
        if score[i][j] == score[i - 1][j] {
            i -= 1;
        } else if score[i][j] == score[i][j - 1] {
            j -= 1;
        } else {
            pairs.push((i - 1, j - 1));
            i -= 1;
            j -= 1;
        }
    }
    pairs.reverse();
    pairs
}

/// How much two array elements have in common: equal fields for objects,
/// shared elements for arrays. Scalars score 0 (they are paired by position).
fn similarity(old: &Value, new: &Value) -> usize {
    match (old, new) {
        (Value::Object(old), Value::Object(new)) => old
            .iter()
            .filter(|(key, value)| new.get(*key) == Some(value))
            .count(),
        (Value::Array(old), Value::Array(new)) if old.len() * new.len() <= 10_000 => {
            old.iter().filter(|value| new.contains(value)).count()
        }
        _ => 0,
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

    #[test]
    fn insertion_next_to_a_modified_element_pairs_by_similarity() {
        // No element is identical, so the whole array is one replaced block.
        let old = json!([{"id": 1, "tags": ["a"]}]);
        let new = json!([{"id": 0}, {"id": 1, "tags": ["a", "b"]}]);
        let (changes, map) = structural_diff_mapped(&old, &new);
        assert_eq!(
            summary(&changes),
            vec![("/0", ChangeKind::Added), ("/1/tags/1", ChangeKind::Added)]
        );
        assert_eq!(map.translate("/0", true).as_deref(), Some("/1"));

        // Scalars share nothing, so they still pair by position.
        let changes = structural_diff(&json!([1, 2, 3]), &json!([1, 5, 3]));
        assert_eq!(summary(&changes), vec![("/1", ChangeKind::Modified)]);
    }

    #[test]
    fn path_map_follows_shifted_array_elements() {
        let old = json!({"list": [{"a": [1, 2]}, {"b": 1}]});
        let new = json!({"list": [{"z": 0}, {"a": [1, 2]}, {"b": 2}], "extra": true});
        let (_, map) = structural_diff_mapped(&old, &new);
        // Equal element moved from index 0 to 1, including its descendants.
        assert_eq!(
            map.translate("/list/0/a/1", true).as_deref(),
            Some("/list/1/a/1")
        );
        assert_eq!(
            map.translate("/list/1/a", false).as_deref(),
            Some("/list/0/a")
        );
        // Replaced in place: compared pairwise, so it maps too.
        assert_eq!(map.translate("/list/2", false).as_deref(), Some("/list/1"));
        // The inserted element has no counterpart.
        assert_eq!(map.translate("/list/0", false), None);
        // Object keys map to themselves, even when only one side has them.
        assert_eq!(map.translate("/extra", false).as_deref(), Some("/extra"));
        assert_eq!(map.translate("", true).as_deref(), Some(""));
    }
}
