use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("config not found: {path}")]
    ConfigNotFound { path: PathBuf },
    #[error("failed to parse config: {0}")]
    ConfigParseFailed(String),
    #[error("config missing required field: {field}")]
    ConfigMissingField { field: String },
    #[error("config invalid field `{field}`: {reason}")]
    ConfigInvalidField { field: String, reason: String },
    #[error("launcher_url invalid: {0}")]
    LauncherUrlInvalid(String),
    #[error("launcher_url must use https in production: {0}")]
    LauncherUrlNotHttps(String),
    #[error("manifest_url invalid: {0}")]
    ManifestUrlInvalid(String),
    #[error("data_url invalid: {0}")]
    DataUrlInvalid(String),
    #[error("[calls] section is missing")]
    CallsMissing,
    #[error("unknown alias: {alias}")]
    UnknownAlias { alias: String },
    #[error("invalid alias target `{alias}`: {reason}")]
    InvalidAliasTarget { alias: String, reason: String },
    #[error("target not found: {path}")]
    TargetNotFound { path: PathBuf },
    #[error("target is not an executable: {path}")]
    TargetNotExecutable { path: PathBuf },
    #[error("failed to start process `{path}`: {reason}")]
    ProcessStartFailed { path: PathBuf, reason: String },
    #[error("patch already running")]
    PatchAlreadyRunning,
    #[error("patch in progress")]
    PatchInProgress,
    #[error("manifest download failed: {0}")]
    ManifestDownloadFailed(String),
    #[error("manifest parse failed: {0}")]
    ManifestParseFailed(String),
    #[error("unsupported manifest version: {0}")]
    ManifestUnsupportedVersion(i64),
    #[error("unsupported hash algorithm: {0}")]
    ManifestUnsupportedHash(String),
    #[error("duplicate manifest path: {path}")]
    ManifestDuplicatePath { path: String },
    #[error("invalid manifest path `{path}`: {reason}")]
    ManifestInvalidPath { path: String, reason: String },
    #[error("protected file skipped: {path}")]
    ProtectedFileSkipped { path: PathBuf },
    #[error("local file read failed `{path}`: {reason}")]
    LocalFileReadFailed { path: PathBuf, reason: String },
    #[error("local hash failed `{path}`: {reason}")]
    LocalHashFailed { path: PathBuf, reason: String },
    #[error("download failed for `{path}`: {reason}")]
    DownloadFailed { path: String, reason: String },
    #[error("download cancelled")]
    DownloadCancelled,
    #[error("hash mismatch for `{path}`")]
    HashMismatch { path: PathBuf },
    #[error("file locked `{path}`: {reason}")]
    FileLocked { path: PathBuf, reason: String },
    #[error("replace failed `{path}`: {reason}")]
    ReplaceFailed { path: PathBuf, reason: String },
    #[error("remote UI load failed")]
    RemoteUiLoadFailed,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    Url(#[from] url::ParseError),
}

pub type Result<T> = std::result::Result<T, Error>;
