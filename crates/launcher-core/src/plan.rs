use crate::error::Result;
use crate::hash::md5_file;
use crate::manifest::FileEntry;
use std::path::Path;

#[derive(Debug, Clone, serde::Serialize)]
pub struct PatchPlan {
    pub checked_files: usize,
    pub valid_files: usize,
    pub missing_files: usize,
    pub changed_files: usize,
    pub protected_skipped: usize,
    pub files_to_download: usize,
    pub bytes_to_download: u64,
    pub items: Vec<PlanItem>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PlanItem {
    pub path: String,
    pub reason: ItemReason,
    pub size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ItemReason {
    Missing,
    SizeMismatch,
    HashMismatch,
}

impl PatchPlan {
    pub fn empty() -> Self {
        Self {
            checked_files: 0,
            valid_files: 0,
            missing_files: 0,
            changed_files: 0,
            protected_skipped: 0,
            files_to_download: 0,
            bytes_to_download: 0,
            items: Vec::new(),
        }
    }

    /// Build a patch plan from manifest entries already filtered for protection/validation.
    pub async fn build<P: AsRef<Path>>(launcher_dir: P, entries: &[&FileEntry]) -> Result<Self> {
        let mut plan = Self::empty();
        for entry in entries {
            plan.checked_files += 1;
            let local_path = crate::path::resolve_relative(launcher_dir.as_ref(), &entry.path)?;
            let reason = if !tokio::fs::try_exists(&local_path).await.unwrap_or(false) {
                plan.missing_files += 1;
                Some(ItemReason::Missing)
            } else {
                let metadata = tokio::fs::metadata(&local_path).await?;
                if metadata.len() != entry.size {
                    plan.changed_files += 1;
                    Some(ItemReason::SizeMismatch)
                } else {
                    let hash = md5_file(&local_path).await?;
                    if hash.to_ascii_lowercase() != entry.md5.to_ascii_lowercase() {
                        plan.changed_files += 1;
                        Some(ItemReason::HashMismatch)
                    } else {
                        plan.valid_files += 1;
                        None
                    }
                }
            };
            if let Some(reason) = reason {
                plan.items.push(PlanItem {
                    path: entry.path.clone(),
                    reason,
                    size: entry.size,
                });
                plan.files_to_download += 1;
                plan.bytes_to_download += entry.size;
            }
        }
        Ok(plan)
    }
}
