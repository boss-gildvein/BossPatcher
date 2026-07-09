use crate::error::{Error, Result};
use crate::hash::md5_file;
use crate::path::{resolve_relative, url_path_segment_for_data_url};
use std::path::{Path, PathBuf};
use tokio::fs::OpenOptions;
use tokio::io::{AsyncWriteExt, BufWriter};

/// Download a single manifest file to a temporary location, verify it, and
/// return the verified temporary path.
pub async fn download_file<P: AsRef<Path>>(
    client: &reqwest::Client,
    data_url: &str,
    manifest_path: &str,
    launcher_dir: P,
    expected_md5: &str,
    progress: &mut dyn DownloadProgress,
) -> Result<TempVerifiedFile> {
    let url = url_path_segment_for_data_url(data_url, manifest_path)?;
    let local_temp = temp_path_for(manifest_path, launcher_dir.as_ref())?;
    if let Some(parent) = local_temp.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| Error::DownloadFailed {
            path: manifest_path.to_string(),
            reason: format!("create dir failed: {}", e),
        })?;
    }
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&local_temp)
        .await
        .map_err(|e| Error::DownloadFailed {
            path: manifest_path.to_string(),
            reason: format!("open temp failed: {}", e),
        })?;
    let mut writer = BufWriter::new(file);

    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|e| Error::DownloadFailed {
            path: manifest_path.to_string(),
            reason: format!("request failed: {}", e),
        })?;
    if !response.status().is_success() {
        return Err(Error::DownloadFailed {
            path: manifest_path.to_string(),
            reason: format!("server returned {}", response.status()),
        });
    }

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| Error::DownloadFailed {
            path: manifest_path.to_string(),
            reason: format!("chunk error: {}", e),
        })?
    {
        writer
            .write_all(&chunk)
            .await
            .map_err(|e| Error::DownloadFailed {
                path: manifest_path.to_string(),
                reason: format!("write failed: {}", e),
            })?;
        progress.on_bytes(chunk.len() as u64);
    }
    writer.flush().await.map_err(|e| Error::DownloadFailed {
        path: manifest_path.to_string(),
        reason: format!("flush failed: {}", e),
    })?;
    drop(writer);

    // Verify after download.
    let actual_hash = md5_file(&local_temp).await.map_err(|e| Error::DownloadFailed {
        path: manifest_path.to_string(),
        reason: format!("hash failed: {}", e),
    })?;
    if actual_hash.to_ascii_lowercase() != expected_md5.to_ascii_lowercase() {
        let _ = tokio::fs::remove_file(&local_temp).await;
        return Err(Error::HashMismatch {
            path: local_temp,
        });
    }

    Ok(TempVerifiedFile {
        temp_path: local_temp,
        manifest_path: manifest_path.to_string(),
    })
}

/// Safely replace the target file with the downloaded temp file.
pub async fn replace_with_temp(temp: TempVerifiedFile, launcher_dir: &Path) -> Result<()> {
    let target = resolve_relative(launcher_dir, &temp.manifest_path)?;
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| Error::ReplaceFailed {
            path: target.clone(),
            reason: format!("create dir failed: {}", e),
        })?;
    }
    // If the target file is locked, the rename will fail and we do not corrupt it.
    tokio::fs::rename(&temp.temp_path, &target).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            Error::FileLocked {
                path: target.clone(),
                reason: e.to_string(),
            }
        } else {
            Error::ReplaceFailed {
                path: target.clone(),
                reason: e.to_string(),
            }
        }
    })?;
    Ok(())
}

fn temp_path_for(_manifest_path: &str, launcher_dir: &Path) -> Result<PathBuf> {
    let file_name = format!("{}.bosspatcher.tmp", uuid::Uuid::new_v4());
    Ok(resolve_relative(launcher_dir, ".bosspatcher")?.join(file_name))
}

pub struct TempVerifiedFile {
    pub temp_path: PathBuf,
    pub manifest_path: String,
}

pub trait DownloadProgress: Send + Sync {
    fn on_bytes(&mut self, bytes: u64);
}

impl DownloadProgress for () {
    fn on_bytes(&mut self, _bytes: u64) {}
}

impl<'a> DownloadProgress for Box<dyn DownloadProgress + Send + Sync + '_> {
    fn on_bytes(&mut self, bytes: u64) {
        (**self).on_bytes(bytes);
    }
}
