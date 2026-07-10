#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod commands;
mod emitter;

use app::BossPatcherApp;
use launcher_core::patch::{
    PatchChecking, PatchEmitter, PatchErrorEvent, PatchFileCompleted, PatchFileProgress,
    PatchFileStarted, PatchPlan, PatchResult, PatchWarning,
};
use std::env;
use std::process::exit;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "--headless-patch") {
        run_headless_patch();
        return;
    }

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
            commands::cancel_patch,
            commands::patch_files,
            commands::get_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running BossPatcher application");
}

fn run_headless_patch() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    rt.block_on(async {
        let exe_path = app::resolve_exe_path().expect("resolve exe");
        let launcher_dir = exe_path.parent().expect("exe parent").to_path_buf();
        let config_path =
            launcher_core::config::derive_config_path(&exe_path).expect("derive config");
        let config = launcher_core::config::load_config(&config_path)
            .await
            .expect("load config");
        let patcher = launcher_core::patch::Patcher::new();
        let emitter = Arc::new(tokio::sync::Mutex::new(ConsoleEmitter));
        match patcher
            .run(&launcher_dir, &exe_path, &config_path, &config, emitter)
            .await
        {
            Ok(result) => {
                println!(
                    "PATCH_OK: {}",
                    serde_json::to_string_pretty(&result).unwrap()
                );
                exit(0);
            }
            Err(e) => {
                eprintln!("PATCH_ERR: {}", e);
                exit(1);
            }
        }
    });
}

use std::sync::Arc;

struct ConsoleEmitter;

impl PatchEmitter for ConsoleEmitter {
    fn emit_started(&mut self) {
        println!("[patch:started]");
    }
    fn emit_manifest_downloaded(&mut self) {
        println!("[patch:manifest-downloaded]");
    }
    fn emit_checking(&mut self, payload: PatchChecking) {
        println!("[patch:checking] {:?}", payload);
    }
    fn emit_plan_ready(&mut self, plan: PatchPlan) {
        println!("[patch:plan-ready] {:?}", plan);
    }
    fn emit_file_started(&mut self, payload: PatchFileStarted) {
        println!("[patch:file-started] {:?}", payload);
    }
    fn emit_file_progress(&mut self, payload: PatchFileProgress) {
        println!("[patch:file-progress] {:?}", payload);
    }
    fn emit_file_completed(&mut self, payload: PatchFileCompleted) {
        println!("[patch:file-completed] {:?}", payload);
    }
    fn emit_warning(&mut self, warning: &PatchWarning) {
        println!("[patch:warning] {:?}", warning);
    }
    fn emit_error(&mut self, error: PatchErrorEvent) {
        println!("[patch:error] {:?}", error);
    }
    fn emit_completed(&mut self, result: PatchResult) {
        println!("[patch:completed] {:?}", result);
    }
}
