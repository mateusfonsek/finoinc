use std::path::Path;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};

/// Queue of `.ofx` paths opened from the file manager ("Open with Finoinc")
/// that the frontend has not consumed yet. Needed for cold start: the file
/// arrives (macOS `RunEvent::Opened`, argv elsewhere) before the frontend
/// mounts its listeners.
#[derive(Default)]
pub struct PendingOpen(pub Mutex<Vec<String>>);

/// Drains the pending paths. The frontend calls this on mount and on every
/// `open-ofx` event.
#[tauri::command]
#[specta::specta]
pub fn take_pending_ofx(pending: State<'_, PendingOpen>) -> Vec<String> {
    let mut queue = pending.0.lock().expect("pending mutex poisoned");
    std::mem::take(&mut *queue)
}

/// The `.ofx` files in a command line. Windows and Linux hand opened files to
/// the app as arguments; the first one is the executable itself, and the rest
/// may include flags. The extension check ignores case: Windows users get
/// `EXTRATO.OFX` from some banks.
// macOS delivers files through `RunEvent::Opened`, never argv.
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn ofx_paths<I: IntoIterator<Item = String>>(args: I) -> Vec<String> {
    args.into_iter()
        .skip(1)
        .filter(|a| {
            Path::new(a)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("ofx"))
        })
        .collect()
}

/// Queues the paths and tells the frontend to drain them. Nothing is emitted
/// for an empty list, so relaunching the app without a file only focuses it.
pub fn queue_and_announce(app: &AppHandle, paths: Vec<String>) {
    if paths.is_empty() {
        return;
    }
    if let Some(pending) = app.try_state::<PendingOpen>() {
        pending
            .0
            .lock()
            .expect("pending mutex poisoned")
            .extend(paths);
    }
    let _ = app.emit("open-ofx", ());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn ofx_paths_keeps_only_ofx_files() {
        let got = ofx_paths(args(&[
            "C:\\Finoinc\\finoinc.exe",
            "--flag",
            "C:\\a b\\Extrato.OFX",
            "/x/y.ofx",
            "/x/notes.txt",
        ]));
        assert_eq!(got, args(&["C:\\a b\\Extrato.OFX", "/x/y.ofx"]));
    }

    #[test]
    fn no_ofx_in_args_queues_nothing() {
        assert!(ofx_paths(args(&["finoinc"])).is_empty());
    }
}
