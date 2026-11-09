use time::OffsetDateTime;

macro_rules! text_enum {
    ($name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            pub fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text),+
                }
            }

            pub fn parse(text: &str) -> Option<Self> {
                match text {
                    $($text => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }
    };
}

text_enum!(Operation {
    Init => "init",
    Unseal => "unseal",
    Seal => "seal",
    PassphraseChange => "passphrase_change",
    SharesRekey => "shares_rekey",
    SecretCreate => "secret_create",
    SecretAddVersion => "secret_add_version",
    SecretList => "secret_list",
    SecretGet => "secret_get",
    SecretRevoke => "secret_revoke",
    SecretRestore => "secret_restore",
    SecretPromote => "secret_promote",
    SecretDestroy => "secret_destroy",
    SecretResolve => "secret_resolve",
});

text_enum!(Method {
    Passphrase => "passphrase",
    Recovery => "recovery",
    Local => "local",
    Identity => "identity",
});

text_enum!(AccessResult {
    Success => "success",
    Failure => "failure",
    RejectedRateLimited => "rejected_rate_limited",
    RejectedConflict => "rejected_conflict",
    InvalidInput => "invalid_input",
    NotFound => "not_found",
});

/// One row of the append-only access log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessLogEntry {
    pub at: OffsetDateTime,
    pub operation: Operation,
    pub method: Method,
    pub result: AccessResult,
    pub detail: String,
    pub subject: Option<String>,
    pub path: Option<String>,
    pub version: Option<u64>,
}
