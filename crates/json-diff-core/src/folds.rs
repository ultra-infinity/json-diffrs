//! Which containers are expanded on each side of the diff.

use std::collections::HashSet;

/// Expanded/collapsed state of every container, by JSON pointer, per side.
///
/// Stored as a default plus exceptions, so "collapse all" and "expand all"
/// are O(1) and a 10k-line document costs nothing until the user opens it.
/// The root is always expanded; [`Folds::default`] therefore shows exactly
/// the document's top-level keys or elements, each folded.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Folds {
    expand_by_default: bool,
    /// Pointers on the original side that differ from the default.
    left: HashSet<String>,
    /// Pointers on the modified side that differ from the default.
    right: HashSet<String>,
}

impl Folds {
    /// Everything expanded.
    pub fn expanded() -> Self {
        Self {
            expand_by_default: true,
            ..Self::default()
        }
    }

    /// Everything below the top level collapsed (the default).
    pub fn collapsed() -> Self {
        Self::default()
    }

    pub fn expands_by_default(&self) -> bool {
        self.expand_by_default
    }

    /// Containers whose state differs from the default, on both sides.
    pub fn exceptions(&self) -> usize {
        self.left.len() + self.right.len()
    }

    pub fn is_expanded(&self, left: bool, pointer: &str) -> bool {
        pointer.is_empty() || self.expand_by_default != self.side(left).contains(pointer)
    }

    pub fn set_expanded(&mut self, left: bool, pointer: &str, expanded: bool) {
        let default = self.expand_by_default;
        let set = if left {
            &mut self.left
        } else {
            &mut self.right
        };
        if expanded == default {
            set.remove(pointer);
        } else {
            set.insert(pointer.to_owned());
        }
    }

    fn side(&self, left: bool) -> &HashSet<String> {
        if left { &self.left } else { &self.right }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_collapses_everything_but_the_root() {
        let folds = Folds::default();
        assert!(folds.is_expanded(true, ""));
        assert!(!folds.is_expanded(true, "/a"));
        assert!(Folds::expanded().is_expanded(false, "/a/b"));
    }

    #[test]
    fn exceptions_flip_the_default_per_side() {
        let mut folds = Folds::collapsed();
        folds.set_expanded(true, "/a", true);
        assert!(folds.is_expanded(true, "/a"));
        assert!(!folds.is_expanded(false, "/a"));
        folds.set_expanded(true, "/a", false);
        assert_eq!(folds, Folds::collapsed());

        let mut folds = Folds::expanded();
        folds.set_expanded(false, "/a", false);
        assert!(!folds.is_expanded(false, "/a"));
        assert_eq!(folds.exceptions(), 1);
    }
}
