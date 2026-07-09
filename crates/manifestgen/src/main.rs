use clap::Parser;
use launcher_core::hash::md5_file;
use launcher_core::manifest::{FileEntry, Manifest, MANIFEST_HASH_ALGORITHM_MD5, MANIFEST_VERSION};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

#[derive(Parser, Debug)]
#[command(name = "manifestgen", about = "Generate manifest.toml from a folder")]
struct Args {
    /// Folder to scan.
    folder: PathBuf,

    /// Output manifest path.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Hash algorithm (MVP supports md5).
    #[arg(short, long, default_value = "md5")]
    #[allow(dead_code)]
    hash: String,

    /// Print progress information.
    #[arg(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    let folder = std::path::absolute(&args.folder)?;
    if !folder.is_dir() {
        anyhow::bail!("input path is not a directory: {}", folder.display());
    }

    let output = args
        .output
        .as_ref()
        .map(|p| std::path::absolute(p).unwrap_or_else(|_| p.clone()))
        .unwrap_or_else(|| folder.join("manifest.toml"));

    if args.verbose {
        info!("Scanning folder: {}", folder.display());
        info!("Output manifest: {}", output.display());
    }

    let mut files = Vec::new();
    let mut failures: Vec<(PathBuf, std::io::Error)> = Vec::new();

    let mut entries = tokio::fs::read_dir(&folder).await?;
    while let Some(entry) = entries.next_entry().await? {
        collect_files(
            &folder,
            &output,
            &entry.path(),
            &mut files,
            &mut failures,
            args.verbose,
        )
        .await?;
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));

    if !failures.is_empty() {
        for (path, err) in &failures {
            warn!("Could not read {}: {}", path.display(), err);
        }
        if failures.len() == files.len() && files.is_empty() {
            anyhow::bail!("no files could be read");
        }
    }

    let manifest = Manifest {
        manifest_version: MANIFEST_VERSION,
        hash_algorithm: MANIFEST_HASH_ALGORITHM_MD5.into(),
        generated_at: Some(chrono::Utc::now().to_rfc3339()),
        files,
    };
    let toml_string = toml::to_string_pretty(&manifest)?;
    tokio::fs::write(&output, toml_string).await?;

    if args.verbose {
        info!(
            "Wrote {} file entries to {}",
            manifest.files.len(),
            output.display()
        );
        if !failures.is_empty() {
            warn!("{} files could not be read", failures.len());
        }
    }
    Ok(())
}

fn collect_files<'a>(
    root: &'a Path,
    output_manifest: &'a Path,
    current: &'a Path,
    files: &'a mut Vec<FileEntry>,
    failures: &'a mut Vec<(PathBuf, std::io::Error)>,
    verbose: bool,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'a>> {
    Box::pin(async move {
        let abs_current = std::path::absolute(current)?;

        if abs_current == *output_manifest {
            return Ok(());
        }

        let metadata = match tokio::fs::metadata(&abs_current).await {
            Ok(m) => m,
            Err(e) => {
                failures.push((abs_current, e));
                return Ok(());
            }
        };

        if metadata.is_dir() {
            let mut entries = tokio::fs::read_dir(&abs_current).await?;
            while let Some(entry) = entries.next_entry().await? {
                collect_files(
                    root,
                    output_manifest,
                    &entry.path(),
                    files,
                    failures,
                    verbose,
                )
                .await?;
            }
        } else if metadata.is_file() {
            let rel = match abs_current.strip_prefix(root) {
                Ok(r) => r,
                Err(_) => {
                    failures.push((
                        abs_current,
                        std::io::Error::new(std::io::ErrorKind::Other, "outside root"),
                    ));
                    return Ok(());
                }
            };
            let rel_str = rel.to_string_lossy().replace("\\", "/");
            if rel_str.is_empty() {
                return Ok(());
            }
            let md5 = match md5_file(&abs_current).await {
                Ok(h) => h,
                Err(e) => {
                    failures.push((
                        abs_current,
                        std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
                    ));
                    return Ok(());
                }
            };
            if verbose {
                info!("{} ({}, md5: {})", rel_str, metadata.len(), md5);
            }
            files.push(FileEntry {
                path: rel_str,
                size: metadata.len(),
                md5,
            });
        } else {
            // Skip symlinks and special files on Windows.
        }
        Ok(())
    })
}
