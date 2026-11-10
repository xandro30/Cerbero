//! The `Key` aggregate of the CA (stream `ca`, data-model.md): one CA, its generation, its root CA
//! and the state of each intermediate. `active` issues, `verify_only` only signs the CRL of its
//! certificates and `retired` has no key any more.

use std::collections::BTreeMap;

use time::OffsetDateTime;

use crate::events::{CaEvent, InvalidEvents, RetireReason};
use crate::identity::names::{RootFingerprint, TrustDomain};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntermediateState {
    Active,
    VerifyOnly,
    Retired,
}

impl IntermediateState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::VerifyOnly => "verify_only",
            Self::Retired => "retired",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Intermediate {
    pub generation: u64,
    pub version: u64,
    pub not_after: OffsetDateTime,
    pub state: IntermediateState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertificateAuthority {
    pub generation: u64,
    pub trust_domain: TrustDomain,
    pub root_fingerprint: RootFingerprint,
    pub root_not_after: OffsetDateTime,
    /// Every intermediate of every generation, by generation and version.
    pub intermediates: BTreeMap<(u64, u64), Intermediate>,
    pub stream_version: u64,
}

impl CertificateAuthority {
    /// The events of a new CA: its first generation with its first intermediate `active`.
    pub fn create(
        trust_domain: TrustDomain,
        root_fingerprint: RootFingerprint,
        root_not_after: OffsetDateTime,
        intermediate_not_after: OffsetDateTime,
        at: OffsetDateTime,
    ) -> Vec<CaEvent> {
        vec![CaEvent::CaCreated {
            at,
            generation: 1,
            trust_domain,
            root_fingerprint,
            root_not_after,
            intermediate_version: 1,
            intermediate_not_after,
        }]
    }

    /// The events of the replacement of the CA (FR-006): every intermediate that is not retired
    /// retires with `ca_replaced`, and the next generation starts with its first intermediate.
    pub fn replace(
        &self,
        root_fingerprint: RootFingerprint,
        root_not_after: OffsetDateTime,
        intermediate_not_after: OffsetDateTime,
        at: OffsetDateTime,
    ) -> Vec<CaEvent> {
        let mut events: Vec<CaEvent> = self
            .intermediates
            .values()
            .filter(|intermediate| intermediate.state != IntermediateState::Retired)
            .map(|intermediate| CaEvent::IntermediateRetired {
                at,
                generation: intermediate.generation,
                intermediate_version: intermediate.version,
                reason: RetireReason::CaReplaced,
            })
            .collect();
        events.push(CaEvent::CaReplaced {
            at,
            generation: self.generation + 1,
            root_fingerprint,
            root_not_after,
            intermediate_version: 1,
            intermediate_not_after,
        });
        events
    }

    /// The CA from its stream; there is only one CA, so `CaCreated` is only the first event.
    pub fn from_events(events: &[CaEvent]) -> Result<Self, InvalidEvents> {
        let Some(CaEvent::CaCreated {
            generation: 1,
            trust_domain,
            root_fingerprint,
            root_not_after,
            intermediate_version: 1,
            intermediate_not_after,
            ..
        }) = events.first()
        else {
            return Err(InvalidEvents(
                "the CA stream starts with CaCreated of generation 1 and intermediate 1",
            ));
        };
        let mut ca = Self {
            generation: 1,
            trust_domain: trust_domain.clone(),
            root_fingerprint: *root_fingerprint,
            root_not_after: *root_not_after,
            intermediates: BTreeMap::new(),
            stream_version: 1,
        };
        ca.add_active(1, *intermediate_not_after);
        for event in &events[1..] {
            ca.apply(event)?;
        }
        Ok(ca)
    }

    pub fn apply(&mut self, event: &CaEvent) -> Result<(), InvalidEvents> {
        match event {
            CaEvent::CaCreated { .. } => {
                return Err(InvalidEvents("CaCreated is only the first event of the CA"));
            }
            CaEvent::IntermediateRenewed {
                generation,
                intermediate_version,
                intermediate_not_after,
                ..
            } => {
                if *generation != self.generation || *intermediate_version != self.next_version() {
                    return Err(InvalidEvents(
                        "a renewed intermediate is the next version of the current generation",
                    ));
                }
                let active = self
                    .intermediates
                    .values_mut()
                    .find(|intermediate| intermediate.state == IntermediateState::Active)
                    .ok_or(InvalidEvents("only an active intermediate is renewed"))?;
                active.state = IntermediateState::VerifyOnly;
                self.add_active(*intermediate_version, *intermediate_not_after);
            }
            CaEvent::IntermediateRetired {
                generation,
                intermediate_version,
                reason,
                ..
            } => {
                let intermediate = self
                    .intermediates
                    .get_mut(&(*generation, *intermediate_version))
                    .ok_or(InvalidEvents(
                        "an event names an intermediate that does not exist",
                    ))?;
                // An active intermediate only retires with the replacement of its CA.
                match (intermediate.state, reason) {
                    (IntermediateState::VerifyOnly, _)
                    | (IntermediateState::Active, RetireReason::CaReplaced) => {
                        intermediate.state = IntermediateState::Retired;
                    }
                    _ => return Err(InvalidEvents("this intermediate cannot be retired")),
                }
            }
            CaEvent::CaReplaced {
                generation,
                root_fingerprint,
                root_not_after,
                intermediate_version,
                intermediate_not_after,
                ..
            } => {
                if *generation != self.generation + 1 || *intermediate_version != 1 {
                    return Err(InvalidEvents(
                        "a replaced CA starts the next generation with intermediate 1",
                    ));
                }
                if self
                    .intermediates
                    .values()
                    .any(|intermediate| intermediate.state != IntermediateState::Retired)
                {
                    return Err(InvalidEvents(
                        "the CA is replaced once all its intermediates are retired",
                    ));
                }
                self.generation = *generation;
                self.root_fingerprint = *root_fingerprint;
                self.root_not_after = *root_not_after;
                self.add_active(1, *intermediate_not_after);
            }
        }
        self.stream_version += 1;
        Ok(())
    }

    fn add_active(&mut self, version: u64, not_after: OffsetDateTime) {
        self.intermediates.insert(
            (self.generation, version),
            Intermediate {
                generation: self.generation,
                version,
                not_after,
                state: IntermediateState::Active,
            },
        );
    }

    /// The version that the next renewal of the intermediate signs.
    pub fn next_version(&self) -> u64 {
        self.intermediates
            .range((self.generation, 0)..=(self.generation, u64::MAX))
            .next_back()
            .map_or(1, |((_, version), _)| version + 1)
    }

    /// The intermediate that issues now; there is at most one.
    pub fn active(&self) -> Option<&Intermediate> {
        self.intermediates
            .values()
            .find(|intermediate| intermediate.state == IntermediateState::Active)
    }

    pub fn intermediate(&self, generation: u64, version: u64) -> Option<&Intermediate> {
        self.intermediates.get(&(generation, version))
    }
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::*;
    use crate::events::{CaEvent, RetireReason};
    use crate::identity::names::{RootFingerprint, TrustDomain};

    const AT: OffsetDateTime = datetime!(2026-11-09 12:00 UTC);

    fn created() -> Vec<CaEvent> {
        CertificateAuthority::create(
            TrustDomain::parse("cerbero.test").unwrap(),
            RootFingerprint::of_der(b"root"),
            datetime!(2036-11-09 12:00 UTC),
            datetime!(2027-11-09 12:00 UTC),
            AT,
        )
    }

    fn renewed(generation: u64, version: u64) -> CaEvent {
        CaEvent::IntermediateRenewed {
            at: AT,
            generation,
            intermediate_version: version,
            intermediate_not_after: datetime!(2028-11-09 12:00 UTC),
        }
    }

    fn retired(generation: u64, version: u64, reason: RetireReason) -> CaEvent {
        CaEvent::IntermediateRetired {
            at: AT,
            generation,
            intermediate_version: version,
            reason,
        }
    }

    fn replaced(generation: u64) -> CaEvent {
        CaEvent::CaReplaced {
            at: AT,
            generation,
            root_fingerprint: RootFingerprint::of_der(b"new root"),
            root_not_after: datetime!(2036-11-10 12:00 UTC),
            intermediate_version: 1,
            intermediate_not_after: datetime!(2027-11-10 12:00 UTC),
        }
    }

    fn state_of(ca: &CertificateAuthority, generation: u64, version: u64) -> IntermediateState {
        ca.intermediate(generation, version).unwrap().state
    }

    #[test]
    fn a_new_ca_has_its_first_generation_and_an_active_intermediate() {
        let ca = CertificateAuthority::from_events(&created()).unwrap();
        assert_eq!(
            (ca.generation, ca.trust_domain.as_str(), ca.stream_version),
            (1, "cerbero.test", 1)
        );
        assert_eq!(ca.root_fingerprint, RootFingerprint::of_der(b"root"));
        let active = ca.active().unwrap();
        assert_eq!(
            (active.generation, active.version, active.state),
            (1, 1, IntermediateState::Active)
        );
        assert_eq!(active.not_after, datetime!(2027-11-09 12:00 UTC));
    }

    #[test]
    fn ca_created_is_only_the_first_event() {
        assert!(CertificateAuthority::from_events(&[]).is_err());
        let mut twice = created();
        twice.extend(created());
        assert!(CertificateAuthority::from_events(&twice).is_err());
        assert!(CertificateAuthority::from_events(&[renewed(1, 2)]).is_err());
        assert!(CertificateAuthority::from_events(&[replaced(2)]).is_err());
    }

    #[test]
    fn renewing_leaves_the_previous_intermediate_verify_only_and_retiring_destroys_it() {
        let mut events = created();
        events.push(renewed(1, 2));
        let ca = CertificateAuthority::from_events(&events).unwrap();
        assert_eq!(state_of(&ca, 1, 1), IntermediateState::VerifyOnly);
        assert_eq!(state_of(&ca, 1, 2), IntermediateState::Active);
        assert_eq!(ca.active().unwrap().version, 2);
        events.push(retired(1, 1, RetireReason::ExpiredCertificates));
        let ca = CertificateAuthority::from_events(&events).unwrap();
        assert_eq!(state_of(&ca, 1, 1), IntermediateState::Retired);
        assert_eq!(ca.stream_version, 3);
        let mut retire_active = events.clone();
        retire_active.push(retired(1, 2, RetireReason::ExpiredCertificates));
        assert!(CertificateAuthority::from_events(&retire_active).is_err());
        for bad in [
            renewed(1, 4),
            renewed(2, 3),
            retired(1, 9, RetireReason::ExpiredCertificates),
        ] {
            let mut wrong = events.clone();
            wrong.push(bad.clone());
            assert!(
                CertificateAuthority::from_events(&wrong).is_err(),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn the_replacement_retires_every_intermediate_and_starts_the_next_generation() {
        let mut events = created();
        events.push(renewed(1, 2));
        events.push(retired(1, 1, RetireReason::ExpiredCertificates));
        events.push(renewed(1, 3));
        let ca = CertificateAuthority::from_events(&events).unwrap();
        let replacement = ca.replace(
            RootFingerprint::of_der(b"new root"),
            datetime!(2036-11-10 12:00 UTC),
            datetime!(2027-11-10 12:00 UTC),
            AT,
        );
        assert_eq!(
            replacement,
            vec![
                retired(1, 2, RetireReason::CaReplaced),
                retired(1, 3, RetireReason::CaReplaced),
                replaced(2),
            ]
        );
        events.extend(replacement);
        let replaced_ca = CertificateAuthority::from_events(&events).unwrap();
        assert_eq!(replaced_ca.generation, 2);
        assert_eq!(replaced_ca.active().unwrap().version, 1);
    }

    #[test]
    fn replacing_the_ca_starts_a_new_generation() {
        let mut events = created();
        events.push(renewed(1, 2));
        events.push(retired(1, 1, RetireReason::CaReplaced));
        events.push(retired(1, 2, RetireReason::CaReplaced));
        events.push(replaced(2));
        let ca = CertificateAuthority::from_events(&events).unwrap();
        assert_eq!(ca.generation, 2);
        assert_eq!(ca.root_fingerprint, RootFingerprint::of_der(b"new root"));
        assert_eq!(ca.trust_domain.as_str(), "cerbero.test");
        let active = ca.active().unwrap();
        assert_eq!((active.generation, active.version), (2, 1));
        assert_eq!(state_of(&ca, 1, 1), IntermediateState::Retired);
        assert_eq!(state_of(&ca, 1, 2), IntermediateState::Retired);
        let mut skipping = created();
        skipping.push(retired(1, 1, RetireReason::CaReplaced));
        skipping.push(replaced(3));
        assert!(CertificateAuthority::from_events(&skipping).is_err());
        let mut with_an_active = created();
        with_an_active.push(replaced(2));
        assert!(CertificateAuthority::from_events(&with_an_active).is_err());
    }

    #[test]
    fn the_ca_is_rebuilt_from_its_stored_events() {
        let mut events = created();
        events.push(renewed(1, 2));
        events.push(retired(1, 1, RetireReason::ExpiredCertificates));
        let stored: Vec<CaEvent> = events
            .iter()
            .map(|event| CaEvent::decode(&event.encode()).unwrap())
            .collect();
        assert_eq!(
            CertificateAuthority::from_events(&stored).unwrap(),
            CertificateAuthority::from_events(&events).unwrap()
        );
    }
}
