use std::fs::{self, DirBuilder, Permissions};
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::ops::RangeInclusive;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::PathBuf;

use hecate_application::sealing::settings::{KDF_MODERATE, KDF_SENSITIVE, KdfLimits};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ConfigError(String);

pub const DEFAULT_API_PORT: u16 = 8443;
pub const DEFAULT_CERT_LIFETIME_HOURS: u32 = 72;
pub const DEFAULT_ENROLL_TOKEN_MINUTES: u32 = 15;

/// Configuration from the environment (12-factor): `HECATE_DATA_DIR`, `HECATE_SOCKET_PATH`,
/// `HECATE_KDF_PROFILE`, `HECATE_API_ADDR`, `HECATE_CERT_LIFETIME_HOURS` and
/// `HECATE_ENROLL_TOKEN_MINUTES` (research R15). None carries a secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HecateConfig {
    pub data_dir: PathBuf,
    pub socket_path: PathBuf,
    pub kdf: KdfLimits,
    pub api_addr: SocketAddr,
    pub cert_lifetime_hours: u32,
    pub enroll_token_minutes: u32,
}

/// A whole number within its range, or the error that names the variable.
fn bounded(
    get: &impl Fn(&str) -> Option<String>,
    name: &str,
    unit: &str,
    range: RangeInclusive<u32>,
) -> Result<Option<u32>, ConfigError> {
    let Some(text) = get(name) else {
        return Ok(None);
    };
    match text.parse::<u32>() {
        Ok(value) if range.contains(&value) => Ok(Some(value)),
        _ => Err(ConfigError(format!(
            "{name} must be a whole number of {unit} from {} to {}",
            range.start(),
            range.end()
        ))),
    }
}

impl HecateConfig {
    pub fn for_dir(data_dir: impl Into<PathBuf>, kdf: KdfLimits) -> Self {
        let data_dir = data_dir.into();
        Self {
            socket_path: data_dir.join("control.sock"),
            data_dir,
            kdf,
            api_addr: SocketAddr::from((Ipv4Addr::LOCALHOST, DEFAULT_API_PORT)),
            cert_lifetime_hours: DEFAULT_CERT_LIFETIME_HOURS,
            enroll_token_minutes: DEFAULT_ENROLL_TOKEN_MINUTES,
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
        if let Some(addr) = get("HECATE_API_ADDR") {
            config.api_addr = addr.parse().map_err(|_| {
                ConfigError(
                    "HECATE_API_ADDR must be an IP address and a port, such as 127.0.0.1:8443"
                        .into(),
                )
            })?;
        }
        if let Some(hours) = bounded(&get, "HECATE_CERT_LIFETIME_HOURS", "hours", 1..=72)? {
            config.cert_lifetime_hours = hours;
        }
        if let Some(minutes) = bounded(&get, "HECATE_ENROLL_TOKEN_MINUTES", "minutes", 1..=60)? {
            config.enroll_token_minutes = minutes;
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
