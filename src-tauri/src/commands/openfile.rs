use std::sync::Mutex;
use tauri::State;

/// Queue of `.ofx` paths opened via Finder ("Open with Finoinc") that the
/// frontend has not consumed yet. Needed for cold start: the file arrives in
/// `RunEvent::Opened` before the frontend mounts its listeners.
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
