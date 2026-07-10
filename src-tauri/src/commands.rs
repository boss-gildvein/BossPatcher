use crate::app::app_state;
use crate::emitter::TauriPatchEmitter;
use launcher_core::call::launch_alias;
use launcher_core::patch::{PatchEmitter, PatchErrorEvent, PatchResult};

#[derive(serde::Serialize, Clone, Debug)]
pub struct Status {
    pub config_loaded: bool,
    pub title: Option<String>,
    pub launcher_url: Option<String>,
    pub aliases: Vec<String>,
    pub patch_running: bool,
    pub last_error: Option<String>,
}

#[tauri::command]
pub async fn get_status(app: tauri::AppHandle) -> Result<Status, String> {
    let state = app_state(&app);
    let last_error = state.last_error.lock().unwrap().clone();
    Ok(Status {
        config_loaded: true,
        title: Some(state.config.title.clone()),
        launcher_url: Some(state.config.launcher_url.clone()),
        aliases: state.config.calls.keys().cloned().collect(),
        patch_running: state.patcher.is_running(),
        last_error,
    })
}

#[tauri::command]
pub async fn app_exit(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    let state = app_state(&app);
    if state.patcher.is_running() {
        return Err("PATCH_IN_PROGRESS".to_string());
    }
    app.exit(0);
    Ok(serde_json::json!({"status": "closing"}))
}

#[tauri::command]
pub async fn call_alias(app: tauri::AppHandle, alias: String) -> Result<serde_json::Value, String> {
    let state = app_state(&app);
    if state.patcher.is_running() {
        return Err("PATCH_IN_PROGRESS".to_string());
    }
    let result = launch_alias(&state.launcher_dir, &alias, &state.config)
        .await
        .map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(result).unwrap())
}

#[tauri::command]
pub async fn patch_files(
    app: tauri::AppHandle,
    window: tauri::Window,
) -> Result<PatchResult, String> {
    let state = app_state(&app);
    let emitter = std::sync::Arc::new(tokio::sync::Mutex::new(TauriPatchEmitter::new(window)));
    let result = state
        .patcher
        .run(
            &state.launcher_dir,
            &state.exe_path,
            &state.config_path,
            &state.config,
            emitter.clone(),
        )
        .await;
    match result {
        Ok(r) => Ok(r),
        Err(e) => {
            // Emit structured error to UI.
            let ev = PatchErrorEvent {
                code: error_code(&e),
                path: error_path(&e),
                message: e.to_string(),
                retryable: matches!(
                    e,
                    launcher_core::Error::DownloadFailed { .. }
                        | launcher_core::Error::FileLocked { .. }
                        | launcher_core::Error::ManifestDownloadFailed(_)
                ),
            };
            let _ = emitter.lock().await.emit_error(ev);
            Err(error_code(&e))
        }
    }
}

#[tauri::command]
pub async fn cancel_patch(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    let state = app_state(&app);
    if state.patcher.cancel() {
        return Ok(serde_json::json!({"status": "cancelling"}));
    }

    Err("PATCH_NOT_RUNNING".to_string())
}

fn error_code(e: &launcher_core::Error) -> String {
    use launcher_core::Error::*;
    match e {
        PatchAlreadyRunning => "PATCH_ALREADY_RUNNING",
        PatchInProgress => "PATCH_IN_PROGRESS",
        ManifestDownloadFailed(_) => "MANIFEST_DOWNLOAD_FAILED",
        ManifestParseFailed(_) => "MANIFEST_PARSE_FAILED",
        ManifestUnsupportedVersion(_) => "MANIFEST_UNSUPPORTED_VERSION",
        ManifestUnsupportedHash(_) => "MANIFEST_UNSUPPORTED_HASH",
        ManifestDuplicatePath { .. } => "MANIFEST_DUPLICATE_PATH",
        ManifestInvalidPath { .. } => "MANIFEST_INVALID_PATH",
        ProtectedFileSkipped { .. } => "PROTECTED_FILE_SKIPPED",
        LocalFileReadFailed { .. } => "LOCAL_FILE_READ_FAILED",
        LocalHashFailed { .. } => "LOCAL_HASH_FAILED",
        DownloadFailed { .. } => "DOWNLOAD_FAILED",
        DownloadCancelled => "DOWNLOAD_CANCELLED",
        HashMismatch { .. } => "HASH_MISMATCH",
        FileLocked { .. } => "FILE_LOCKED",
        ReplaceFailed { .. } => "REPLACE_FAILED",
        UnknownAlias { .. } => "UNKNOWN_ALIAS",
        InvalidAliasTarget { .. } => "INVALID_ALIAS_TARGET",
        TargetNotFound { .. } => "TARGET_NOT_FOUND",
        TargetNotExecutable { .. } => "TARGET_NOT_EXECUTABLE",
        ProcessStartFailed { .. } => "PROCESS_START_FAILED",
        ConfigNotFound { .. } => "CONFIG_NOT_FOUND",
        ConfigParseFailed(_) => "CONFIG_PARSE_FAILED",
        ConfigMissingField { .. } => "CONFIG_MISSING_FIELD",
        ConfigInvalidField { .. } => "CONFIG_INVALID_FIELD",
        LauncherUrlInvalid(_) => "LAUNCHER_URL_INVALID",
        LauncherUrlNotHttps(_) => "LAUNCHER_URL_NOT_HTTPS",
        ManifestUrlInvalid(_) => "MANIFEST_URL_INVALID",
        DataUrlInvalid(_) => "DATA_URL_INVALID",
        CallsMissing => "CALLS_MISSING",
        RemoteUiLoadFailed => "REMOTE_UI_LOAD_FAILED",
        Io(_) | Join(_) | Reqwest(_) | Url(_) => "INTERNAL_ERROR",
    }
    .to_string()
}

fn error_path(e: &launcher_core::Error) -> String {
    use launcher_core::Error::*;
    match e {
        ManifestDuplicatePath { path }
        | ManifestInvalidPath { path, .. }
        | DownloadFailed { path, .. } => path.clone(),
        ProtectedFileSkipped { path }
        | LocalFileReadFailed { path, .. }
        | LocalHashFailed { path, .. }
        | HashMismatch { path, .. }
        | FileLocked { path, .. }
        | ReplaceFailed { path, .. }
        | TargetNotFound { path, .. }
        | TargetNotExecutable { path, .. }
        | ProcessStartFailed { path, .. } => path.to_string_lossy().into_owned(),
        ConfigNotFound { path } => path.to_string_lossy().into_owned(),
        _ => String::new(),
    }
}
