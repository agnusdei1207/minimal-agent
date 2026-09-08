use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;

use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::constants::BACKUP_FILE_NAME;

pub fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub fn atomic_replace(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "brief path has no parent")
    })?;
    fs::create_dir_all(parent)?;
    let backup = parent.join(BACKUP_FILE_NAME);
    if backup.exists() && !path.exists() {
        fs::rename(&backup, path)?;
    } else if backup.exists() {
        fs::remove_file(&backup)?;
    }

    let temp = parent.join(format!(".brief.{}.tmp", Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp)?;
    file.write_all(bytes)?;
    file.flush()?;
    file.sync_all()?;
    drop(file);

    let had_previous = path.exists();
    if had_previous {
        fs::rename(path, &backup)?;
    }
    if let Err(error) = fs::rename(&temp, path) {
        let _ = fs::remove_file(&temp);
        if had_previous {
            let _ = fs::rename(&backup, path);
        }
        return Err(error);
    }
    if had_previous {
        fs::remove_file(&backup)?;
    }
    sync_directory(parent)?;
    Ok(())
}

#[cfg(unix)]
pub fn sync_directory(path: &Path) -> std::io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(not(unix))]
pub fn sync_directory(_path: &Path) -> std::io::Result<()> {
    Ok(())
}
