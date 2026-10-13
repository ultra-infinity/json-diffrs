//! AGENTS.md support (<https://agents.md>): project instructions for the
//! assistant, written as plain Markdown next to the files being compared.
//!
//! Files are found the way coding agents look for them:
//! - in an opened folder, every `AGENTS.md` from the folder's root down to
//!   the directory of the file being compared;
//! - for a file opened on its own, every `AGENTS.md` from the file's
//!   directory up to the enclosing repository (the nearest directory with a
//!   `.git`), or only the file's directory outside a repository.
//!
//! The user can also add any instructions file by hand, and switch each file
//! off. Files are listed outermost first, so the closest one comes last and
//! takes precedence.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = "AGENTS.md";

/// Larger files are cut off here, as coding agents do, so one huge file
/// cannot crowd the documents out of the model's context.
pub const MAX_BYTES: usize = 32 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Found next to an opened folder or file.
    Found,
    /// Added by the user (or the assistant on the user's behalf).
    Added,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Found => "found",
            Source::Added => "added",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstructionFile {
    pub path: PathBuf,
    pub source: Source,
    pub enabled: bool,
}

/// The instruction files in play, and the user's choices about them.
#[derive(Debug, Clone, Default)]
pub struct AgentsMd {
    found: Vec<PathBuf>,
    added: Vec<PathBuf>,
    disabled: HashSet<PathBuf>,
}

impl AgentsMd {
    /// Re-runs discovery for the opened folders and files, one pair per side.
    pub fn refresh(&mut self, sides: [(Option<&Path>, Option<&Path>); 2]) {
        let mut found = Vec::new();
        for (root, file) in sides {
            for path in discover(root, file) {
                if !found.contains(&path) {
                    found.push(path);
                }
            }
        }
        self.found = found;
    }

    /// Every file, found ones first (outermost to innermost), then added
    /// ones in the order they were added.
    pub fn files(&self) -> Vec<InstructionFile> {
        let found = self.found.iter().map(|path| (path, Source::Found));
        let added = self
            .added
            .iter()
            .filter(|path| !self.found.contains(path))
            .map(|path| (path, Source::Added));
        found
            .chain(added)
            .map(|(path, source)| InstructionFile {
                path: path.clone(),
                source,
                enabled: !self.disabled.contains(path),
            })
            .collect()
    }

    pub fn add(&mut self, path: &Path) -> Result<PathBuf, String> {
        let path = std::fs::canonicalize(path)
            .map_err(|error| format!("cannot open {}: {error}", path.display()))?;
        if !path.is_file() {
            return Err(format!("{} is not a file", path.display()));
        }
        self.disabled.remove(&path);
        if !self.added.contains(&path) {
            self.added.push(path.clone());
        }
        Ok(path)
    }

    pub fn remove(&mut self, path: &Path) -> Result<(), String> {
        let before = self.added.len();
        self.added.retain(|added| !same_file(added, path));
        if self.added.len() == before {
            return Err(format!(
                "{} was not added by hand; switch it off instead",
                path.display()
            ));
        }
        Ok(())
    }

    pub fn set_enabled(&mut self, path: &Path, enabled: bool) -> Result<(), String> {
        let file = self
            .files()
            .into_iter()
            .find(|file| same_file(&file.path, path))
            .ok_or_else(|| format!("{} is not an instructions file in use", path.display()))?;
        if enabled {
            self.disabled.remove(&file.path);
        } else {
            self.disabled.insert(file.path);
        }
        Ok(())
    }

    /// The enabled files' contents as a section for the assistant's system
    /// prompt, or an empty string when there are none. Unreadable files are
    /// skipped.
    pub fn prompt_section(&self) -> String {
        let mut section = String::new();
        // Both sides often hold the same AGENTS.md: send its text once.
        let mut seen = HashSet::new();
        for file in self.files().into_iter().filter(|file| file.enabled) {
            let Ok(bytes) = std::fs::read(&file.path) else {
                continue;
            };
            if !seen.insert(bytes.clone()) {
                continue;
            }
            let truncated = bytes.len() > MAX_BYTES;
            let text = String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_BYTES)]);
            section.push_str(&format!(
                "\n### {}\n\n{}\n",
                file.path.display(),
                text.trim()
            ));
            if truncated {
                section.push_str(&format!("\n(cut off after {MAX_BYTES} bytes)\n"));
            }
        }
        if section.is_empty() {
            return section;
        }
        format!(
            "\n\n## Project instructions (AGENTS.md)\n\n\
             The user's project provides these instructions. Follow them when they \
             say how to read, compare or explain these documents. The user's own \
             messages take precedence over them, and a file listed later (closer \
             to the compared files) takes precedence over an earlier one. Do not \
             add, remove or switch off instruction files unless the user asks.\n{section}"
        )
    }
}

/// The `AGENTS.md` files that apply to `file` (opened from folder `root`, if
/// any), outermost first.
pub fn discover(root: Option<&Path>, file: Option<&Path>) -> Vec<PathBuf> {
    let directories: Vec<&Path> = match (root, file) {
        (Some(root), Some(file)) if file.starts_with(root) => {
            // From the root down to the file's directory.
            let mut chain: Vec<&Path> = file
                .ancestors()
                .skip(1)
                .take_while(|directory| directory.starts_with(root))
                .collect();
            chain.reverse();
            chain
        }
        (Some(root), _) => vec![root],
        (None, Some(file)) => {
            let Some(directory) = file.parent() else {
                return Vec::new();
            };
            // Up to the repository root, or just the file's directory.
            match directory
                .ancestors()
                .position(|ancestor| ancestor.join(".git").exists())
            {
                Some(depth) => {
                    let mut chain: Vec<&Path> = directory.ancestors().take(depth + 1).collect();
                    chain.reverse();
                    chain
                }
                None => vec![directory],
            }
        }
        (None, None) => Vec::new(),
    };
    directories
        .into_iter()
        .map(|directory| directory.join(FILE_NAME))
        .filter(|path| path.is_file())
        .collect()
}

fn same_file(a: &Path, b: &Path) -> bool {
    a == b || std::fs::canonicalize(b).is_ok_and(|b| a == b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn folder_files_run_from_root_to_the_compared_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::write(root.join(FILE_NAME), "root rules").unwrap();
        fs::write(root.join("a/b").join(FILE_NAME), "inner rules").unwrap();
        fs::write(root.join("a/b/x.json"), "{}").unwrap();

        let file = root.join("a/b/x.json");
        assert_eq!(
            discover(Some(&root), Some(&file)),
            vec![root.join(FILE_NAME), root.join("a/b").join(FILE_NAME)]
        );
        // Without a file, only the root's.
        assert_eq!(discover(Some(&root), None), vec![root.join(FILE_NAME)]);
    }

    #[test]
    fn a_lone_file_looks_up_to_its_repository() {
        let dir = tempfile::tempdir().unwrap();
        let repo = fs::canonicalize(dir.path()).unwrap().join("repo");
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::create_dir_all(repo.join("configs")).unwrap();
        fs::write(repo.join(FILE_NAME), "repo rules").unwrap();
        fs::write(repo.join("configs/x.json"), "{}").unwrap();
        // Outside the repository: never read.
        fs::write(repo.parent().unwrap().join(FILE_NAME), "outside").unwrap();

        let file = repo.join("configs/x.json");
        assert_eq!(discover(None, Some(&file)), vec![repo.join(FILE_NAME)]);
    }

    #[test]
    fn added_files_can_be_switched_off_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let found = root.join(FILE_NAME);
        let extra = root.join("review-rules.md");
        fs::write(&found, "Version bumps are expected.").unwrap();
        fs::write(&extra, "Ignore the metadata key.").unwrap();

        let mut agents = AgentsMd::default();
        agents.refresh([(Some(&root), None), (None, None)]);
        agents.add(&extra).unwrap();
        let section = agents.prompt_section();
        assert!(section.contains("Version bumps are expected."));
        assert!(section.contains("Ignore the metadata key."));
        // The closer, later file comes after the found one.
        assert!(section.find("Version bumps") < section.find("Ignore the metadata"));

        agents.set_enabled(&found, false).unwrap();
        assert!(!agents.prompt_section().contains("Version bumps"));
        assert!(
            agents.remove(&found).is_err(),
            "found files are switched off"
        );
        agents.remove(&extra).unwrap();
        assert_eq!(agents.prompt_section(), "");
        assert_eq!(agents.files().len(), 1);
    }

    #[test]
    fn large_files_are_cut_off() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        fs::write(&path, "x".repeat(MAX_BYTES + 100)).unwrap();
        let mut agents = AgentsMd::default();
        agents.add(&path).unwrap();
        let section = agents.prompt_section();
        assert!(section.contains("cut off after"));
        assert!(section.len() < MAX_BYTES + 2_000);
    }
}
