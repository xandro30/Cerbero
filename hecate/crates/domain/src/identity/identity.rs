//! The Identity aggregate (stream `identity/<name>`, data-model.md): its enrollment tokens, its
//! enrollments, whether it is enabled and its epoch. It is born with its first enrollment; before
//! that it only has tokens.

use std::collections::BTreeMap;

use time::OffsetDateTime;

use crate::events::{IdentityEvent, InvalidEvents};
use crate::identity::names::{IdentityName, Serial, TokenId};
use crate::sealing::errors::HecateError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityStatus {
    NotEnrolled,
    Enrolled,
    Disabled,
}

impl IdentityStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotEnrolled => "not_enrolled",
            Self::Enrolled => "enrolled",
            Self::Disabled => "disabled",
        }
    }
}

/// An enrollment token as the identity keeps it: the SHA-256 of its secret, never the token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenRecord {
    pub id: TokenId,
    pub secret_sha256: [u8; 32],
    pub generation: u64,
    pub created_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
    /// The serial that its enrollment issued.
    pub used_by: Option<Serial>,
    pub annulled: bool,
    /// Its place in the stream, to tell whether it came after the last disable.
    position: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub name: IdentityName,
    pub status: IdentityStatus,
    /// Grows with every enrollment and every disable: a certificate renews only with the epoch
    /// of its identity.
    pub epoch: u64,
    pub urgently_disabled: bool,
    pub tokens: BTreeMap<TokenId, TokenRecord>,
    pub suspicious_renewals: Vec<OffsetDateTime>,
    pub stream_version: u64,
    disabled_at: u64,
}

impl Identity {
    /// An identity with nothing yet: no tokens and no enrollment.
    pub fn new(name: IdentityName) -> Self {
        Self {
            name,
            status: IdentityStatus::NotEnrolled,
            epoch: 0,
            urgently_disabled: false,
            tokens: BTreeMap::new(),
            suspicious_renewals: Vec::new(),
            stream_version: 0,
            disabled_at: 0,
        }
    }

    pub fn from_events(
        name: IdentityName,
        events: &[IdentityEvent],
    ) -> Result<Self, InvalidEvents> {
        let mut identity = Self::new(name);
        for event in events {
            identity.apply(event)?;
        }
        Ok(identity)
    }

    fn token_mut(&mut self, id: &TokenId) -> Result<&mut TokenRecord, InvalidEvents> {
        self.tokens
            .get_mut(id)
            .ok_or(InvalidEvents("an event names a token that does not exist"))
    }

    pub fn apply(&mut self, event: &IdentityEvent) -> Result<(), InvalidEvents> {
        let position = self.stream_version + 1;
        match event {
            IdentityEvent::EnrollmentTokenCreated {
                at,
                token_id,
                secret_sha256,
                generation,
                expires_at,
            } => {
                if self.tokens.contains_key(token_id) {
                    return Err(InvalidEvents("a token is created only once"));
                }
                self.tokens.insert(
                    token_id.clone(),
                    TokenRecord {
                        id: token_id.clone(),
                        secret_sha256: *secret_sha256,
                        generation: *generation,
                        created_at: *at,
                        expires_at: *expires_at,
                        used_by: None,
                        annulled: false,
                        position,
                    },
                );
            }
            IdentityEvent::EnrollmentTokenAnnulled { token_id, .. } => {
                let token = self.token_mut(token_id)?;
                if token.annulled || token.used_by.is_some() {
                    return Err(InvalidEvents("only an unused token is annulled"));
                }
                token.annulled = true;
            }
            IdentityEvent::IdentityEnrolled {
                token_id, serial, ..
            } => {
                if self.status == IdentityStatus::Disabled {
                    return Err(InvalidEvents("a disabled identity does not enroll"));
                }
                let token = self.token_mut(token_id)?;
                if token.annulled || token.used_by.is_some() {
                    return Err(InvalidEvents("a token is used only once"));
                }
                token.used_by = Some(serial.clone());
                self.status = IdentityStatus::Enrolled;
                self.epoch += 1;
            }
            IdentityEvent::IdentityDisabled { urgent, .. } => {
                // A second disable only makes a passive one urgent.
                let allowed = match self.status {
                    IdentityStatus::Enrolled => true,
                    IdentityStatus::Disabled => *urgent && !self.urgently_disabled,
                    IdentityStatus::NotEnrolled => false,
                };
                if !allowed {
                    return Err(InvalidEvents("this identity cannot be disabled"));
                }
                self.status = IdentityStatus::Disabled;
                self.urgently_disabled |= *urgent;
                self.epoch += 1;
                self.disabled_at = position;
            }
            IdentityEvent::IdentityEnabled { .. } => {
                if self.status != IdentityStatus::Disabled {
                    return Err(InvalidEvents("only a disabled identity is enabled"));
                }
                self.status = IdentityStatus::Enrolled;
                self.urgently_disabled = false;
            }
            IdentityEvent::SuspiciousRenewalDetected { at, .. } => {
                self.suspicious_renewals.push(*at);
            }
        }
        self.stream_version = position;
        Ok(())
    }

    fn is_pending(&self, token: &TokenRecord, generation: u64, now: OffsetDateTime) -> bool {
        token.used_by.is_none()
            && !token.annulled
            && now < token.expires_at
            && token.generation == generation
            && token.position > self.disabled_at
    }

    /// A token that still enrolls: unused, not annulled, not expired, of the current generation
    /// of the CA and created after the last disable.
    pub fn pending_token(
        &self,
        id: &TokenId,
        generation: u64,
        now: OffsetDateTime,
    ) -> Option<&TokenRecord> {
        self.tokens
            .get(id)
            .filter(|token| self.is_pending(token, generation, now))
    }

    pub fn pending_tokens(&self, generation: u64, now: OffsetDateTime) -> Vec<&TokenRecord> {
        self.tokens
            .values()
            .filter(|token| self.is_pending(token, generation, now))
            .collect()
    }

    /// A new token, never for a reserved identity (FR-007) nor for a disabled one (FR-019).
    pub fn create_token(
        &self,
        id: TokenId,
        secret_sha256: [u8; 32],
        generation: u64,
        expires_at: OffsetDateTime,
        at: OffsetDateTime,
    ) -> Result<IdentityEvent, HecateError> {
        if self.name.is_reserved() {
            return Err(HecateError::invalid(format!(
                "the identity '{}' is reserved",
                self.name
            )));
        }
        if self.status == IdentityStatus::Disabled {
            return Err(HecateError::state_conflict(
                None,
                Some("the identity is disabled".to_string()),
                None,
            ));
        }
        if self.tokens.contains_key(&id) {
            return Err(HecateError::state_conflict(
                None,
                Some("the token id already exists".to_string()),
                None,
            ));
        }
        Ok(IdentityEvent::EnrollmentTokenCreated {
            at,
            token_id: id,
            secret_sha256,
            generation,
            expires_at,
        })
    }

    /// The annulment of a pending token; one that is used, annulled or expired changes nothing.
    pub fn annul(
        &self,
        id: &TokenId,
        generation: u64,
        now: OffsetDateTime,
        at: OffsetDateTime,
    ) -> Result<Option<IdentityEvent>, HecateError> {
        let token = self
            .tokens
            .get(id)
            .ok_or_else(|| HecateError::not_found("token not found"))?;
        Ok(self.is_pending(token, generation, now).then(|| {
            IdentityEvent::EnrollmentTokenAnnulled {
                at,
                token_id: id.clone(),
            }
        }))
    }

    /// An enrolled identity exists for the orders of the admin; one with only tokens does not.
    fn require_enrolled(&self) -> Result<(), HecateError> {
        if self.status == IdentityStatus::NotEnrolled {
            return Err(HecateError::not_found("identity not found"));
        }
        Ok(())
    }

    /// The disable, or nothing when it changes nothing: a disabled identity only takes a new
    /// urgency (FR-019, FR-026). `hecate` is never disabled.
    pub fn disable(
        &self,
        urgent: bool,
        at: OffsetDateTime,
    ) -> Result<Option<IdentityEvent>, HecateError> {
        if self.name == IdentityName::hecate() {
            return Err(HecateError::invalid(
                "the identity 'hecate' cannot be disabled",
            ));
        }
        self.require_enrolled()?;
        let changes = match self.status {
            IdentityStatus::Disabled => urgent && !self.urgently_disabled,
            _ => true,
        };
        Ok(changes.then_some(IdentityEvent::IdentityDisabled { at, urgent }))
    }

    /// The enable of a disabled identity, or nothing when it is enabled already.
    pub fn enable(&self, at: OffsetDateTime) -> Result<Option<IdentityEvent>, HecateError> {
        self.require_enrolled()?;
        Ok((self.status == IdentityStatus::Disabled)
            .then_some(IdentityEvent::IdentityEnabled { at }))
    }
}

#[cfg(test)]
mod tests {
    use time::Duration;
    use time::macros::datetime;

    use super::*;
    use crate::events::IdentityEvent;
    use crate::identity::names::{IdentityName, Serial, TokenId};

    const AT: OffsetDateTime = datetime!(2026-11-09 12:00 UTC);
    const LATER: OffsetDateTime = datetime!(2026-11-09 12:05 UTC);

    fn hermes() -> IdentityName {
        IdentityName::parse("hermes").unwrap()
    }

    fn token(byte: u8) -> TokenId {
        TokenId::from_bytes([byte; 8])
    }

    fn serial(byte: u8) -> Serial {
        Serial::from_be_bytes(&[byte; 16]).unwrap()
    }

    fn created(byte: u8, generation: u64) -> IdentityEvent {
        IdentityEvent::EnrollmentTokenCreated {
            at: AT,
            token_id: token(byte),
            secret_sha256: [byte; 32],
            generation,
            expires_at: AT + Duration::minutes(15),
        }
    }

    fn enrolled(byte: u8) -> IdentityEvent {
        IdentityEvent::IdentityEnrolled {
            at: AT,
            token_id: token(byte),
            serial: serial(byte),
        }
    }

    fn disabled(urgent: bool) -> IdentityEvent {
        IdentityEvent::IdentityDisabled { at: AT, urgent }
    }

    fn enabled() -> IdentityEvent {
        IdentityEvent::IdentityEnabled { at: AT }
    }

    fn identity(events: &[IdentityEvent]) -> Identity {
        Identity::from_events(hermes(), events).unwrap()
    }

    fn pending(identity: &Identity, byte: u8) -> bool {
        identity.pending_token(&token(byte), 1, LATER).is_some()
    }

    #[test]
    fn an_identity_is_born_with_its_first_enrollment() {
        assert_eq!(identity(&[]).status, IdentityStatus::NotEnrolled);
        assert_eq!(
            identity(&[created(1, 1)]).status,
            IdentityStatus::NotEnrolled
        );
        let events = [created(1, 1), enrolled(1)];
        assert_eq!(identity(&events).status, IdentityStatus::Enrolled);
        let events = [created(1, 1), enrolled(1), disabled(false)];
        assert_eq!(identity(&events).status, IdentityStatus::Disabled);
        let events = [created(1, 1), enrolled(1), disabled(false), enabled()];
        assert_eq!(identity(&events).status, IdentityStatus::Enrolled);
        assert_eq!(identity(&events).stream_version, 4);
    }

    #[test]
    fn the_epoch_grows_with_each_enrollment_and_each_disable() {
        let mut events = vec![created(1, 1), enrolled(1)];
        assert_eq!(identity(&events).epoch, 1);
        events.extend([disabled(false), enabled()]);
        assert_eq!(identity(&events).epoch, 2);
        events.extend([created(2, 1), enrolled(2)]);
        assert_eq!(identity(&events).epoch, 3);
        events.extend([disabled(false), disabled(true)]);
        let urgent = identity(&events);
        assert_eq!((urgent.epoch, urgent.urgently_disabled), (5, true));
    }

    #[test]
    fn a_token_is_pending_until_used_annulled_expired_replaced_or_disabled() {
        let fresh = identity(&[created(1, 1)]);
        assert!(pending(&fresh, 1));
        assert!(
            fresh
                .pending_token(&token(1), 1, AT + Duration::minutes(15))
                .is_none()
        );
        assert!(fresh.pending_token(&token(1), 2, LATER).is_none());
        assert!(!pending(&fresh, 2));
        assert!(!pending(&identity(&[created(1, 1), enrolled(1)]), 1));
        let annulled = IdentityEvent::EnrollmentTokenAnnulled {
            at: AT,
            token_id: token(1),
        };
        assert!(!pending(&identity(&[created(1, 1), annulled]), 1));
        let before_disable = [
            created(1, 1),
            enrolled(1),
            created(2, 1),
            disabled(false),
            enabled(),
            created(3, 1),
        ];
        let identity = identity(&before_disable);
        assert!(!pending(&identity, 2));
        assert!(pending(&identity, 3));
        let ids: Vec<&TokenId> = identity
            .pending_tokens(1, LATER)
            .iter()
            .map(|record| &record.id)
            .collect();
        assert_eq!(ids, vec![&token(3)]);
    }

    #[test]
    fn tokens_are_refused_for_reserved_or_disabled_identities() {
        let refused = Identity::new(IdentityName::hecate())
            .create_token(token(1), [1; 32], 1, LATER, AT)
            .unwrap_err();
        assert!(
            matches!(refused, HecateError::InvalidInput(_)),
            "{refused:?}"
        );
        let admin = IdentityName::parse("admin").unwrap();
        assert!(
            Identity::new(admin)
                .create_token(token(1), [1; 32], 1, LATER, AT)
                .is_err()
        );
        let off = identity(&[created(1, 1), enrolled(1), disabled(false)]);
        let conflict = off
            .create_token(token(2), [2; 32], 1, LATER, AT)
            .unwrap_err();
        assert!(
            matches!(conflict, HecateError::StateConflict { .. }),
            "{conflict:?}"
        );
        let event = identity(&[])
            .create_token(token(1), [1; 32], 1, LATER, AT)
            .unwrap();
        assert_eq!(
            event,
            IdentityEvent::EnrollmentTokenCreated {
                at: AT,
                token_id: token(1),
                secret_sha256: [1; 32],
                generation: 1,
                expires_at: LATER,
            }
        );
    }

    #[test]
    fn annulling_a_token_that_is_no_longer_pending_changes_nothing() {
        let fresh = identity(&[created(1, 1), created(2, 1), enrolled(2)]);
        assert!(matches!(
            fresh.annul(&token(9), 1, LATER, AT).unwrap_err(),
            HecateError::NotFound(_)
        ));
        assert_eq!(
            fresh.annul(&token(1), 1, LATER, AT).unwrap(),
            Some(IdentityEvent::EnrollmentTokenAnnulled {
                at: AT,
                token_id: token(1),
            })
        );
        assert_eq!(fresh.annul(&token(2), 1, LATER, AT).unwrap(), None);
        let expired = AT + Duration::hours(1);
        assert_eq!(fresh.annul(&token(1), 1, expired, AT).unwrap(), None);
    }

    #[test]
    fn disabling_again_changes_nothing_unless_the_urgency_is_new() {
        let on = identity(&[created(1, 1), enrolled(1)]);
        assert_eq!(on.disable(false, AT).unwrap(), Some(disabled(false)));
        assert_eq!(on.disable(true, AT).unwrap(), Some(disabled(true)));
        let off = identity(&[created(1, 1), enrolled(1), disabled(false)]);
        assert_eq!(off.disable(false, AT).unwrap(), None);
        assert_eq!(off.disable(true, AT).unwrap(), Some(disabled(true)));
        let urgent = identity(&[created(1, 1), enrolled(1), disabled(false), disabled(true)]);
        assert_eq!(urgent.disable(true, AT).unwrap(), None);
        assert_eq!(urgent.disable(false, AT).unwrap(), None);
    }

    #[test]
    fn enabling_changes_only_a_disabled_identity() {
        let off = identity(&[created(1, 1), enrolled(1), disabled(true)]);
        assert_eq!(off.enable(AT).unwrap(), Some(enabled()));
        let on = identity(&[created(1, 1), enrolled(1), disabled(true), enabled()]);
        assert_eq!(on.enable(AT).unwrap(), None);
    }

    #[test]
    fn hecate_is_not_disabled_and_an_identity_without_enrollment_does_not_exist() {
        let hecate = Identity::new(IdentityName::hecate());
        for urgent in [false, true] {
            let refused = hecate.disable(urgent, AT).unwrap_err();
            assert!(
                matches!(refused, HecateError::InvalidInput(_)),
                "{refused:?}"
            );
        }
        let only_tokens = identity(&[created(1, 1)]);
        assert!(matches!(
            only_tokens.disable(false, AT).unwrap_err(),
            HecateError::NotFound(_)
        ));
        assert!(matches!(
            only_tokens.enable(AT).unwrap_err(),
            HecateError::NotFound(_)
        ));
    }

    #[test]
    fn streams_that_break_the_rules_are_refused() {
        for events in [
            vec![enrolled(1)],
            vec![created(1, 1), created(1, 1)],
            vec![created(1, 1), enrolled(1), enrolled(1)],
            vec![disabled(false)],
            vec![created(1, 1), enrolled(1), enabled()],
            vec![created(1, 1), enrolled(1), disabled(true), disabled(true)],
            vec![created(1, 1), enrolled(1), disabled(true), disabled(false)],
            vec![IdentityEvent::EnrollmentTokenAnnulled {
                at: AT,
                token_id: token(1),
            }],
        ] {
            assert!(
                Identity::from_events(hermes(), &events).is_err(),
                "{events:?}"
            );
        }
    }

    #[test]
    fn the_identity_is_rebuilt_from_its_stored_events() {
        let events = vec![
            created(1, 1),
            enrolled(1),
            IdentityEvent::SuspiciousRenewalDetected {
                at: AT,
                serial: serial(1),
            },
            disabled(true),
        ];
        let stored: Vec<IdentityEvent> = events
            .iter()
            .map(|event| IdentityEvent::decode(&event.encode()).unwrap())
            .collect();
        let rebuilt = identity(&stored);
        assert_eq!(rebuilt, identity(&events));
        assert_eq!(rebuilt.suspicious_renewals, vec![AT]);
    }
}
