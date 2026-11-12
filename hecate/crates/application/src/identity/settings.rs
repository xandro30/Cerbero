use time::Duration;

/// The least time that a refusal of the resolution takes by default (research R2).
pub const REFUSAL_FLOOR: std::time::Duration = std::time::Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdentitySettings {
    /// How long an enrollment token stays pending (`HECATE_ENROLL_TOKEN_MINUTES`).
    pub token_ttl: Duration,
    /// The least time that a `not_found` of the resolution takes since the request reaches its
    /// handler, so that its three cases are not told apart by their time (FR-004); the tests
    /// lower it.
    pub refusal_floor: std::time::Duration,
}

impl IdentitySettings {
    pub fn new(enroll_token_minutes: u32) -> Self {
        Self {
            token_ttl: Duration::minutes(i64::from(enroll_token_minutes)),
            refusal_floor: REFUSAL_FLOOR,
        }
    }
}

impl Default for IdentitySettings {
    fn default() -> Self {
        Self::new(15)
    }
}
