#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod commands;
mod emitter;

use app::BossPatcherApp;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_log::Builder::default().build())
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::block_on(async move {
                BossPatcherApp::setup(handle).await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_exit,
            commands::call_alias,
            commands::patch_files,
            commands::get_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running BossPatcher application");
}
