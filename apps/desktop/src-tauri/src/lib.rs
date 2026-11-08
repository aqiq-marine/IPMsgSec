mod commands;
mod state;
mod tray;

use tauri::{Manager, RunEvent, WindowEvent};

use state::DesktopState;

pub fn run() {
    tauri::Builder::default()
        .manage(DesktopState::default())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .invoke_handler(tauri::generate_handler![
            commands::start_session,
            commands::discover,
            commands::get_snapshot,
            commands::send_message,
            commands::stop_session,
        ])
        .setup(|app| {
            tray::install(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main"
                && let WindowEvent::CloseRequested { api, .. } = event
            {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("failed to build IPMsgSec desktop application")
        .run(|app_handle, event| {
            if let RunEvent::Exit = event
                && let Some(state) = app_handle.try_state::<DesktopState>()
            {
                let _ = state.shutdown();
            }
        });
}
