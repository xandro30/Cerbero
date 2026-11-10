use time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaSettings {
    /// The lifetime of the certificates of the components and of Hécate
    /// (`HECATE_CERT_LIFETIME_HOURS`).
    pub certificate_lifetime: Duration,
    /// How long a ceremony waits for its confirmation.
    pub ceremony_ttl: Duration,
}

impl CaSettings {
    pub fn new(certificate_lifetime_hours: u32) -> Self {
        Self {
            certificate_lifetime: Duration::hours(i64::from(certificate_lifetime_hours)),
            ceremony_ttl: Duration::minutes(15),
        }
    }
}

impl Default for CaSettings {
    fn default() -> Self {
        Self::new(72)
    }
}
