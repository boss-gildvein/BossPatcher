use launcher_core::config::{derive_config_path, load_config, Config, Origin};
use launcher_core::patch::Patcher;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};
use tauri::{AppHandle, Emitter, Manager, Url, WebviewUrl, WebviewWindowBuilder};

pub struct BossPatcherApp {
    pub config: Config,
    pub launcher_dir: PathBuf,
    pub exe_path: PathBuf,
    pub config_path: PathBuf,
    pub patcher: Arc<Patcher>,
    pub last_error: StdMutex<Option<String>>,
}

impl BossPatcherApp {
    pub async fn setup(handle: AppHandle) {
        let exe_path = match resolve_exe_path() {
            Ok(p) => p,
            Err(e) => {
                show_fatal_error(&handle, &format!("Failed to resolve executable path: {}", e));
                return;
            }
        };
        let launcher_dir = match exe_path.parent() {
            Some(p) => p.to_path_buf(),
            None => {
                show_fatal_error(&handle, "Executable has no parent directory");
                return;
            }
        };
        let config_path = match derive_config_path(&exe_path) {
            Ok(p) => p,
            Err(e) => {
                show_fatal_error(&handle, &format!("Failed to derive config path: {}", e));
                return;
            }
        };

        let config = match load_config(&config_path).await {
            Ok(c) => c,
            Err(e) => {
                show_fatal_error(
                    &handle,
                    &format!(
                        "Failed to load config at {}: {}. Please create the config file and restart.",
                        config_path.display(),
                        e
                    ),
                );
                return;
            }
        };

        let state = BossPatcherApp {
            config: config.clone(),
            launcher_dir: launcher_dir.clone(),
            exe_path: exe_path.clone(),
            config_path: config_path.clone(),
            patcher: Arc::new(Patcher::new()),
            last_error: StdMutex::new(None),
        };
        handle.manage(state);

        let origin = match Origin::from_url_str(&config.launcher_url) {
            Ok(o) => o,
            Err(e) => {
                show_fatal_error(&handle, &format!("Invalid launcher_url: {}", e));
                return;
            }
        };

        let window = match WebviewWindowBuilder::new(
            &handle,
            "main",
            WebviewUrl::External(Url::parse(&config.launcher_url).unwrap_or_else(|_| {
                Url::parse("https://localhost").expect("localhost url is valid")
            })),
        )
        .title(&config.title)
        .inner_size(1280.0, 800.0)
        .center()
        .visible(true)
        .build()
        {
            Ok(w) => w,
            Err(e) => {
                show_fatal_error(&handle, &format!("Failed to create window: {}", e));
                return;
            }
        };

        let _ = window;
        let _ = origin;
        // Origin restriction is enforced via Tauri v2 capabilities in capabilities/.
    }
}

fn show_fatal_error(handle: &AppHandle, message: &str) {
    let _ = handle.emit("app:fatal-error", message);
    tracing::error!("{}", message);
    // In a real build you would show a local error webview. For now emit an event
    // and leave the app running with a blank window.
}

fn resolve_exe_path() -> std::io::Result<PathBuf> {
    #[cfg(windows)]
    {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;
        use windows_sys::Win32::System::LibraryLoader::GetModuleFileNameW;
        let mut buf = vec![0u16; 4096];
        let len = unsafe {
            GetModuleFileNameW(std::ptr::null_mut(), buf.as_mut_ptr(), buf.len() as u32)
        };
        if len == 0 {
            return Err(std::io::Error::last_os_error());
        }
        buf.truncate(len as usize);
        Ok(PathBuf::from(OsString::from_wide(&buf)))
    }
    #[cfg(not(windows))]
    {
        std::env::current_exe()
    }
}

/// Get a reference to the managed app state.
pub fn app_state<R: tauri::Runtime>(handle: &tauri::AppHandle<R>) -> tauri::State<'_, BossPatcherApp> {
    handle.state::<BossPatcherApp>()
}
