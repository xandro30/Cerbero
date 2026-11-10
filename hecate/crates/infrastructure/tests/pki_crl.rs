//! T086: the CRL of an intermediate (contracts/formats.md, research R4): X.509 v2 signed by the
//! intermediate that issued the certificates, with `thisUpdate`, `nextUpdate` 300 s later and the
//! `cRLNumber` given; its entries carry their reason, and webpki marks them revoked.

use std::sync::Arc;

use hecate_application::ports::{
    CertificateProfile, Issuance, Issuer, PkiProvider, RevokedCertificate,
};
use hecate_domain::events::RevocationReason;
use hecate_domain::identity::names::{IdentityName, Serial, SpiffeId, TrustDomain};
use hecate_domain::sensitive::SecretBytes;
use hecate_infrastructure::pki::{RcgenPki, RootCaKey};
use rustls::RootCertStore;
use rustls::pki_types::{CertificateDer, CertificateRevocationListDer, UnixTime};
use rustls::server::WebPkiClientVerifier;
use time::{Duration, OffsetDateTime};
use x509_parser::prelude::FromDer;
use x509_parser::revocation_list::CertificateRevocationList;
use x509_parser::x509::{ReasonCode, X509Version};

fn td() -> TrustDomain {
    TrustDomain::parse("cerbero.test").unwrap()
}

fn now() -> OffsetDateTime {
    OffsetDateTime::now_utc().replace_nanosecond(0).unwrap()
}

struct Ca {
    pki: RcgenPki,
    root: Vec<u8>,
    intermediate: Vec<u8>,
    key: SecretBytes,
}

impl Ca {
    fn new(now: OffsetDateTime) -> Self {
        let pki = RcgenPki::new();
        let root_key = RootCaKey::generate().unwrap();
        let root = root_key
            .root_certificate(&td(), &Serial::from_be_bytes(&[0x9f; 16]).unwrap(), now)
            .unwrap();
        let key = pki.generate_key().unwrap();
        let request = pki
            .create_request(
                &key,
                &CertificateProfile::Intermediate {
                    trust_domain: td(),
                    generation: 1,
                    version: 1,
                },
            )
            .unwrap();
        let intermediate = root_key
            .sign_intermediate(
                &root,
                &request,
                &Serial::from_be_bytes(&[0x9e; 16]).unwrap(),
                now,
            )
            .unwrap();
        Self {
            pki,
            root,
            intermediate,
            key,
        }
    }

    fn issuer(&self) -> Issuer<'_> {
        Issuer {
            certificate: &self.intermediate,
            key: &self.key,
        }
    }

    fn leaf(&self, serial: &Serial, now: OffsetDateTime) -> Vec<u8> {
        let key = self.pki.generate_key().unwrap();
        let profile = CertificateProfile::Leaf {
            spiffe_id: SpiffeId::new(td(), IdentityName::parse("hermes").unwrap()),
        };
        self.pki
            .issue(
                &self.issuer(),
                &Issuance {
                    profile: &profile,
                    public_key: &self.pki.public_key(&key).unwrap(),
                    serial,
                    not_before: now - Duration::minutes(1),
                    not_after: now + Duration::hours(72),
                },
            )
            .unwrap()
    }

    fn crl(&self, revoked: &[RevokedCertificate], now: OffsetDateTime, number: u64) -> Vec<u8> {
        self.pki
            .crl(
                &self.issuer(),
                revoked,
                now,
                now + Duration::seconds(300),
                number,
            )
            .unwrap()
    }
}

fn serial(byte: u8) -> Serial {
    Serial::from_be_bytes(&[byte; 16]).unwrap()
}

fn entries(now: OffsetDateTime) -> Vec<RevokedCertificate> {
    vec![
        RevokedCertificate {
            serial: serial(0x51),
            revoked_at: now - Duration::minutes(5),
            reason: RevocationReason::SuspiciousRenewal,
        },
        RevokedCertificate {
            serial: serial(0x52),
            revoked_at: now - Duration::minutes(2),
            reason: RevocationReason::Admin,
        },
    ]
}

#[test]
fn the_crl_is_x509_v2_signed_by_the_intermediate_with_its_times_and_number() {
    let now = now();
    let ca = Ca::new(now);
    let number = 1_762_689_600_123;
    let der = ca.crl(&entries(now), now, number);
    let (rest, crl) = CertificateRevocationList::from_der(&der).unwrap();
    assert!(rest.is_empty());
    assert_eq!(crl.version(), Some(X509Version::V2));
    assert_eq!(crl.last_update().to_datetime(), now);
    assert_eq!(
        crl.next_update().unwrap().to_datetime(),
        now + Duration::seconds(300)
    );
    assert_eq!(crl.crl_number().unwrap().to_string(), number.to_string());
    let (_, intermediate) =
        x509_parser::prelude::X509Certificate::from_der(&ca.intermediate).unwrap();
    assert!(crl.verify_signature(intermediate.public_key()).is_ok());
    assert_eq!(crl.issuer(), intermediate.subject());
}

/// RFC 5280 §5.3.1: `unspecified` is written as an entry without the reason code extension.
#[test]
fn each_entry_carries_its_date_and_its_reason() {
    let now = now();
    let ca = Ca::new(now);
    let der = ca.crl(&entries(now), now, 1);
    let (_, crl) = CertificateRevocationList::from_der(&der).unwrap();
    let read: Vec<(Serial, OffsetDateTime, Option<ReasonCode>)> = crl
        .iter_revoked_certificates()
        .map(|entry| {
            (
                Serial::from_be_bytes(entry.raw_serial()).unwrap(),
                entry.revocation_date.to_datetime(),
                entry.reason_code().map(|(_, code)| code),
            )
        })
        .collect();
    assert_eq!(
        read,
        vec![
            (
                serial(0x51),
                now - Duration::minutes(5),
                Some(ReasonCode::KeyCompromise)
            ),
            (serial(0x52), now - Duration::minutes(2), None),
        ]
    );
    let empty = ca.crl(&[], now, 2);
    let (_, empty) = CertificateRevocationList::from_der(&empty).unwrap();
    assert_eq!(empty.iter_revoked_certificates().count(), 0);
}

#[test]
fn webpki_takes_the_crl_and_refuses_the_revoked_serials() {
    let now = now();
    let ca = Ca::new(now);
    let revoked = ca.leaf(&serial(0x51), now);
    let valid = ca.leaf(&serial(0x60), now);
    let crl = ca.crl(&entries(now), now, 1);
    let mut roots = RootCertStore::empty();
    roots.add(CertificateDer::from(ca.root.clone())).unwrap();
    let verifier = WebPkiClientVerifier::builder_with_provider(
        Arc::new(roots),
        Arc::new(rustls::crypto::aws_lc_rs::default_provider()),
    )
    .with_crls([CertificateRevocationListDer::from(crl)])
    .only_check_end_entity_revocation()
    .enforce_revocation_expiration()
    .build()
    .unwrap();
    let at =
        UnixTime::since_unix_epoch(std::time::Duration::from_secs(now.unix_timestamp() as u64));
    let intermediates = [CertificateDer::from(ca.intermediate.clone())];
    assert!(
        verifier
            .verify_client_cert(&CertificateDer::from(valid), &intermediates, at)
            .is_ok()
    );
    let refused = verifier
        .verify_client_cert(&CertificateDer::from(revoked), &intermediates, at)
        .err()
        .unwrap();
    assert!(format!("{refused:?}").contains("Revoked"), "{refused:?}");
}
