use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use super::constants::PROVIDER_SETTINGS_FILE;
use super::provider::ProviderSettings;

#[derive(Debug, Clone)]
pub struct ProviderSettingsStore {
    path: PathBuf,
}

impl ProviderSettingsStore {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            path: root.as_ref().join(PROVIDER_SETTINGS_FILE),
        }
    }

    pub fn path(&self) -> PathBuf {
        self.path.clone()
    }

    pub fn load(&self) -> anyhow::Result<Option<ProviderSettings>> {
        match fs::read(&self.path) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn save(&self, settings: &ProviderSettings) -> anyhow::Result<()> {
        let parent = self.path.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "provider settings need a parent",
            )
        })?;
        fs::create_dir_all(parent)?;
        let temporary = self
            .path
            .with_extension(format!("json.{}.tmp", std::process::id()));
        let result = (|| -> anyhow::Result<()> {
            let mut options = OpenOptions::new();
            options.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temporary)?;
            serde_json::to_writer(&mut file, settings)?;
            file.write_all(b"\n")?;
            file.sync_all()?;
            fs::rename(&temporary, &self.path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}
