use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

pub const MANIFEST_VERSION: i64 = 1;
pub const MANIFEST_HASH_ALGORITHM_MD5: &str = "md5";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    pub manifest_version: i64,
    pub hash_algorithm: String,
    #[serde(default)]
    pub generated_at: Option<String>,
    pub files: Vec<FileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileEntry {
    pub path: String,
    pub size: u64,
    pub md5: String,
}

impl Manifest {
    /// Parse a manifest from a TOML string and validate it.
    pub fn from_toml(s: &str) -> Result<Self> {
        let manifest: Manifest =
            toml::from_str(s).map_err(|e| Error::ManifestParseFailed(e.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<()> {
        if self.manifest_version != MANIFEST_VERSION {
            return Err(Error::ManifestUnsupportedVersion(self.manifest_version));
        }
        let algo = self.hash_algorithm.to_ascii_lowercase();
        if algo != MANIFEST_HASH_ALGORITHM_MD5 {
            return Err(Error::ManifestUnsupportedHash(self.hash_algorithm.clone()));
        }
        let mut seen: HashSet<String> = HashSet::with_capacity(self.files.len());
        for entry in &self.files {
            validate_entry_path(&entry.path)?;
            validate_entry_hash(&entry.md5, &entry.path)?;
            let lower = entry.path.to_ascii_lowercase();
            if !seen.insert(lower.clone()) {
                return Err(Error::ManifestDuplicatePath {
                    path: entry.path.clone(),
                });
            }
        }
        Ok(())
    }

    /// Return file entries in deterministic order, skipping protected files.
    pub fn entries_excluding_protected<P: AsRef<Path>>(
        &self,
        launcher_dir: P,
        exe_path: P,
        config_path: P,
    ) -> Vec<ProtectedResult<'_>> {
        self.files
            .iter()
            .filter_map(|entry| {
                match check_protected(
                    entry,
                    launcher_dir.as_ref(),
                    exe_path.as_ref(),
                    config_path.as_ref(),
                ) {
                    Ok(()) => Some(ProtectedResult::Ok(entry)),
                    Err(e) => Some(ProtectedResult::Skipped { entry, error: e }),
                }
            })
            .collect()
    }
}

pub enum ProtectedResult<'a> {
    Ok(&'a FileEntry),
    Skipped { entry: &'a FileEntry, error: Error },
}

pub fn validate_entry_path(path: &str) -> Result<()> {
    if path.is_empty() {
        return Err(Error::ManifestInvalidPath {
            path: path.to_string(),
            reason: "empty path".to_string(),
        });
    }
    if path.starts_with('/')
        || path.starts_with("\\")
        || path.contains(":/")
        || path.contains(":\\")
    {
        return Err(Error::ManifestInvalidPath {
            path: path.to_string(),
            reason: "absolute path".to_string(),
        });
    }
    for segment in path.split('/') {
        if segment == ".." {
            return Err(Error::ManifestInvalidPath {
                path: path.to_string(),
                reason: "path traversal".to_string(),
            });
        }
    }
    Ok(())
}

fn validate_entry_hash(hash: &str, path: &str) -> Result<()> {
    if hash.len() != 32 {
        return Err(Error::ManifestInvalidPath {
            path: path.to_string(),
            reason: "md5 hash must be 32 hex characters".to_string(),
        });
    }
    if !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(Error::ManifestInvalidPath {
            path: path.to_string(),
            reason: "md5 hash contains non-hex characters".to_string(),
        });
    }
    Ok(())
}

/// Check whether a manifest entry points to a protected file (launcher exe or config).
/// Returns Ok(()) if safe, Err(ProtectedFileSkipped) if protected.
pub fn check_protected(
    entry: &FileEntry,
    launcher_dir: &Path,
    exe_path: &Path,
    config_path: &Path,
) -> Result<()> {
    let local_path = crate::path::resolve_relative(launcher_dir, &entry.path)?;
    if is_same_file(&local_path, exe_path) || is_same_file(&local_path, config_path) {
        return Err(Error::ProtectedFileSkipped { path: local_path });
    }
    Ok(())
}

fn is_same_file(left: &Path, right: &Path) -> bool {
    left.to_string_lossy().to_lowercase() == right.to_string_lossy().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_manifest() {
        let toml = r#"
manifest_version = 1
hash_algorithm = "md5"
generated_at = "2026-07-09T00:00:00Z"

[[files]]
path = "data/client.grf"
size = 100
md5 = "9e107d9d372bb6826bd81d3542a419d6"
"#;
        let m = Manifest::from_toml(toml).unwrap();
        assert_eq!(m.files.len(), 1);
    }

    #[test]
    fn reject_duplicate_case_insensitive() {
        let toml = r#"
manifest_version = 1
hash_algorithm = "md5"

[[files]]
path = "Data/File.txt"
size = 1
md5 = "00000000000000000000000000000000"

[[files]]
path = "data/file.txt"
size = 1
md5 = "00000000000000000000000000000000"
"#;
        assert!(matches!(
            Manifest::from_toml(toml).unwrap_err(),
            Error::ManifestDuplicatePath { .. }
        ));
    }

    #[test]
    fn reject_absolute_path() {
        let toml = r#"
manifest_version = 1
hash_algorithm = "md5"

[[files]]
path = "C:/evil.txt"
size = 1
md5 = "00000000000000000000000000000000"
"#;
        assert!(Manifest::from_toml(toml).is_err());
    }
}
