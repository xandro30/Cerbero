//! The Certificate aggregate (stream `certificate/<identity>/<serial>`, data-model.md): its
//! issuance, its single renewal and its revocation. A passive revocation only removes the renewal;
//! an urgent one makes it `revoked`. One of another generation of the CA is valid for nothing.

use time::OffsetDateTime;

use crate::events::{
    CertificateEvent, CertificateOrigin, InvalidEvents, IssuedCertificate, RevocationReason,
    RevokedBy,
};
use crate::identity::names::Serial;
use crate::sealing::errors::HecateError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertificateState {
    Valid,
    Revoked,
    Expired,
}

impl CertificateState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::Revoked => "revoked",
            Self::Expired => "expired",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Revocation {
    pub at: OffsetDateTime,
    pub urgent: bool,
    pub by: RevokedBy,
    pub reason: RevocationReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Certificate {
    pub issued: IssuedCertificate,
    pub renewed_to: Option<Serial>,
    /// The latest revocation: an urgent one after a passive one replaces it.
    pub revocation: Option<Revocation>,
    pub stream_version: u64,
}

impl Certificate {
    /// The certificate from its stream, which starts with its only issuance.
    pub fn from_events(events: &[CertificateEvent]) -> Result<Self, InvalidEvents> {
        let Some(CertificateEvent::CertificateIssued(issued)) = events.first() else {
            return Err(InvalidEvents(
                "a certificate stream starts with CertificateIssued",
            ));
        };
        let mut certificate = Self {
            issued: (**issued).clone(),
            renewed_to: None,
            revocation: None,
            stream_version: 1,
        };
        for event in &events[1..] {
            certificate.apply(event)?;
        }
        Ok(certificate)
    }

    pub fn apply(&mut self, event: &CertificateEvent) -> Result<(), InvalidEvents> {
        match event {
            CertificateEvent::CertificateIssued(_) => {
                return Err(InvalidEvents(
                    "CertificateIssued is only the first event of a certificate",
                ));
            }
            CertificateEvent::CertificateRenewed { new_serial, .. } => {
                if self.renewed_to.is_some() {
                    return Err(InvalidEvents("a certificate is renewed only once"));
                }
                self.renewed_to = Some(new_serial.clone());
            }
            CertificateEvent::CertificateRevoked {
                at,
                urgent,
                by,
                reason,
            } => {
                // Only urgency changes a revocation that already exists.
                if self
                    .revocation
                    .as_ref()
                    .is_some_and(|current| current.urgent || !urgent)
                {
                    return Err(InvalidEvents("a revocation that changes nothing"));
                }
                self.revocation = Some(Revocation {
                    at: *at,
                    urgent: *urgent,
                    by: *by,
                    reason: *reason,
                });
            }
        }
        self.stream_version += 1;
        Ok(())
    }

    pub fn serial(&self) -> &Serial {
        &self.issued.serial
    }

    pub fn state(&self, now: OffsetDateTime) -> CertificateState {
        if self
            .revocation
            .as_ref()
            .is_some_and(|revocation| revocation.urgent)
        {
            CertificateState::Revoked
        } else if now >= self.issued.not_after {
            CertificateState::Expired
        } else {
            CertificateState::Valid
        }
    }

    /// `valid` and of the current generation of the CA (FR-021).
    pub fn is_valid(&self, now: OffsetDateTime, generation: u64) -> bool {
        self.state(now) == CertificateState::Valid && self.issued.generation == generation
    }

    /// Valid, never renewed nor revoked, of the epoch of its identity and with the identity
    /// enabled: a new enrollment or a disable leave the older ones without renewal (FR-013).
    pub fn renewable(
        &self,
        now: OffsetDateTime,
        generation: u64,
        identity_epoch: u64,
        identity_enabled: bool,
    ) -> bool {
        self.is_valid(now, generation)
            && self.renewed_to.is_none()
            && self.revocation.is_none()
            && self.issued.epoch == identity_epoch
            && identity_enabled
    }

    /// Whether its call key signs new calls (FR-017); after that it only verifies past ones.
    pub fn call_key_active(
        &self,
        now: OffsetDateTime,
        generation: u64,
        identity_enabled: bool,
    ) -> bool {
        self.issued.call_key.is_some() && self.is_valid(now, generation) && identity_enabled
    }

    /// Hécate's own certificate.
    pub fn is_own(&self) -> bool {
        self.issued.origin == CertificateOrigin::OwnCertificate
    }

    /// The revocation, or nothing when it changes nothing: an expired certificate, an urgent
    /// revocation already there, or a passive one again (FR-026). Hécate's own certificate only
    /// admits the urgent one.
    pub fn revoke(
        &self,
        urgent: bool,
        by: RevokedBy,
        reason: RevocationReason,
        at: OffsetDateTime,
    ) -> Result<Option<CertificateEvent>, HecateError> {
        if self.is_own() && !urgent {
            return Err(HecateError::invalid(
                "Hécate's certificate only admits an urgent revocation",
            ));
        }
        let unchanged = self.state(at) == CertificateState::Expired
            || self
                .revocation
                .as_ref()
                .is_some_and(|current| current.urgent || !urgent);
        Ok(
            (!unchanged).then_some(CertificateEvent::CertificateRevoked {
                at,
                urgent,
                by,
                reason,
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use time::Duration;
    use time::macros::datetime;

    use super::*;
    use crate::events::{
        CertificateEvent, CertificateOrigin, IssuedCertificate, RevocationReason, RevokedBy,
    };
    use crate::identity::call_key::CallKey;
    use crate::identity::names::{IdentityName, Serial};

    const AT: OffsetDateTime = datetime!(2026-11-09 12:00 UTC);

    fn issued(origin: CertificateOrigin) -> CertificateEvent {
        CertificateEvent::CertificateIssued(Box::new(IssuedCertificate {
            at: AT,
            identity: IdentityName::parse("hermes").unwrap(),
            serial: Serial::from_be_bytes(&[7; 16]).unwrap(),
            generation: 1,
            intermediate_version: 1,
            epoch: 2,
            not_before: AT - Duration::minutes(1),
            not_after: AT + Duration::hours(72),
            call_key: (origin != CertificateOrigin::OwnCertificate)
                .then(|| CallKey::from_bytes(&[9; 32]).unwrap()),
            origin,
            renewed_from: None,
            request_sha256: None,
            idempotency_key: None,
            certificate: vec![1, 2, 3],
        }))
    }

    fn revoked(urgent: bool) -> CertificateEvent {
        CertificateEvent::CertificateRevoked {
            at: AT,
            urgent,
            by: RevokedBy::Admin,
            reason: RevocationReason::Admin,
        }
    }

    fn renewed() -> CertificateEvent {
        CertificateEvent::CertificateRenewed {
            at: AT,
            new_serial: Serial::from_be_bytes(&[8; 16]).unwrap(),
        }
    }

    fn certificate(events: &[CertificateEvent]) -> Certificate {
        Certificate::from_events(events).unwrap()
    }

    fn component() -> Certificate {
        certificate(&[issued(CertificateOrigin::Enrollment)])
    }

    #[test]
    fn a_certificate_is_valid_until_it_expires_or_is_revoked_urgently() {
        let valid = component();
        assert_eq!(valid.state(AT), CertificateState::Valid);
        assert_eq!(
            valid.state(AT + Duration::hours(72)),
            CertificateState::Expired
        );
        let passive = certificate(&[issued(CertificateOrigin::Enrollment), revoked(false)]);
        assert_eq!(passive.state(AT), CertificateState::Valid);
        let urgent = certificate(&[issued(CertificateOrigin::Enrollment), revoked(true)]);
        assert_eq!(urgent.state(AT), CertificateState::Revoked);
        assert!(valid.is_valid(AT, 1));
        assert!(
            !valid.is_valid(AT, 2),
            "one of another generation is not valid"
        );
    }

    #[test]
    fn it_renews_only_valid_untouched_and_of_the_identity_epoch_while_enabled() {
        let fresh = component();
        assert!(fresh.renewable(AT, 1, 2, true));
        assert!(!fresh.renewable(AT, 2, 2, true));
        assert!(!fresh.renewable(AT, 1, 3, true));
        assert!(!fresh.renewable(AT, 1, 2, false));
        assert!(!fresh.renewable(AT + Duration::hours(72), 1, 2, true));
        for touched in [renewed(), revoked(false), revoked(true)] {
            let events = [issued(CertificateOrigin::Enrollment), touched];
            assert!(!certificate(&events).renewable(AT, 1, 2, true));
        }
    }

    #[test]
    fn its_call_key_signs_calls_while_it_is_valid_and_the_identity_enabled() {
        let fresh = component();
        assert!(fresh.call_key_active(AT, 1, true));
        assert!(!fresh.call_key_active(AT, 1, false));
        assert!(!fresh.call_key_active(AT + Duration::hours(72), 1, true));
        let urgent = certificate(&[issued(CertificateOrigin::Enrollment), revoked(true)]);
        assert!(!urgent.call_key_active(AT, 1, true));
        let renewed = certificate(&[issued(CertificateOrigin::Enrollment), renewed()]);
        assert!(renewed.call_key_active(AT, 1, true));
    }

    #[test]
    fn hecate_certificates_only_admit_urgent_revocation() {
        let own = certificate(&[issued(CertificateOrigin::OwnCertificate)]);
        let refused = own
            .revoke(false, RevokedBy::Admin, RevocationReason::Admin, AT)
            .unwrap_err();
        assert!(
            matches!(refused, HecateError::InvalidInput(_)),
            "{refused:?}"
        );
        assert_eq!(
            own.revoke(true, RevokedBy::Admin, RevocationReason::Admin, AT)
                .unwrap(),
            Some(revoked(true))
        );
    }

    #[test]
    fn a_revocation_that_changes_nothing_has_no_event_and_urgency_is_a_change() {
        let passive = certificate(&[issued(CertificateOrigin::Enrollment), revoked(false)]);
        let urgent = certificate(&[issued(CertificateOrigin::Enrollment), revoked(true)]);
        let by = (RevokedBy::Admin, RevocationReason::Admin);
        assert_eq!(passive.revoke(false, by.0, by.1, AT).unwrap(), None);
        assert_eq!(
            passive.revoke(true, by.0, by.1, AT).unwrap(),
            Some(revoked(true))
        );
        assert_eq!(urgent.revoke(true, by.0, by.1, AT).unwrap(), None);
        assert_eq!(urgent.revoke(false, by.0, by.1, AT).unwrap(), None);
        let expired = AT + Duration::hours(72);
        assert_eq!(component().revoke(true, by.0, by.1, expired).unwrap(), None);
        assert_eq!(
            component().revoke(false, by.0, by.1, AT).unwrap(),
            Some(revoked(false))
        );
    }

    #[test]
    fn streams_that_break_the_rules_are_refused() {
        let first = issued(CertificateOrigin::Enrollment);
        for events in [
            vec![],
            vec![renewed()],
            vec![first.clone(), first.clone()],
            vec![first.clone(), renewed(), renewed()],
            vec![first.clone(), revoked(true), revoked(true)],
            vec![first.clone(), revoked(false), revoked(false)],
            vec![first.clone(), revoked(true), revoked(false)],
        ] {
            assert!(Certificate::from_events(&events).is_err(), "{events:?}");
        }
        let stored: Vec<CertificateEvent> = [first, renewed(), revoked(false), revoked(true)]
            .iter()
            .map(|event| CertificateEvent::decode(&event.encode()).unwrap())
            .collect();
        let rebuilt = certificate(&stored);
        assert_eq!(rebuilt.stream_version, 4);
        assert!(
            rebuilt
                .revocation
                .as_ref()
                .is_some_and(|revocation| revocation.urgent)
        );
    }
}
