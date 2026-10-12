//! Pretty printing that remembers where every line sits in the document, so
//! line alignment can respect JSON structure instead of matching look-alike
//! lines (`},`, `"id": 1,`) from unrelated objects.

use crate::structural::push_escaped;
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
}

impl FormattedJson {
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }
}

/// Pretty prints `value` with two-space indentation, optionally sorting object
/// keys. Output is byte-identical to `serde_json::to_string_pretty` when keys
/// are not sorted.
pub fn format_json(value: &Value, sort_keys: bool) -> FormattedJson {
    let mut printer = Printer {
        sort_keys,
        out: FormattedJson::default(),
        current: String::new(),
        path: String::new(),
        pointer: String::new(),
        depth: 0,
    };
    printer.value(value);
    printer.finish_line();
    printer.out
}

struct Printer {
    sort_keys: bool,
    out: FormattedJson,
    current: String,
    path: String,
    pointer: String,
    depth: usize,
}

impl Printer {
    fn finish_line(&mut self) {
        let line = std::mem::take(&mut self.current);
        self.out
            .keys
            .push(format!("{}\u{0}{}", self.path, line.trim()));
        self.out.pointers.push(self.pointer.clone());
        self.out.lines.push(line);
    }

    fn indent(&mut self) {
        for _ in 0..self.depth {
            self.current.push_str("  ");
        }
    }

    fn value(&mut self, value: &Value) {
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
}
