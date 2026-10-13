//! Keys whose changes the user chose to ignore. An ignored node, and
//! everything inside it, no longer counts as changed: its changes are left
//! out of the structural list, its lines are not coloured, and change blocks
//! made only of ignored lines disappear.

use serde::{Deserialize, Serialize};
use std::fmt::{self, Display};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IgnoreRule {
    /// The node at this JSON pointer (in either document) and everything
    /// inside it.
    Pointer { pointer: String },
    /// Every key with this name, at any depth, and everything inside it.
    Key { key: String },
}

impl IgnoreRule {
    /// Whether the node at `pointer` is, or is inside, what this rule
    /// ignores.
    pub fn matches(&self, pointer: &str) -> bool {
        match self {
            IgnoreRule::Pointer { pointer: ignored } => {
                pointer
                    .strip_prefix(ignored.as_str())
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
                    // The root pointer "" would ignore everything.
                    && !ignored.is_empty()
            }
            IgnoreRule::Key { key } => pointer
                .split('/')
                .skip(1)
                .any(|token| unescape(token) == *key),
        }
    }
}

impl Display for IgnoreRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IgnoreRule::Pointer { pointer } => write!(f, "{pointer}"),
            IgnoreRule::Key { key } => write!(f, "\"{key}\" everywhere"),
        }
    }
}

/// Whether any of `rules` ignores the node at `pointer`.
pub fn is_ignored(rules: &[IgnoreRule], pointer: &str) -> bool {
    rules.iter().any(|rule| rule.matches(pointer))
}

/// The name of the last key in `pointer`, if it ends in one (not the root).
pub fn last_key(pointer: &str) -> Option<String> {
    let (_, token) = pointer.rsplit_once('/')?;
    Some(unescape(token))
}

/// A JSON pointer token back to the key it names (RFC 6901).
fn unescape(token: &str) -> String {
    token.replace("~1", "/").replace("~0", "~")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pointer(pointer: &str) -> IgnoreRule {
        IgnoreRule::Pointer {
            pointer: pointer.into(),
        }
    }

    fn key(key: &str) -> IgnoreRule {
        IgnoreRule::Key { key: key.into() }
    }

    #[test]
    fn a_pointer_covers_its_node_and_what_is_inside() {
        let rule = pointer("/services/0/env");
        assert!(rule.matches("/services/0/env"));
        assert!(rule.matches("/services/0/env/LOG"));
        assert!(!rule.matches("/services/0/environment"), "a longer key");
        assert!(!rule.matches("/services/0"), "its parent");
        assert!(
            !pointer("").matches("/anything"),
            "never the whole document"
        );
    }

    #[test]
    fn a_key_name_matches_at_any_depth() {
        let rule = key("updatedAt");
        assert!(rule.matches("/updatedAt"));
        assert!(rule.matches("/items/3/updatedAt"));
        assert!(rule.matches("/items/3/updatedAt/seconds"));
        assert!(!rule.matches("/items/3/created"));
        assert!(key("a/b").matches("/x/a~1b"), "escaped names");
    }

    #[test]
    fn last_keys_are_unescaped() {
        assert_eq!(last_key("/a/b~1c").as_deref(), Some("b/c"));
        assert_eq!(last_key(""), None);
    }

    #[test]
    fn rules_read_naturally() {
        assert_eq!(pointer("/a/b").to_string(), "/a/b");
        assert_eq!(key("port").to_string(), "\"port\" everywhere");
    }
}
