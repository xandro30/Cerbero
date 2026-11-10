//! T028: the PKI of the CA with the profiles of contracts/formats.md: the root CA and the
//! intermediate that the CLI signs, Hécate's own certificate, the request of the intermediate and
//! the fingerprint.

use std::sync::Arc;

use aws_lc_rs::digest::{SHA256, digest};
use data_encoding::HEXLOWER;
use hecate_application::ports::{CertificateProfile, Issuance, Issuer, PkiProvider};
use hecate_domain::identity::names::{IdentityName, Serial, SpiffeId, TrustDomain};
use hecate_domain::pem;
use hecate_domain::sensitive::SecretBytes;
use hecate_infrastructure::pki::{RcgenPki, RootCaKey};
use rustls::RootCertStore;
use rustls::pki_types::{CertificateDer, UnixTime};
use rustls::server::WebPkiClientVerifier;
use time::{Duration, OffsetDateTime};
use x509_parser::extensions::GeneralName;
use x509_parser::oid_registry::{
    OID_EC_P256, OID_KEY_TYPE_EC_PUBLIC_KEY, OID_SIG_ECDSA_WITH_SHA256,
};
use x509_parser::prelude::{FromDer, X509Certificate};

/// keyCertSign and cRLSign, and nothing else.
const CA_USAGE: u16 = 0x60;
/// digitalSignature, and nothing else.
const LEAF_USAGE: u16 = 0x01;

fn td() -> TrustDomain {
    TrustDomain::parse("cerbero.test").unwrap()
}

fn intermediate_profile(version: u64) -> CertificateProfile {
    CertificateProfile::Intermediate {
        trust_domain: td(),
        generation: 1,
        version,
    }
}

fn hecate_profile() -> CertificateProfile {
    CertificateProfile::Leaf {
        spiffe_id: SpiffeId::new(td(), IdentityName::hecate()),
    }
}

/// Whole seconds, as certificates keep them.
fn now() -> OffsetDateTime {
    OffsetDateTime::now_utc().replace_nanosecond(0).unwrap()
}

/// 128 bits whose first bit is set, so the DER integer needs a leading zero to stay positive.
fn serial(byte: u8) -> Serial {
    Serial::from_be_bytes(&[byte; 16]).unwrap()
}

fn parsed(der: &[u8]) -> X509Certificate<'_> {
    let (rest, certificate) = X509Certificate::from_der(der).unwrap();
    assert!(rest.is_empty());
    certificate
}

struct Chain {
    pki: RcgenPki,
    now: OffsetDateTime,
    root_key: RootCaKey,
    root: Vec<u8>,
    intermediate_key: SecretBytes,
    intermediate: Vec<u8>,
}

impl Chain {
    fn new() -> Self {
        let pki = RcgenPki::new();
        let now = now();
        let root_key = RootCaKey::generate().unwrap();
        let root = root_key
            .root_certificate(&td(), &serial(0x9f), now)
            .unwrap();
        let intermediate_key = pki.generate_key().unwrap();
        let request = pki
            .create_request(&intermediate_key, &intermediate_profile(1))
            .unwrap();
        let intermediate = root_key
            .sign_intermediate(&root, &request, &serial(0x9e), now)
            .unwrap();
        Self {
            pki,
            now,
            root_key,
            root,
            intermediate_key,
            intermediate,
        }
    }

    fn hecate(&self, not_before: OffsetDateTime, not_after: OffsetDateTime) -> Vec<u8> {
        let key = self.pki.generate_key().unwrap();
        self.issue_leaf(&key, not_before, not_after).unwrap()
    }

    fn issue_leaf(
        &self,
        key: &SecretBytes,
        not_before: OffsetDateTime,
        not_after: OffsetDateTime,
    ) -> Result<Vec<u8>, hecate_application::ports::PkiError> {
        let public_key = self.pki.public_key(key).unwrap();
        self.pki.issue(
            &Issuer {
                certificate: &self.intermediate,
                key: &self.intermediate_key,
            },
            &Issuance {
                profile: &hecate_profile(),
                public_key: &public_key,
                serial: &serial(0xa5),
                not_before,
                not_after,
            },
        )
    }
}

fn assert_p256_with_a_positive_128_bit_serial(certificate: &X509Certificate<'_>) {
    assert_eq!(
        certificate.signature_algorithm.algorithm,
        OID_SIG_ECDSA_WITH_SHA256
    );
    let key = certificate.public_key();
    assert_eq!(key.algorithm.algorithm, OID_KEY_TYPE_EC_PUBLIC_KEY);
    let curve = key.algorithm.parameters.as_ref().unwrap().as_oid().unwrap();
    assert_eq!(curve, OID_EC_P256);
    let raw = certificate.raw_serial();
    assert!(raw[0] < 0x80, "the serial is positive: {raw:02x?}");
    assert_eq!(raw.iter().skip_while(|byte| **byte == 0).count(), 16);
}

fn assert_ca_extensions(certificate: &X509Certificate<'_>, path_len: u32) {
    let constraints = certificate.basic_constraints().unwrap().unwrap();
    assert!(constraints.critical);
    assert!(constraints.value.ca);
    assert_eq!(constraints.value.path_len_constraint, Some(path_len));
    let usage = certificate.key_usage().unwrap().unwrap();
    assert!(usage.critical);
    assert_eq!(usage.value.flags, CA_USAGE);
    assert!(certificate.extended_key_usage().unwrap().is_none());
    assert!(certificate.subject_alternative_name().unwrap().is_none());
}

#[test]
fn the_root_ca_is_self_signed_with_path_length_1_for_ten_years() {
    let chain = Chain::new();
    let root = parsed(&chain.root);
    assert_eq!(
        root.subject().to_string(),
        "CN=Cerbero Root CA cerbero.test"
    );
    assert_eq!(root.issuer(), root.subject());
    assert!(root.verify_signature(None).is_ok());
    assert_ca_extensions(&root, 1);
    assert_eq!(root.validity().not_before.to_datetime(), chain.now);
    assert_eq!(
        root.validity().not_after.to_datetime(),
        chain.now + Duration::days(3650)
    );
    assert_p256_with_a_positive_128_bit_serial(&root);
    assert_eq!(
        chain.root_key.public_key().unwrap(),
        root.public_key().raw.to_vec()
    );
}

#[test]
fn the_intermediate_is_signed_by_the_root_with_path_length_0_for_a_year() {
    let chain = Chain::new();
    let (root, intermediate) = (parsed(&chain.root), parsed(&chain.intermediate));
    assert_eq!(
        intermediate.subject().to_string(),
        "CN=Cerbero Intermediate CA cerbero.test 1.1"
    );
    assert_eq!(intermediate.issuer(), root.subject());
    assert!(
        intermediate
            .verify_signature(Some(root.public_key()))
            .is_ok()
    );
    assert_ca_extensions(&intermediate, 0);
    assert_eq!(intermediate.validity().not_before.to_datetime(), chain.now);
    assert_eq!(
        intermediate.validity().not_after.to_datetime(),
        chain.now + Duration::days(365)
    );
    assert_p256_with_a_positive_128_bit_serial(&intermediate);
    assert_eq!(
        intermediate.public_key().raw.to_vec(),
        chain.pki.public_key(&chain.intermediate_key).unwrap()
    );
}

#[test]
fn the_intermediate_never_outlives_the_root() {
    let chain = Chain::new();
    let old_root = chain
        .root_key
        .root_certificate(&td(), &serial(0x9f), chain.now - Duration::days(3650 - 100))
        .unwrap();
    let request = chain
        .pki
        .create_request(&chain.intermediate_key, &intermediate_profile(2))
        .unwrap();
    let intermediate = chain
        .root_key
        .sign_intermediate(&old_root, &request, &serial(0x9e), chain.now)
        .unwrap();
    assert_eq!(
        parsed(&intermediate).validity().not_after.to_datetime(),
        parsed(&old_root).validity().not_after.to_datetime()
    );
    assert_eq!(
        parsed(&intermediate).subject().to_string(),
        "CN=Cerbero Intermediate CA cerbero.test 1.2"
    );
}

#[test]
fn the_cli_only_signs_requests_for_an_intermediate_of_its_root() {
    let chain = Chain::new();
    let leaf_request = chain
        .pki
        .create_request(&chain.intermediate_key, &hecate_profile())
        .unwrap();
    let other_domain = chain
        .pki
        .create_request(
            &chain.intermediate_key,
            &CertificateProfile::Intermediate {
                trust_domain: TrustDomain::parse("other.test").unwrap(),
                generation: 1,
                version: 1,
            },
        )
        .unwrap();
    for request in [leaf_request, other_domain, "not a request".to_string()] {
        assert!(
            chain
                .root_key
                .sign_intermediate(&chain.root, &request, &serial(0x9e), chain.now)
                .is_err(),
            "{request}"
        );
    }
}

#[test]
fn hecate_certificate_names_only_its_spiffe_id() {
    let chain = Chain::new();
    let not_before = chain.now - Duration::minutes(1);
    let not_after = chain.now + Duration::hours(72);
    let der = chain.hecate(not_before, not_after);
    let (intermediate, certificate) = (parsed(&chain.intermediate), parsed(&der));
    assert_eq!(certificate.subject().to_string(), "CN=hecate");
    assert_eq!(certificate.issuer(), intermediate.subject());
    assert!(
        certificate
            .verify_signature(Some(intermediate.public_key()))
            .is_ok()
    );
    let names = certificate.subject_alternative_name().unwrap().unwrap();
    assert_eq!(
        names.value.general_names,
        vec![GeneralName::URI("spiffe://cerbero.test/hecate")]
    );
    let constraints = certificate.basic_constraints().unwrap().unwrap();
    assert!(constraints.critical && !constraints.value.ca);
    assert_eq!(constraints.value.path_len_constraint, None);
    let usage = certificate.key_usage().unwrap().unwrap();
    assert!(usage.critical);
    assert_eq!(usage.value.flags, LEAF_USAGE);
    let extended = certificate.extended_key_usage().unwrap().unwrap().value;
    assert!(extended.server_auth && extended.client_auth);
    assert!(!(extended.any || extended.code_signing || extended.email_protection));
    assert!(!(extended.time_stamping || extended.ocsp_signing) && extended.other.is_empty());
    assert_eq!(certificate.validity().not_before.to_datetime(), not_before);
    assert_eq!(certificate.validity().not_after.to_datetime(), not_after);
    assert_p256_with_a_positive_128_bit_serial(&certificate);
}

#[test]
fn no_certificate_outlives_its_issuer() {
    let chain = Chain::new();
    let key = chain.pki.generate_key().unwrap();
    let intermediate_end = parsed(&chain.intermediate)
        .validity()
        .not_after
        .to_datetime();
    assert!(chain.issue_leaf(&key, chain.now, intermediate_end).is_ok());
    assert!(
        chain
            .issue_leaf(&key, chain.now, intermediate_end + Duration::seconds(1))
            .is_err()
    );
}

#[test]
fn the_chain_validates_with_webpki() {
    let chain = Chain::new();
    let leaf = chain.hecate(
        chain.now - Duration::minutes(1),
        chain.now + Duration::hours(72),
    );
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let mut roots = RootCertStore::empty();
    roots.add(CertificateDer::from(chain.root.clone())).unwrap();
    let verifier = WebPkiClientVerifier::builder_with_provider(Arc::new(roots), provider)
        .build()
        .unwrap();
    verifier
        .verify_client_cert(
            &CertificateDer::from(leaf),
            &[CertificateDer::from(chain.intermediate.clone())],
            UnixTime::now(),
        )
        .unwrap();
}

#[test]
fn the_fingerprint_is_the_sha256_of_the_root_der() {
    let chain = Chain::new();
    let expected = HEXLOWER.encode(digest(&SHA256, &chain.root).as_ref());
    assert_eq!(
        chain.pki.fingerprint(&chain.root).to_string(),
        format!("sha256:{expected}")
    );
}

#[test]
fn the_request_of_the_intermediate_verifies() {
    let chain = Chain::new();
    let text = chain
        .pki
        .create_request(&chain.intermediate_key, &intermediate_profile(1))
        .unwrap();
    assert!(
        text.starts_with("-----BEGIN CERTIFICATE REQUEST-----\n"),
        "{text}"
    );
    let request = chain.pki.read_request(&text).unwrap();
    assert_eq!(
        request.public_key,
        chain.pki.public_key(&chain.intermediate_key).unwrap()
    );
    assert_eq!(
        request.common_name.as_deref(),
        Some("Cerbero Intermediate CA cerbero.test 1.1")
    );
    assert_eq!(request.requested_uri, None);
    let mut der = pem::single(&text, "CERTIFICATE REQUEST").unwrap();
    let last = der.len() - 1;
    der[last] ^= 1;
    assert!(
        chain
            .pki
            .read_request(&pem::encode("CERTIFICATE REQUEST", &der))
            .is_err()
    );
    let leaf = chain
        .pki
        .create_request(&chain.intermediate_key, &hecate_profile())
        .unwrap();
    assert_eq!(
        chain
            .pki
            .read_request(&leaf)
            .unwrap()
            .requested_uri
            .as_deref(),
        Some("spiffe://cerbero.test/hecate")
    );
}

#[test]
fn read_certificate_gives_what_hecate_reads() {
    let chain = Chain::new();
    let leaf = chain.hecate(
        chain.now - Duration::minutes(1),
        chain.now + Duration::hours(72),
    );
    let info = chain.pki.read_certificate(&leaf).unwrap();
    assert_eq!(info.serial, serial(0xa5));
    assert_eq!(info.subject, "CN=hecate");
    assert_eq!(info.issuer, "CN=Cerbero Intermediate CA cerbero.test 1.1");
    assert_eq!(
        info.spiffe_id.map(|id| id.to_string()).as_deref(),
        Some("spiffe://cerbero.test/hecate")
    );
    assert_eq!(info.not_before, chain.now - Duration::minutes(1));
    assert_eq!(info.not_after, chain.now + Duration::hours(72));
    assert!(!info.is_ca);
    assert_eq!(info.public_key, parsed(&leaf).public_key().raw.to_vec());
    let root = chain.pki.read_certificate(&chain.root).unwrap();
    assert!(root.is_ca && root.spiffe_id.is_none());
    assert_eq!(root.serial, serial(0x9f));
}

#[test]
fn is_signed_by_follows_the_chain() {
    let chain = Chain::new();
    let leaf = chain.hecate(
        chain.now - Duration::minutes(1),
        chain.now + Duration::hours(72),
    );
    let pki = &chain.pki;
    assert!(pki.is_signed_by(&chain.root, &chain.root).unwrap());
    assert!(pki.is_signed_by(&chain.intermediate, &chain.root).unwrap());
    assert!(pki.is_signed_by(&leaf, &chain.intermediate).unwrap());
    assert!(!pki.is_signed_by(&chain.root, &chain.intermediate).unwrap());
    assert!(!pki.is_signed_by(&leaf, &chain.root).unwrap());
    assert!(
        !pki.is_signed_by(&chain.intermediate, &chain.intermediate)
            .unwrap()
    );
}

#[test]
fn follows_profile_checks_names_extensions_and_algorithms() {
    let chain = Chain::new();
    let leaf = chain.hecate(
        chain.now - Duration::minutes(1),
        chain.now + Duration::hours(72),
    );
    let pki = &chain.pki;
    let root_profile = CertificateProfile::Root { trust_domain: td() };
    assert!(pki.follows_profile(&chain.root, &root_profile).unwrap());
    assert!(
        pki.follows_profile(&chain.intermediate, &intermediate_profile(1))
            .unwrap()
    );
    assert!(pki.follows_profile(&leaf, &hecate_profile()).unwrap());
    assert!(
        !pki.follows_profile(&chain.root, &intermediate_profile(1))
            .unwrap()
    );
    assert!(
        !pki.follows_profile(&chain.intermediate, &intermediate_profile(2))
            .unwrap()
    );
    assert!(
        !pki.follows_profile(&chain.intermediate, &root_profile)
            .unwrap()
    );
    let other = CertificateProfile::Leaf {
        spiffe_id: SpiffeId::new(td(), IdentityName::parse("hermes").unwrap()),
    };
    assert!(!pki.follows_profile(&leaf, &other).unwrap());
    let foreign = rcgen::generate_simple_self_signed(vec!["cerbero.test".to_string()]).unwrap();
    assert!(
        !pki.follows_profile(foreign.cert.der(), &root_profile)
            .unwrap()
    );
}

#[test]
fn the_root_key_comes_back_from_its_pkcs8() {
    let chain = Chain::new();
    let copy = SecretBytes::new(chain.root_key.pkcs8().expose().to_vec());
    let restored = RootCaKey::from_pkcs8(copy).unwrap();
    assert_eq!(
        restored.public_key().unwrap(),
        chain.root_key.public_key().unwrap()
    );
    assert!(RootCaKey::from_pkcs8(SecretBytes::new(b"not a key".to_vec())).is_err());
}
