//! What the updater needs from Rust: how this install can be updated, and a
//! quiet moment to install in.

use serde::Serialize;
use specta::Type;
use tauri::AppHandle;

/// Whether the updater can replace this install in place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum UpdateInstallKind {
    Installable,
    /// A `.deb` install: the updater only knows how to replace an AppImage on
    /// Linux, so the app links to the release instead.
    DownloadOnly,
}

/// The AppImage runtime sets `APPIMAGE` to the image's path; a `.deb` install
/// never has it.
pub fn install_kind(os: &str, appimage: Option<&str>) -> UpdateInstallKind {
    match (os, appimage) {
        ("linux", Some(path)) if !path.is_empty() => UpdateInstallKind::Installable,
        ("linux", _) => UpdateInstallKind::DownloadOnly,
        _ => UpdateInstallKind::Installable,
    }
}

#[tauri::command]
#[specta::specta]
pub fn update_install_kind() -> UpdateInstallKind {
    install_kind(std::env::consts::OS, std::env::var("APPIMAGE").ok().as_deref())
}

/// Stops the MCP server before the installer replaces the app, so no agent
/// write is cut mid-way. Its setting is untouched: it comes back after the
/// relaunch exactly as the user left it.
#[tauri::command]
#[specta::specta]
pub fn prepare_for_update(app: AppHandle) {
    crate::mcp::stop(&app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_kind_per_platform() {
        assert_eq!(install_kind("macos", None), UpdateInstallKind::Installable);
        assert_eq!(install_kind("windows", None), UpdateInstallKind::Installable);
        assert_eq!(
            install_kind("linux", Some("/home/u/Finoinc.AppImage")),
            UpdateInstallKind::Installable
        );
        assert_eq!(install_kind("linux", None), UpdateInstallKind::DownloadOnly);
        assert_eq!(install_kind("linux", Some("")), UpdateInstallKind::DownloadOnly);
    }
}
