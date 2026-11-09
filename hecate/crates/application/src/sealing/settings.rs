/// Argon2id cost limits; stored with every passphrase protection so they can be raised later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdfLimits {
    pub opslimit: u64,
    pub memlimit: usize,
}

/// libsodium's SENSITIVE profile: 4 passes and 1 GiB.
pub const KDF_SENSITIVE: KdfLimits = KdfLimits {
    opslimit: 4,
    memlimit: 1 << 30,
};

/// libsodium's MODERATE profile: 3 passes and 256 MiB.
pub const KDF_MODERATE: KdfLimits = KdfLimits {
    opslimit: 3,
    memlimit: 256 << 20,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SealingSettings {
    pub kdf: KdfLimits,
    /// How long a pending initialization or share set waits for its confirmation.
    pub pending_ttl: time::Duration,
}

impl SealingSettings {
    pub fn new(kdf: KdfLimits) -> Self {
        Self {
            kdf,
            pending_ttl: time::Duration::minutes(15),
        }
    }
}
