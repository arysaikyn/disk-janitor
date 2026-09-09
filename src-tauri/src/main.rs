//! Tauri shell over the disk_janitor core (SPEC Part C §2.3).
//! Bootstrap-then-stream: snapshot commands seed state; typed `Channel<T>`
//! delivers ordered deltas. The webview never receives raw fs verbs — ~10 named
//! commands, capability file deny-by-default.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            use tauri::Manager;
            // Second launch focuses the existing window (SPEC Part C §2.1).
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_focus();
            }
        }))
        .manage(commands::ScanState::default())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_volumes,
            commands::start_scan,
            commands::cancel_scan,
            commands::get_last_scan,
            commands::build_plan,
            commands::apply_plan,
            commands::purge_plan,
            commands::restore_op,
            commands::get_ledger_tail,
            commands::recover_on_launch,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
