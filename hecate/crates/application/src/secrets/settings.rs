use time::Duration;

/// The rules of secrets that the Python version fixed in `SecretsSettings`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecretsSettings {
    /// How long a version replaced by a new one still resolves.
    pub grace: Duration,
    pub default_max_versions: u64,
    pub cache_ttl_soft_s: u64,
    pub cache_ttl_hard_s: u64,
    /// How far ahead `status` counts a secret as expiring soon.
    pub expiring_soon: Duration,
}

impl Default for SecretsSettings {
    fn default() -> Self {
        Self {
            grace: Duration::hours(24),
            default_max_versions: 10,
            cache_ttl_soft_s: 300,
            cache_ttl_hard_s: 3600,
            expiring_soon: Duration::days(7),
        }
    }
}
