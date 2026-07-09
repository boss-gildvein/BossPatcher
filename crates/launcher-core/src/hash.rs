use crate::error::{Error, Result};
use std::path::Path;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, BufReader};

pub const HASH_ALGORITHM_MD5: &str = "md5";
pub const HASH_BUF_SIZE: usize = 256 * 1024; // 256 KiB

/// Stream-compute the MD5 digest of a file.
pub async fn md5_file<P: AsRef<Path>>(path: P) -> Result<String> {
    let path = path.as_ref();
    let file = File::open(path)
        .await
        .map_err(|e| Error::LocalFileReadFailed {
            path: path.to_path_buf(),
            reason: e.to_string(),
        })?;
    let mut reader = BufReader::new(file);
    let mut hasher = md5::Context::new();
    let mut buf = vec![0u8; HASH_BUF_SIZE];
    loop {
        let n = reader
            .read(&mut buf)
            .await
            .map_err(|e| Error::LocalHashFailed {
                path: path.to_path_buf(),
                reason: e.to_string(),
            })?;
        if n == 0 {
            break;
        }
        hasher.consume(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.compute()))
}

/// Compute MD5 of bytes in memory (for small chunks).
pub fn md5_bytes(bytes: &[u8]) -> String {
    format!("{:x}", md5::compute(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[tokio::test]
    async fn md5_known_empty() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let result = md5_file(tmp.path()).await.unwrap();
        assert_eq!(result, "d41d8cd98f00b204e9800998ecf8427e");
    }

    #[tokio::test]
    async fn md5_known_hello() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(b"hello").unwrap();
        tmp.flush().unwrap();
        let result = md5_file(tmp.path()).await.unwrap();
        assert_eq!(result, "5d41402abc4b2a76b9719d911017c592");
    }
}
