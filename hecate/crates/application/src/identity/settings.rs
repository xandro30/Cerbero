use time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdentitySettings {
    /// How long an enrollment token stays pending (`HECATE_ENROLL_TOKEN_MINUTES`).
    pub token_ttl: Duration,
}

impl IdentitySettings {
    pub fn new(enroll_token_minutes: u32) -> Self {
        Self {
            token_ttl: Duration::minutes(i64::from(enroll_token_minutes)),
        }
    }
}

impl Default for IdentitySettings {
    fn default() -> Self {
        Self::new(15)
    }
}
