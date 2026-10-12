//! Pretty printing that remembers where every line sits in the document, so
//! line alignment can respect JSON structure instead of matching look-alike
//! lines (`},`, `"id": 1,`) from unrelated objects. Containers can be folded
//! to a single `{…}` / `[…]` line.

use crate::structural::push_escaped;
use serde::Serialize;
use serde_json::Value;

/// A pretty-printed document, one entry per line.
#[derive(Debug, Clone, Default)]
pub struct FormattedJson {
    pub lines: Vec<String>,
    /// Structural key per line: the line's object-key path with array indices
    /// wildcarded, so inserting an array element does not re-key every later
    /// element.
    pub keys: Vec<String>,
    /// JSON pointer (RFC 6901) of the node each line belongs to. Opening and
    /// closing brackets belong to their container.
    pub pointers: Vec<String>,
    /// For each line that opens a non-empty container (other than the root),
    /// or shows one folded: what can be expanded or collapsed there.
    pub folds: Vec<Option<FoldMark>>,
}

/// A foldable container on one line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FoldMark {
    /// The container's JSON pointer.
    pub pointer: String,
    /// Shown as `{…}` / `[…]`.
    pub collapsed: bool,
    /// Keys or elements in the container.
    pub children: usize,
    /// Lines the container takes when expanded, beyond its first.
    pub hidden_lines: usize,
}

impl FormattedJson {
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }
}

/// How to fold while printing.
pub struct FoldOptions<'a> {
    /// Whether the container at a pointer is expanded. The root always is.
    pub expanded: &'a dyn Fn(&str) -> bool,
    /// Whether anything at or below a pointer changed. A folded container
    /// that hides changes gets a key unique to its side, so line alignment
    /// reports it as a change instead of matching it with the other side.
    pub changed: &'a dyn Fn(&str) -> bool,
    /// Distinguishes the two sides in those keys.
    pub side: char,
}

/// Pretty prints `value` fully expanded, with two-space indentation,
/// optionally sorting object keys. Output is byte-identical to
/// `serde_json::to_string_pretty` when keys are not sorted.
pub fn format_json(value: &Value, sort_keys: bool) -> FormattedJson {
    format_json_folded(
        value,
        sort_keys,
        &FoldOptions {
            expanded: &|_| true,
            changed: &|_| false,
            side: ' ',
        },
    )
}

/// [`format_json`], with every container that `folds` does not expand shown
/// on one line.
pub fn format_json_folded(value: &Value, sort_keys: bool, folds: &FoldOptions) -> FormattedJson {
    let mut printer = Printer {
        sort_keys,
        folds,
        out: FormattedJson::default(),
        current: String::new(),
        path: String::new(),
        pointer: String::new(),
        depth: 0,
        pending: None,
    };
    printer.value(value);
    printer.finish_line();
    printer.out
}

struct Printer<'a> {
    sort_keys: bool,
    folds: &'a FoldOptions<'a>,
    out: FormattedJson,
    current: String,
    path: String,
    pointer: String,
    depth: usize,
    /// The fold mark of the line being built, and whether its key must be
    /// side-specific.
    pending: Option<(FoldMark, bool)>,
}

impl Printer<'_> {
    fn finish_line(&mut self) {
        let line = std::mem::take(&mut self.current);
        let (fold, unique) = match self.pending.take() {
            Some((mark, unique)) => (Some(mark), unique),
            None => (None, false),
        };
        let mut key = format!("{}\u{0}{}", self.path, line.trim());
        if unique {
            key.push('\u{1}');
            key.push(self.folds.side);
        }
        self.out.keys.push(key);
        self.out.pointers.push(self.pointer.clone());
        self.out.folds.push(fold);
        self.out.lines.push(line);
    }

    fn indent(&mut self) {
        for _ in 0..self.depth {
            self.current.push_str("  ");
        }
    }

    fn value(&mut self, value: &Value) {
        let children = match value {
            Value::Object(map) => map.len(),
            Value::Array(items) => items.len(),
            _ => 0,
        };
        if children > 0 && !self.pointer.is_empty() {
            let collapsed = !(self.folds.expanded)(&self.pointer);
            let mark = FoldMark {
                pointer: self.pointer.clone(),
                collapsed,
                children,
                hidden_lines: line_count(value) - 1,
            };
            if collapsed {
                let changed = (self.folds.changed)(&self.pointer);
                self.current
                    .push_str(if value.is_object() { "{…}" } else { "[…]" });
                self.pending = Some((mark, changed));
                return;
            }
            self.pending = Some((mark, false));
        }

        match value {
            Value::Object(map) if !map.is_empty() => {
                let mut entries: Vec<_> = map.iter().collect();
                if self.sort_keys {
                    entries.sort_by(|a, b| a.0.cmp(b.0));
                }
                self.current.push('{');
                self.finish_line();
                self.depth += 1;
                let count = entries.len();
                for (index, (key, child)) in entries.into_iter().enumerate() {
                    let (len, pointer_len) = (self.path.len(), self.pointer.len());
                    self.path.push('/');
                    self.path.push_str(key);
                    self.pointer.push('/');
                    push_escaped(&mut self.pointer, key);
                    self.indent();
                    self.current
                        .push_str(&Value::String(key.clone()).to_string());
                    self.current.push_str(": ");
                    self.value(child);
                    if index + 1 < count {
                        self.current.push(',');
                    }
                    self.finish_line();
                    self.path.truncate(len);
                    self.pointer.truncate(pointer_len);
                }
                self.depth -= 1;
                self.indent();
                self.current.push('}');
            }
            Value::Array(items) if !items.is_empty() => {
                self.current.push('[');
                self.finish_line();
                self.depth += 1;
                let len = self.path.len();
                self.path.push_str("/*");
                for (index, child) in items.iter().enumerate() {
                    let pointer_len = self.pointer.len();
                    self.pointer.push('/');
                    self.pointer.push_str(&index.to_string());
                    self.indent();
                    self.value(child);
                    if index + 1 < items.len() {
                        self.current.push(',');
                    }
                    self.finish_line();
                    self.pointer.truncate(pointer_len);
                }
                self.path.truncate(len);
                self.depth -= 1;
                self.indent();
                self.current.push(']');
            }
            Value::Object(_) => self.current.push_str("{}"),
            Value::Array(_) => self.current.push_str("[]"),
            scalar => self.current.push_str(&scalar.to_string()),
        }
    }
}

/// Lines `value` takes when fully expanded.
fn line_count(value: &Value) -> usize {
    match value {
        Value::Object(map) if !map.is_empty() => 2 + map.values().map(line_count).sum::<usize>(),
        Value::Array(items) if !items.is_empty() => 2 + items.iter().map(line_count).sum::<usize>(),
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn matches_serde_pretty_printer() {
        let value = json!({"b": [1, {"c": null, "d": []}], "a": {}, "s": "x\"y"});
        assert_eq!(
            format_json(&value, false).text(),
            serde_json::to_string_pretty(&value).unwrap()
        );
    }

    #[test]
    fn sorts_keys() {
        let value = json!({"b": 1, "a": 2});
        assert_eq!(
            format_json(&value, true).lines,
            ["{", "  \"a\": 2,", "  \"b\": 1", "}"]
        );
    }

    #[test]
    fn keys_distinguish_identical_lines_in_different_objects() {
        let value = json!({"x": {"id": 1}, "y": {"id": 1}});
        let formatted = format_json(&value, false);
        assert_eq!(formatted.lines[2], formatted.lines[5]);
        assert_ne!(formatted.keys[2], formatted.keys[5]);
    }

    #[test]
    fn folds_everything_below_level_one() {
        let value = json!({"a": {"b": {"c": 1}}, "list": [1, [2, 3]], "n": 1, "e": {}});
        let folds = FoldOptions {
            expanded: &|pointer: &str| pointer == "/a",
            changed: &|_| false,
            side: 'L',
        };
        let formatted = format_json_folded(&value, false, &folds);
        assert_eq!(
            formatted.lines,
            [
                "{",
                "  \"a\": {",
                "    \"b\": {…}",
                "  },",
                "  \"list\": […],",
                "  \"n\": 1,",
                "  \"e\": {}",
                "}"
            ]
        );
        let marks: Vec<_> = formatted
            .folds
            .iter()
            .map(|mark| {
                mark.as_ref()
                    .map(|m| (m.pointer.as_str(), m.collapsed, m.children, m.hidden_lines))
            })
            .collect();
        assert_eq!(
            marks,
            [
                None,
                Some(("/a", false, 1, 4)),
                Some(("/a/b", true, 1, 2)),
                None,
                Some(("/list", true, 2, 6)),
                None,
                None,
                None,
            ]
        );
    }

    #[test]
    fn folded_change_gets_a_side_specific_key() {
        let value = json!({"a": {"b": 1}});
        let fold = |side| FoldOptions {
            expanded: &|_| false,
            changed: &|pointer: &str| pointer == "/a",
            side,
        };
        let left = format_json_folded(&value, false, &fold('L'));
        let right = format_json_folded(&value, false, &fold('R'));
        assert_eq!(left.lines, right.lines);
        assert_ne!(left.keys[1], right.keys[1]);
        assert_eq!(left.keys[0], right.keys[0]);
    }
}
