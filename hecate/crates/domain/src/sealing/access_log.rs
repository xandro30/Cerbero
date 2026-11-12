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
    CaCreate => "ca_create",
    CaIntermediateRenew => "ca_intermediate_renew",
    CaReplace => "ca_replace",
    TokenCreate => "token_create",
    TokenList => "token_list",
    TokenAnnul => "token_annul",
    IdentityList => "identity_list",
    IdentityGet => "identity_get",
    IdentityDisable => "identity_disable",
    IdentityEnable => "identity_enable",
    IdentityEnroll => "identity_enroll",
    IdentityEnrollThrottled => "identity_enroll_throttled",
    CertificateRenew => "certificate_renew",
    CertificateRevoke => "certificate_revoke",
    KeyList => "key_list",
    KeyGet => "key_get",
    RevocationList => "revocation_list",
    CapabilitiesGet => "capabilities_get",
    PolicyGrant => "policy_grant",
    PolicyRevoke => "policy_revoke",
    PolicyList => "policy_list",
    PolicyGet => "policy_get",
    StatusGet => "status_get",
});

text_enum!(Method {
    Passphrase => "passphrase",
    Recovery => "recovery",
    Local => "local",
    Identity => "identity",
    EnrollmentToken => "enrollment_token",
    Internal => "internal",
});

text_enum!(AccessResult {
    Success => "success",
    Failure => "failure",
    RejectedRateLimited => "rejected_rate_limited",
    RejectedConflict => "rejected_conflict",
    InvalidInput => "invalid_input",
    NotFound => "not_found",
});

/// One row of the append-only access log; `identity` and `certificate` are the identity and the
/// serial an operation of the identities touches (FR-024).
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
    pub identity: Option<String>,
    pub certificate: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_operations_of_the_access_policy_and_the_status_have_their_text() {
        for (operation, text) in [
            (Operation::PolicyGrant, "policy_grant"),
            (Operation::PolicyRevoke, "policy_revoke"),
            (Operation::PolicyList, "policy_list"),
            (Operation::PolicyGet, "policy_get"),
            (Operation::StatusGet, "status_get"),
        ] {
            assert_eq!(operation.as_str(), text);
            assert_eq!(Operation::parse(text), Some(operation));
        }
        assert_eq!(Operation::parse("policy"), None);
    }
}
