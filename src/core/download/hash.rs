use std::fs::File;
use std::io::Read;
use std::path::Path;

use sha1::{Digest, Sha1};

use crate::core::error::{Error, Result};

pub fn sha1_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut digest = Sha1::new();
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub fn verify_sha1(path: &Path, expected: Option<&str>) -> Result<()> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let actual = sha1_file(path)?;
    if actual.eq_ignore_ascii_case(expected.trim()) {
        Ok(())
    } else {
        Err(Error::Download(format!(
            "SHA-1 mismatch for {}: expected {}, got {}",
            path.display(),
            expected,
            actual
        )))
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{sha1_file, verify_sha1};

    #[test]
    fn verifies_correct_and_incorrect_hashes() {
        let root = std::env::temp_dir().join(format!(
            "tmcl-hash-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("create test directory");
        let path = root.join("fixture.bin");
        std::fs::write(&path, b"tmcl").expect("write fixture");

        assert_eq!(
            sha1_file(&path).expect("hash file"),
            "c42b2247c7467caed77ee71cc67a234c327aa9d3"
        );
        assert!(verify_sha1(&path, Some("C42B2247C7467CAED77EE71CC67A234C327AA9D3")).is_ok());
        assert!(verify_sha1(&path, Some("bad-hash")).is_err());
        assert!(verify_sha1(&path, None).is_ok());

        std::fs::remove_dir_all(root).expect("clean test directory");
    }
}
