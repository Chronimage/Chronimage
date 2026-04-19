use crate::{AppError, AppResult};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};

/// Compute the SHA256 of the file at `path`, streaming in 64 KiB chunks so
/// a 120 MB ARW doesn't balloon memory.
pub fn sha256_file(path: &Path) -> AppResult<String> {
    let mut f = File::open(path).map_err(|e| {
        AppError::Io(std::io::Error::new(
            e.kind(),
            format!("open {} failed: {}", path.display(), e),
        ))
    })?;
    let mut digest = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        digest.update(&buf[..n]);
    }
    Ok(hex::encode(digest.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn hashes_a_small_file() {
        let tmp = TempDir::new().expect("tempdir");
        let p = tmp.path().join("x.bin");
        fs::write(&p, b"hello").expect("write");
        let h = sha256_file(&p).expect("hash");
        assert_eq!(
            h,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn errors_on_missing_file() {
        let err = sha256_file(Path::new("/no/such/file/chronimage-test")).unwrap_err();
        assert!(matches!(err, AppError::Io(_)));
    }
}
