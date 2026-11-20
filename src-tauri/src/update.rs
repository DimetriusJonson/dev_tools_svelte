//! Restarting the app once an update has been installed.
//!
//! The updater plugin has no restart command: on Windows the installer is launched outside the
//! app and relaunches it, while on macOS and Linux the new bundle is already on disk and the
//! running process still is the old one. `request_restart` is used instead of `restart` so the
//! `RunEvent::ExitRequested`/`RunEvent::Exit` events are still delivered — the share server and
//! the SQLite connection are cleaned up by them before the new process starts.

#[tauri::command(rename_all = "snake_case")]
pub async fn restart_app(app: tauri::AppHandle) {
    app.request_restart();
}
