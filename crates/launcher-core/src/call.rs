use crate::config::Config;
use crate::error::{Error, Result};
use crate::path::resolve_relative;
use std::path::{Path, PathBuf};
use std::process::Stdio;

/// Resolve and validate an alias target.
pub fn resolve_alias_target<P: AsRef<Path>>(
    launcher_dir: P,
    alias: &str,
    config: &Config,
) -> Result<PathBuf> {
    let target = config.calls.get(alias).ok_or_else(|| Error::UnknownAlias {
        alias: alias.to_string(),
    })?;
    let resolved = resolve_relative(launcher_dir.as_ref(), target)?;
    Ok(resolved)
}

/// Spawn the configured executable by alias.
pub async fn launch_alias<P: AsRef<Path>>(
    launcher_dir: P,
    alias: &str,
    config: &Config,
) -> Result<LaunchResult> {
    let target = resolve_alias_target(launcher_dir.as_ref(), alias, config)?;
    if !tokio::fs::try_exists(&target).await.unwrap_or(false) {
        return Err(Error::TargetNotFound { path: target });
    }
    let metadata = tokio::fs::metadata(&target)
        .await
        .map_err(|e| Error::LocalFileReadFailed {
            path: target.clone(),
            reason: e.to_string(),
        })?;
    if metadata.is_dir() {
        return Err(Error::TargetNotExecutable { path: target });
    }
    let ext = target.extension().and_then(|e| e.to_str()).unwrap_or("");
    if !is_executable_extension(ext) {
        // We allow arbitrary files to be launched as long as they exist, but the PRD states
        // "Values should normally point to .exe files". Keep permissive but still report
        // TARGET_NOT_EXECUTABLE for obvious non-runnable extensions.
        if !["exe", "bat", "cmd", "com"].contains(&ext.to_ascii_lowercase().as_str()) {
            return Err(Error::TargetNotExecutable { path: target });
        }
    }
    if config.call_options_for(alias).elevated {
        return launch_elevated(launcher_dir.as_ref(), alias, &target);
    }

    let mut cmd = tokio::process::Command::new(&target);
    cmd.current_dir(launcher_dir.as_ref())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // Windows: detach the child so we don't wait. CREATE_NO_WINDOW = 0x08000000;
        // DETACHED_PROCESS = 0x00000008 is also common. Use DETACHED_PROCESS for simplicity.
        .creation_flags(0x08000000);
    let mut child = cmd.spawn().map_err(|e| Error::ProcessStartFailed {
        path: target.clone(),
        reason: e.to_string(),
    })?;
    let pid = child.id();
    tokio::spawn(async move {
        let _ = child.wait().await;
    });
    Ok(LaunchResult {
        alias: alias.to_string(),
        target: target.to_string_lossy().into_owned(),
        started: true,
        pid,
    })
}

#[cfg(windows)]
fn launch_elevated(launcher_dir: &Path, alias: &str, target: &Path) -> Result<LaunchResult> {
    use std::ffi::OsStr;
    use std::iter::once;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null_mut;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    fn wide(value: &OsStr) -> Vec<u16> {
        value.encode_wide().chain(once(0)).collect()
    }

    let operation = wide(OsStr::new("runas"));
    let file = wide(target.as_os_str());
    let directory = wide(launcher_dir.as_os_str());
    let result = unsafe {
        ShellExecuteW(
            null_mut(),
            operation.as_ptr(),
            file.as_ptr(),
            null_mut(),
            directory.as_ptr(),
            SW_SHOWNORMAL,
        )
    };

    if (result as isize) <= 32 {
        return Err(Error::ProcessStartFailed {
            path: target.to_path_buf(),
            reason: format!("ShellExecuteW runas failed with code {}", result as isize),
        });
    }

    Ok(LaunchResult {
        alias: alias.to_string(),
        target: target.to_string_lossy().into_owned(),
        started: true,
        pid: None,
    })
}

#[cfg(not(windows))]
fn launch_elevated(launcher_dir: &Path, alias: &str, target: &Path) -> Result<LaunchResult> {
    let _ = launcher_dir;
    let _ = alias;
    let _ = target;
    Err(Error::ProcessStartFailed {
        path: target.to_path_buf(),
        reason: "elevated aliases are only supported on Windows".to_string(),
    })
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct LaunchResult {
    pub alias: String,
    pub target: String,
    pub started: bool,
    pub pid: Option<u32>,
}

fn is_executable_extension(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "exe" | "bat" | "cmd" | "com"
    )
}
