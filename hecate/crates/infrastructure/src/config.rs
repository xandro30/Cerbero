use std::fs::{self, DirBuilder, Permissions};
use std::io;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::PathBuf;

use hecate_application::sealing::settings::{KDF_MODERATE, KDF_SENSITIVE, KdfLimits};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ConfigError(String);

/// Configuration from the environment (12-factor): `HECATE_DATA_DIR`, `HECATE_SOCKET_PATH` and
/// `HECATE_KDF_PROFILE`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HecateConfig {
    pub data_dir: PathBuf,
    pub socket_path: PathBuf,
    pub kdf: KdfLimits,
}

impl HecateConfig {
    pub fn for_dir(data_dir: impl Into<PathBuf>, kdf: KdfLimits) -> Self {
        let data_dir = data_dir.into();
        Self {
            socket_path: data_dir.join("control.sock"),
            data_dir,
            kdf,
        }
    }

    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_vars(|name| std::env::var(name).ok())
    }

    pub fn from_vars(get: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let data_dir = get("HECATE_DATA_DIR")
            .filter(|dir| !dir.is_empty())
            .ok_or_else(|| ConfigError("HECATE_DATA_DIR is required".into()))?;
        let kdf = match get("HECATE_KDF_PROFILE")
            .unwrap_or_else(|| "sensitive".into())
            .to_lowercase()
            .as_str()
        {
            "sensitive" => KDF_SENSITIVE,
            "moderate" => KDF_MODERATE,
            _ => {
                return Err(ConfigError(
                    "HECATE_KDF_PROFILE must be one of ['moderate', 'sensitive']".into(),
                ));
            }
        };
        let mut config = Self::for_dir(data_dir, kdf);
        if let Some(socket_path) = get("HECATE_SOCKET_PATH").filter(|path| !path.is_empty()) {
            config.socket_path = PathBuf::from(socket_path);
        }
        Ok(config)
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("hecate.db")
    }

    /// Creates the data directory, private to the Hécate user.
    pub fn prepare(&self) -> io::Result<()> {
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.data_dir)?;
        fs::set_permissions(&self.data_dir, Permissions::from_mode(0o700))
    }
}
