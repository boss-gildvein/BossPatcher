use crate::error::{Error, Result};
use std::path::{Component, Path, PathBuf};

/// Resolve a manifest/alias path relative to a base directory.
/// Rejects absolute paths and paths that escape the base directory.
pub fn resolve_relative<P: AsRef<Path>, R: AsRef<Path>>(base: P, rel: R) -> Result<PathBuf> {
    let base = base.as_ref();
    let rel = normalize_slashes(rel.as_ref())?;
    check_path_for_traversal(&rel)?;
    let joined = base.join(rel);
    canonicalize_within_base(base, &joined)
}

/// Normalize a path to forward slashes, preserving Unicode and spaces.
/// Returns an error for backslash-only absolute paths or paths containing
/// components that try to escape the root.
pub fn normalize_slashes<P: AsRef<Path>>(path: P) -> Result<PathBuf> {
    let path = path.as_ref();
    if path.is_absolute() {
        let s = path.to_string_lossy();
        return Err(Error::ManifestInvalidPath {
            path: s.into_owned(),
            reason: "absolute path".to_string(),
        });
    }
    let mut normalized = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::Prefix(_) => {
                return Err(Error::ManifestInvalidPath {
                    path: path.to_string_lossy().into_owned(),
                    reason: "prefix component not allowed".to_string(),
                })
            }
            Component::RootDir => continue,
            Component::CurDir => continue,
            Component::ParentDir => {
                return Err(Error::ManifestInvalidPath {
                    path: path.to_string_lossy().into_owned(),
                    reason: "path traversal".to_string(),
                })
            }
            Component::Normal(os) => normalized.push(os),
        }
    }
    Ok(normalized)
}

pub fn check_path_for_traversal<P: AsRef<Path>>(path: P) -> Result<()> {
    for comp in path.as_ref().components() {
        match comp {
            Component::Prefix(_) | Component::RootDir => {
                return Err(Error::ManifestInvalidPath {
                    path: path.as_ref().to_string_lossy().into_owned(),
                    reason: "absolute path".to_string(),
                })
            }
            Component::ParentDir => {
                return Err(Error::ManifestInvalidPath {
                    path: path.as_ref().to_string_lossy().into_owned(),
                    reason: "path traversal".to_string(),
                })
            }
            _ => {}
        }
    }
    Ok(())
}

/// Validate that a target stays inside base.
pub fn canonicalize_within_base(base: &Path, target: &Path) -> Result<PathBuf> {
    // Use GetFullPathName-like behavior by normalizing within the base.
    let base_abs = std::path::absolute(base).map_err(Error::Io)?;
    let target_abs = std::path::absolute(target).map_err(Error::Io)?;
    if !target_abs.starts_with(&base_abs) {
        return Err(Error::ManifestInvalidPath {
            path: target.to_string_lossy().into_owned(),
            reason: "path escapes launcher directory".to_string(),
        });
    }
    Ok(target_abs)
}

/// Validate alias target from config: must be relative, point inside launcher dir.
pub fn validate_alias_target(alias: &str, target: &str) -> Result<()> {
    if alias.trim().is_empty() {
        return Err(Error::InvalidAliasTarget {
            alias: alias.to_string(),
            reason: "alias is empty".to_string(),
        });
    }
    let path = Path::new(target);
    if path.is_absolute() {
        return Err(Error::InvalidAliasTarget {
            alias: alias.to_string(),
            reason: "absolute path".to_string(),
        });
    }
    normalize_slashes(path).map_err(|e| Error::InvalidAliasTarget {
        alias: alias.to_string(),
        reason: format!("{}", e),
    })?;
    Ok(())
}

/// Append a manifest path to a data URL base, URL-encoding each path segment.
/// Skips trailing slash handling because the caller already supplies a base
/// URL that ends with `/`.
pub fn url_path_segment_for_data_url(data_url: &str, manifest_path: &str) -> Result<url::Url> {
    let base = url::Url::parse(data_url)?;
    let mut result = base.clone();
    for seg in manifest_path.split('/') {
        if seg.is_empty() {
            continue;
        }
        result = result
            .join(&format!("{}/", urlencoding::encode(seg)))
            .map_err(|e| url::ParseError::from(e))?;
    }
    // Remove the trailing slash that join adds after the final segment.
    let mut s = result.as_str().to_string();
    if s.ends_with('/') {
        s.pop();
    }
    Ok(url::Url::parse(&s)?)
}

#[cfg(not(target_os = "linux"))]
pub fn same_windows_path_case_insensitive(left: &Path, right: &Path) -> bool {
    left.to_string_lossy().to_lowercase() == right.to_string_lossy().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal() {
        assert!(normalize_slashes("../outside.txt").is_err());
        assert!(normalize_slashes("data/../../outside.txt").is_err());
    }

    #[test]
    fn rejects_absolute() {
        assert!(normalize_slashes("C:/Windows/cmd.exe").is_err());
        assert!(normalize_slashes("C:\\Windows\\cmd.exe").is_err());
    }

    #[test]
    fn accepts_unicode_and_spaces() {
        let p = normalize_slashes("BGM/日本語/track 01.mp3").unwrap();
        let rendered = p.to_string_lossy().replace('\\', "/");
        assert_eq!(rendered, "BGM/日本語/track 01.mp3");
    }

    #[test]
    fn resolves_within_base() {
        let base = Path::new("C:\\Games\\Test");
        let resolved = resolve_relative(base, "data/file.txt").unwrap();
        assert!(resolved.starts_with(base));
    }

    #[test]
    fn rejects_escaping_resolution() {
        let base = Path::new("C:\\Games\\Test");
        assert!(resolve_relative(base, "..\\outside.txt").is_err());
    }

    #[test]
    fn validate_alias_target_rejects_absolute() {
        assert!(validate_alias_target("bad", "C:/Windows/cmd.exe").is_err());
        assert!(validate_alias_target("bad", "../bad.exe").is_err());
    }

    #[test]
    fn url_path_segment_encoding() {
        let url = url_path_segment_for_data_url(
            "https://patch.example.com/data/",
            "BGM/日本語/track 01.mp3",
        )
        .unwrap();
        assert_eq!(
            url.as_str(),
            "https://patch.example.com/data/BGM/%E6%97%A5%E6%9C%AC%E8%AA%9E/track%2001.mp3"
        );
    }
}
