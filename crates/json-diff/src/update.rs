//! New releases: checking for one, and installing it when the user clicks
//! "Update". Built on tauri-plugin-updater, which reads `latest.json` from
//! the newest GitHub release (`plugins.updater` in tauri.conf.json) and only
//! installs files signed with the release key.
//!
//! Installing in place only works on Windows, where the update is the MSI.
//! On Linux the app is a Nix package (or built from source), which cannot
//! replace itself, so the user is told about the release instead.

use serde::Serialize;
use std::sync::{Mutex, PoisonError};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_updater::{Update, UpdaterExt};

/// Release pages; the only URLs [`open_release`] opens.
const RELEASES_URL: &str = "https://github.com/ultra-infinity/json-diffrs/releases";

/// The update found by the last check, kept until the user installs it.
#[derive(Default)]
pub struct PendingUpdate(Mutex<Option<Update>>);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    pub notes: Option<String>,
    /// This build can install it itself (Windows).
    pub can_install: bool,
    pub release_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    downloaded: u64,
    total: Option<u64>,
}

/// The newest release, or `None` when this build is up to date.
#[tauri::command]
pub async fn check_update(
    app: AppHandle,
    pending: State<'_, PendingUpdate>,
) -> Result<Option<UpdateInfo>, String> {
    if !signing_key_configured(&app) {
        return Err(
            "updates are not set up in this build: tauri.conf.json has no updater public key"
                .into(),
        );
    }
    let builder = app.updater_builder();
    // The manifest only lists the Windows installer. Its version is the
    // release's version on every platform, so other platforms read it too
    // and just never install it.
    let builder = if cfg!(windows) {
        builder
    } else {
        builder.target("windows-x86_64")
    };
    let update = builder
        .build()
        .map_err(|error| error.to_string())?
        .check()
        .await
        .map_err(|error| format!("could not check for updates: {error}"))?;
    let info = update.as_ref().map(|update| UpdateInfo {
        version: update.version.clone(),
        current_version: update.current_version.clone(),
        notes: update.body.clone(),
        can_install: cfg!(windows),
        release_url: release_url(&update.version),
    });
    *pending.0.lock().unwrap_or_else(PoisonError::into_inner) = update;
    Ok(info)
}

/// Downloads the update found by [`check_update`], checks its signature and
/// installs it, reporting progress as `update-progress` events. On Windows
/// the installer takes over and the app closes.
#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    pending: State<'_, PendingUpdate>,
) -> Result<(), String> {
    if !cfg!(windows) {
        return Err("updates install themselves on Windows only".into());
    }
    let update = pending
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
        .ok_or("there is no update to install; check for updates first")?;
    let mut downloaded = 0u64;
    update
        .download_and_install(
            |chunk, total| {
                downloaded += chunk as u64;
                let _ = app.emit("update-progress", Progress { downloaded, total });
            },
            || {},
        )
        .await
        .map_err(|error| format!("the update failed: {error}"))?;
    app.restart()
}

/// Opens a release page in the browser.
#[tauri::command]
pub fn open_release(app: AppHandle, url: String) -> Result<(), String> {
    if !url.starts_with(RELEASES_URL) {
        return Err(format!("not a json-diff release page: {url}"));
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

fn release_url(version: &str) -> String {
    format!("{RELEASES_URL}/tag/v{}", version.trim_start_matches('v'))
}

/// Updates need the public half of the release key; until it is pasted into
/// tauri.conf.json, checking is off.
fn signing_key_configured(app: &AppHandle) -> bool {
    app.config()
        .plugins
        .0
        .get("updater")
        .and_then(|updater| updater.get("pubkey"))
        .and_then(|key| key.as_str())
        .is_some_and(|key| !key.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_pages_are_tagged_with_a_v() {
        assert_eq!(release_url("0.3.0"), format!("{RELEASES_URL}/tag/v0.3.0"));
        assert_eq!(release_url("v0.3.0"), format!("{RELEASES_URL}/tag/v0.3.0"));
    }
}
