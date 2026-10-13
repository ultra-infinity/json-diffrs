//! What changed in each release, from CHANGELOG.md (built into the app).
//! After an update the app shows the releases since the version it last
//! ran; Help > What's New shows them all.

use semver::Version;
use serde::Serialize;

const CHANGELOG: &str = include_str!("../../../CHANGELOG.md");

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Release {
    pub version: String,
    pub date: Option<String>,
    /// The entry's Markdown: `-` bullets with `**bold**` and `` `code` ``.
    pub notes: String,
}

/// Every released version up to `current`, newest first. Unreleased notes
/// and anything newer than `current` are left out.
pub fn releases(current: &str) -> Vec<Release> {
    let current = Version::parse(current).ok();
    parse(CHANGELOG)
        .into_iter()
        .filter(
            |release| match (&current, Version::parse(&release.version)) {
                (Some(current), Ok(version)) => version <= *current,
                _ => false,
            },
        )
        .collect()
}

/// The releases to show at startup. `previous` is the version that last
/// ran, and `upgraded` whether some version ran before (settings were
/// saved): those upgrading from a version too old to record itself see
/// just the current release, and a first run shows nothing.
pub fn news(previous: Option<&str>, upgraded: bool, current: &str) -> Vec<Release> {
    let all = releases(current);
    match previous.map(Version::parse) {
        Some(Ok(previous)) => all
            .into_iter()
            .filter(|release| {
                Version::parse(&release.version).is_ok_and(|version| version > previous)
            })
            .collect(),
        // An unreadable version: the current release is still news.
        Some(Err(_)) => all.into_iter().take(1).collect(),
        None if upgraded => all.into_iter().take(1).collect(),
        None => Vec::new(),
    }
}

/// The `## <version> - <date>` sections of a changelog, in file order.
/// Other `##` headings (Unreleased) end the previous section and are
/// skipped.
fn parse(text: &str) -> Vec<Release> {
    fn finish(release: Option<Release>, releases: &mut Vec<Release>) {
        if let Some(mut release) = release {
            release.notes = release.notes.trim().to_string();
            releases.push(release);
        }
    }
    let mut releases = Vec::new();
    let mut current: Option<Release> = None;
    for line in text.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            finish(current.take(), &mut releases);
            let (version, date) = match heading.split_once(" - ") {
                Some((version, date)) => (version.trim(), Some(date.trim().to_string())),
                None => (heading.trim(), None),
            };
            let version = version.trim_start_matches('v');
            if Version::parse(version).is_ok() {
                current = Some(Release {
                    version: version.to_string(),
                    date,
                    notes: String::new(),
                });
            }
        } else if let Some(release) = &mut current {
            release.notes.push_str(line);
            release.notes.push('\n');
        }
    }
    finish(current, &mut releases);
    releases
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# Changelog\n\nIntro.\n\n## Unreleased\n\n- not yet\n\n\
        ## 0.5.0 - 2026-11-01\n\n- **New:** one\n- two\n\n## 0.4.1 - 2026-10-13\n\n- fix\n";

    #[test]
    fn sections_are_versions_with_their_notes() {
        let releases = parse(SAMPLE);
        assert_eq!(releases.len(), 2, "Unreleased is skipped");
        assert_eq!(releases[0].version, "0.5.0");
        assert_eq!(releases[0].date.as_deref(), Some("2026-11-01"));
        assert_eq!(releases[0].notes, "- **New:** one\n- two");
        assert_eq!(releases[1].notes, "- fix");
    }

    #[test]
    fn the_built_in_changelog_covers_this_version() {
        let current = env!("CARGO_PKG_VERSION");
        let releases = releases(current);
        assert_eq!(
            releases.first().map(|release| release.version.as_str()),
            Some(current),
            "CHANGELOG.md needs a `## {current} - <date>` entry"
        );
        assert!(releases.iter().all(|release| !release.notes.is_empty()));
    }

    #[test]
    fn news_is_what_came_after_the_last_run() {
        let current = env!("CARGO_PKG_VERSION");
        let all = releases(current);
        assert!(all.len() >= 3);
        let versions = |releases: Vec<Release>| -> Vec<String> {
            releases
                .into_iter()
                .map(|release| release.version)
                .collect()
        };
        assert_eq!(
            versions(news(Some(&all[2].version), true, current)),
            versions(all[..2].to_vec())
        );
        assert!(
            news(Some(current), true, current).is_empty(),
            "already seen"
        );
        assert!(news(None, false, current).is_empty(), "a first run");
        assert_eq!(
            versions(news(None, true, current)),
            vec![current.to_string()],
            "upgraded from a version that didn't record itself"
        );
    }
}
