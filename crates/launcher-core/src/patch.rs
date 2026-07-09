use crate::config::Config;
use crate::download::{replace_with_temp, DownloadProgress};
use crate::error::{Error, Result};
use crate::manifest::Manifest;
pub use crate::plan::PatchPlan;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct Patcher {
    running: AtomicBool,
    client: reqwest::Client,
}

impl Default for Patcher {
    fn default() -> Self {
        Self::new()
    }
}

impl Patcher {
    pub fn new() -> Self {
        Self {
            running: AtomicBool::new(false),
            client: reqwest::Client::new(),
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub async fn run<P: AsRef<Path>>(
        &self,
        launcher_dir: P,
        exe_path: P,
        config_path: P,
        config: &Config,
        emitter: Arc<Mutex<dyn PatchEmitter + Send + Sync>>,
    ) -> Result<PatchResult> {
        let launcher_dir = launcher_dir.as_ref();
        let exe_path = exe_path.as_ref();
        let config_path = config_path.as_ref();

        if self
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(Error::PatchAlreadyRunning);
        }
        let _guard = scopeguard::guard(&self.running, |r| r.store(false, Ordering::SeqCst));

        emitter.lock().await.emit_started();

        let manifest_bytes = self
            .client
            .get(&config.manifest_url)
            .send()
            .await
            .map_err(|e| Error::ManifestDownloadFailed(e.to_string()))?
            .bytes()
            .await
            .map_err(|e| Error::ManifestDownloadFailed(e.to_string()))?;
        let manifest = Manifest::from_toml(std::str::from_utf8(&manifest_bytes).map_err(|e| {
            Error::ManifestParseFailed(format!("manifest is not valid UTF-8: {}", e))
        })?)?;

        emitter.lock().await.emit_manifest_downloaded();

        let filtered = manifest.entries_excluding_protected(launcher_dir, exe_path, config_path);
        let mut entries: Vec<&crate::manifest::FileEntry> = Vec::new();
        for item in &filtered {
            match item {
                crate::manifest::ProtectedResult::Ok(entry) => entries.push(entry),
                crate::manifest::ProtectedResult::Skipped { entry, error } => {
                    emitter
                        .lock()
                        .await
                        .emit_warning(&PatchWarning::ProtectedFileSkipped {
                            path: entry.path.clone(),
                            message: format!("{}", error),
                        });
                }
            }
        }

        emitter
            .lock()
            .await
            .emit_checking(PatchChecking {
                checked_files: 0,
                total_files: entries.len(),
            });

        let plan = PatchPlan::build(launcher_dir, &entries).await?;
        let protected_skipped = filtered.len() - entries.len();
        let plan_with_skipped = PatchPlan {
            protected_skipped,
            ..plan.clone()
        };

        emitter.lock().await.emit_plan_ready(plan_with_skipped);

        let mut bytes_downloaded: u64 = 0;
        let total_files = plan.items.len();
        for (index, item) in plan.items.iter().enumerate() {
            let manifest_entry = entries
                .iter()
                .find(|e| e.path == item.path)
                .expect("plan item path must exist in entries");
            emitter.lock().await.emit_file_started(PatchFileStarted {
                path: item.path.clone(),
                file_index: index + 1,
                file_total: total_files,
                file_size: item.size,
            });

            let progress_handle = {
                let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<u64>();
                let path = item.path.clone();
                let file_index = index + 1;
                let file_total = total_files;
                let file_total_bytes = item.size;
                let total_bytes = plan.bytes_to_download;
                let emitter = emitter.clone();
                let jh = tokio::spawn(async move {
                    let mut file_downloaded: u64 = 0;
                    let mut total_downloaded: u64 = bytes_downloaded;
                    let mut last_emitted_total: u64 = bytes_downloaded;
                    while let Some(bytes) = rx.recv().await {
                        file_downloaded += bytes;
                        total_downloaded += bytes;
                        if total_downloaded.saturating_sub(last_emitted_total) >= 64 * 1024 {
                            last_emitted_total = total_downloaded;
                            emitter
                                .lock()
                                .await
                                .emit_file_progress(PatchFileProgress {
                                    path: path.clone(),
                                    file_index,
                                    file_total,
                                    file_downloaded_bytes: file_downloaded,
                                    file_total_bytes,
                                    total_downloaded_bytes: total_downloaded,
                                    total_bytes,
                                });
                        }
                    }
                    total_downloaded
                });
                (tx, jh)
            };

            let mut progress_sender = ChannelProgressSender {
                sender: progress_handle.0,
            };
            let temp = crate::download::download_file(
                &self.client,
                &config.data_url,
                &item.path,
                launcher_dir,
                &manifest_entry.md5,
                &mut progress_sender,
            )
            .await?;
            drop(progress_sender);
            let final_total = progress_handle.1.await.unwrap_or(bytes_downloaded);

            replace_with_temp(temp, launcher_dir).await?;
            bytes_downloaded = final_total;

            emitter
                .lock()
                .await
                .emit_file_completed(PatchFileCompleted {
                    path: item.path.clone(),
                    file_index: index + 1,
                    file_total: total_files,
                    status: "completed".to_string(),
                });
        }

        let result = PatchResult {
            status: "completed".to_string(),
            checked_files: plan.checked_files,
            files_to_patch: plan.files_to_download,
            files_patched: plan.files_to_download,
            bytes_downloaded,
        };
        emitter.lock().await.emit_completed(result.clone());
        Ok(result)
    }
}

pub struct ProgressTracker {
    pub emitter: Arc<Mutex<dyn PatchEmitter + Send + Sync>>,
    pub file_index: usize,
    pub file_total: usize,
    pub file_total_bytes: u64,
    pub file_downloaded_bytes: Arc<std::sync::atomic::AtomicU64>,
    pub total_downloaded_bytes: Arc<std::sync::atomic::AtomicU64>,
    pub total_bytes: u64,
    pub last_emit: Arc<std::sync::atomic::AtomicU64>,
    pub path: String,
}

impl DownloadProgress for ProgressTracker {
    fn on_bytes(&mut self, bytes: u64) {
        self.file_downloaded_bytes.fetch_add(bytes, Ordering::Relaxed);
        let total = self.total_downloaded_bytes.fetch_add(bytes, Ordering::Relaxed) + bytes;
        // Throttle emits to every 64 KiB to avoid flooding the UI.
        let last = self.last_emit.load(Ordering::Relaxed);
        if total.saturating_sub(last) > 64 * 1024 {
            self.last_emit.store(total, Ordering::Relaxed);
            let payload = PatchFileProgress {
                path: self.path.clone(),
                file_index: self.file_index,
                file_total: self.file_total,
                file_downloaded_bytes: self.file_downloaded_bytes.load(Ordering::Relaxed),
                file_total_bytes: self.file_total_bytes,
                total_downloaded_bytes: total,
                total_bytes: self.total_bytes,
            };
            let emitter = self.emitter.clone();
            let _ = tokio::runtime::Handle::try_current().map(|rt| {
                rt.spawn(async move {
                    emitter.lock().await.emit_file_progress(payload);
                });
            });
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PatchResult {
    pub status: String,
    pub checked_files: usize,
    pub files_to_patch: usize,
    pub files_patched: usize,
    pub bytes_downloaded: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PatchChecking {
    pub checked_files: usize,
    pub total_files: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PatchFileStarted {
    pub path: String,
    pub file_index: usize,
    pub file_total: usize,
    pub file_size: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PatchFileProgress {
    pub path: String,
    pub file_index: usize,
    pub file_total: usize,
    pub file_downloaded_bytes: u64,
    pub file_total_bytes: u64,
    pub total_downloaded_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PatchFileCompleted {
    pub path: String,
    pub file_index: usize,
    pub file_total: usize,
    pub status: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "code", content = "details")]
pub enum PatchWarning {
    ProtectedFileSkipped { path: String, message: String },
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PatchErrorEvent {
    pub code: String,
    pub path: String,
    pub message: String,
    pub retryable: bool,
}

struct ChannelProgressSender {
    sender: tokio::sync::mpsc::UnboundedSender<u64>,
}

impl DownloadProgress for ChannelProgressSender {
    fn on_bytes(&mut self, bytes: u64) {
        let _ = self.sender.send(bytes);
    }
}

#[allow(clippy::too_long_first_doc_paragraph)]
pub trait PatchEmitter: Send + Sync {
    fn emit_started(&mut self);
    fn emit_manifest_downloaded(&mut self);
    fn emit_checking(&mut self, payload: PatchChecking);
    fn emit_plan_ready(&mut self, plan: PatchPlan);
    fn emit_file_started(&mut self, payload: PatchFileStarted);
    fn emit_file_progress(&mut self, payload: PatchFileProgress);
    fn emit_file_completed(&mut self, payload: PatchFileCompleted);
    fn emit_warning(&mut self, warning: &PatchWarning);
    fn emit_error(&mut self, error: PatchErrorEvent);
    fn emit_completed(&mut self, result: PatchResult);
}
