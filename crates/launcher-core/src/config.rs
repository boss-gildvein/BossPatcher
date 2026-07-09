use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use url::Url;

pub const CONFIG_VERSION: i64 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    pub config_version: i64,
    pub title: String,
    pub launcher_url: String,
    pub manifest_url: String,
    pub data_url: String,
    pub calls: HashMap<String, String>,
    #[serde(default)]
    pub patch: PatchConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PatchConfig {
    #[serde(default = "default_max_concurrent_downloads")]
    pub max_concurrent_downloads: usize,
    #[serde(default = "default_verify_after_download")]
    pub verify_after_download: bool,
    #[serde(default = "default_resume_downloads")]
    pub resume_downloads: bool,
    #[serde(default = "default_hash_algorithm")]
    pub hash_algorithm: String,
}

impl Default for PatchConfig {
    fn default() -> Self {
        Self {
            max_concurrent_downloads: default_max_concurrent_downloads(),
            verify_after_download: default_verify_after_download(),
            resume_downloads: default_resume_downloads(),
            hash_algorithm: default_hash_algorithm(),
        }
    }
}

fn default_max_concurrent_downloads() -> usize {
    3
}

fn default_verify_after_download() -> bool {
    true
}

fn default_resume_downloads() -> bool {
    false
}

fn default_hash_algorithm() -> String {
    "md5".into()
}

/// Resolve the config path from the running executable path.
/// `exe_path` should be the absolute path to the launcher executable.
pub fn derive_config_path<P: AsRef<Path>>(exe_path: P) -> Result<PathBuf> {
    let exe = exe_path.as_ref();
    let dir = exe
        .parent()
        .ok_or_else(|| Error::ConfigInvalidField {
            field: "exe_path".to_string(),
            reason: "executable path has no parent directory".to_string(),
        })?;
    let file_stem = exe
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| Error::ConfigInvalidField {
            field: "exe_path".to_string(),
            reason: "executable filename is not valid UTF-8".to_string(),
        })?;
    Ok(dir.join(format!("{file_stem}.toml")))
}

/// Load and validate the config file at `path`.
pub async fn load_config<P: AsRef<Path>>(path: P) -> Result<Config> {
    let path = path.as_ref();
    let contents = tokio::fs::read_to_string(path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Error::ConfigNotFound {
                path: path.to_path_buf(),
            }
        } else {
            Error::LocalFileReadFailed {
                path: path.to_path_buf(),
                reason: e.to_string(),
            }
        }
    })?;
    let config: Config = toml::from_str(&contents).map_err(|e| {
        Error::ConfigParseFailed(format!(
            "{}: {}",
            path.display(),
            e.message().to_string()
        ))
    })?;
    validate_config(&config).await?;
    Ok(config)
}

pub async fn validate_config(config: &Config) -> Result<()> {
    if config.config_version != CONFIG_VERSION {
        return Err(Error::ConfigInvalidField {
            field: "config_version".to_string(),
            reason: format!("expected {}, got {}", CONFIG_VERSION, config.config_version),
        });
    }
    if config.title.trim().is_empty() {
        return Err(Error::ConfigMissingField {
            field: "title".to_string(),
        });
    }
    validate_url(&config.launcher_url, "launcher_url", true)?;
    validate_url(&config.manifest_url, "manifest_url", false)?;
    validate_url(&config.data_url, "data_url", false)?;
    if config.calls.is_empty() {
        return Err(Error::CallsMissing);
    }
    for (alias, target) in &config.calls {
        crate::path::validate_alias_target(alias, target)?;
    }
    let algo = config.patch.hash_algorithm.to_ascii_lowercase();
    if algo != "md5" {
        return Err(Error::ConfigInvalidField {
            field: "patch.hash_algorithm".to_string(),
            reason: format!("unsupported hash algorithm: {}", algo),
        });
    }
    Ok(())
}

pub fn validate_url(value: &str, field: &str, require_https: bool) -> Result<()> {
    let url = Url::parse(value).map_err(|e| match field {
        "launcher_url" => Error::LauncherUrlInvalid(e.to_string()),
        "manifest_url" => Error::ManifestUrlInvalid(e.to_string()),
        "data_url" => Error::DataUrlInvalid(e.to_string()),
        _ => Error::ConfigInvalidField {
            field: field.to_string(),
            reason: e.to_string(),
        },
    })?;
    if require_https && url.scheme() != "https" {
        return Err(Error::LauncherUrlNotHttps(value.to_string()));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Origin {
    pub scheme: String,
    pub host: String,
    pub port: Option<u16>,
}

impl Origin {
    pub fn from_url_str(value: &str) -> Result<Self> {
        let url = Url::parse(value)?;
        let host = url
            .host_str()
            .ok_or_else(|| Error::LauncherUrlInvalid("missing host".into()))?
            .to_string();
        Ok(Self {
            scheme: url.scheme().to_string(),
            host,
            port: url.port(),
        })
    }

    pub fn matches(&self, other: &Origin) -> bool {
        self.scheme == other.scheme && self.host == other.host && self.port == other.port
    }

    pub fn as_core_origin(&self) -> url::Url {
        let port_part = self.port.map(|p| format!(":{p}")).unwrap_or_default();
        let s = format!("{}://{}{}", self.scheme, self.host, port_part);
        url::Url::parse(&s).unwrap_or_else(|_| {
            url::Url::parse("https://localhost").expect("localhost url is valid")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> Config {
        Config {
            config_version: 1,
            title: "BossPatcher Launcher".into(),
            launcher_url: "https://launcher.example.com/".into(),
            manifest_url: "https://patch.example.com/manifest.toml".into(),
            data_url: "https://patch.example.com/data/".into(),
            calls: [("game".into(), "Game.exe".into())].into_iter().collect(),
            patch: PatchConfig::default(),
        }
    }

    #[test]
    fn derive_config_path_works() {
        assert_eq!(
            derive_config_path("C:\\Games\\Test\\testLauncher.exe").unwrap(),
            PathBuf::from("C:\\Games\\Test\\testLauncher.toml")
        );
        assert_eq!(
            derive_config_path("C:\\Games\\Test\\BossPatcher.exe").unwrap(),
            PathBuf::from("C:\\Games\\Test\\BossPatcher.toml")
        );
    }

    #[tokio::test]
    async fn validate_config_accepts_valid() {
        let cfg = sample_config();
        validate_config(&cfg).await.unwrap();
    }

    #[tokio::test]
    async fn validate_config_rejects_http_launcher_url() {
        let mut cfg = sample_config();
        cfg.launcher_url = "http://launcher.example.com/".into();
        assert!(matches!(
            validate_config(&cfg).await.unwrap_err(),
            Error::LauncherUrlNotHttps(_)
        ));
    }

    #[tokio::test]
    async fn validate_config_rejects_empty_calls() {
        let mut cfg = sample_config();
        cfg.calls.clear();
        assert!(matches!(validate_config(&cfg).await.unwrap_err(), Error::CallsMissing));
    }
}
