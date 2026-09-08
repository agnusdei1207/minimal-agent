use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use uuid::Uuid;

use super::error::JournalError;
use super::record::digest;

pub fn write_blob(path: &Path, bytes: &[u8]) -> Result<(), JournalError> {
    if path.exists() {
        let actual = path.metadata()?.len();
        if actual != bytes.len() as u64 {
            return Err(JournalError::BlobLengthMismatch {
                expected: bytes.len() as u64,
                actual,
            });
        }
        let existing = fs::read(path)?;
        return (digest(&existing) == digest(bytes))
            .then_some(())
            .ok_or_else(|| JournalError::BlobDigestMismatch {
                expected: digest(bytes),
                actual: digest(&existing),
            });
    }
    let temp = path.with_extension(format!("tmp-{}", Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp)?;
    file.write_all(bytes)?;
    file.flush()?;
    file.sync_all()?;
    match fs::rename(&temp, path) {
        Ok(()) => Ok(()),
        Err(error) if path.exists() => {
            let _ = fs::remove_file(temp);
            let actual = fs::read(path)?;
            if digest(&actual) == digest(bytes) {
                Ok(())
            } else {
                Err(JournalError::Io(error))
            }
        }
        Err(error) => {
            let _ = fs::remove_file(temp);
            Err(JournalError::Io(error))
        }
    }
}
