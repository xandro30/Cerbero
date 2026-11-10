//! T050: the PKI of the components (contracts/formats.md): the request in strict PEM with a valid
//! signature and a P-256 key, the SAN absent or a single URI, the profile of a component and the
//! call key, an Ed25519 `PUBLIC KEY`.

use hecate_application::ports::{CertificateProfile, Issuance, Issuer, PkiProvider};
use hecate_domain::identity::call_key::CallKey;
use hecate_domain::identity::names::{IdentityName, Serial, SpiffeId, TrustDomain};
use hecate_domain::pem;
use hecate_domain::sealing::errors::HecateError;
use hecate_infrastructure::pki::{RcgenPki, RootCaKey};
use rcgen::{
    CertificateParams, KeyPair, PKCS_ECDSA_P256_SHA256, PKCS_ECDSA_P384_SHA384, PKCS_ED25519,
    PublicKeyData, SanType,
};
use time::{Duration, OffsetDateTime};

const SPIFFE: &str = "spiffe://cerbero.test/hermes";

fn td() -> TrustDomain {
    TrustDomain::parse("cerbero.test").unwrap()
}

fn now() -> OffsetDateTime {
    OffsetDateTime::now_utc().replace_nanosecond(0).unwrap()
}

fn request(key: &KeyPair, names: Vec<SanType>) -> String {
    let mut params = CertificateParams::default();
    params.subject_alt_names = names;
    pem::encode(
        "CERTIFICATE REQUEST",
        params.serialize_request(key).unwrap().der(),
    )
}

fn uri(text: &str) -> SanType {
    SanType::URI(text.to_string().try_into().unwrap())
}

fn is_validation(error: &HecateError) -> bool {
    matches!(error, HecateError::InvalidInput(_))
}

#[test]
fn a_request_needs_a_p256_key_and_a_valid_signature() {
    let pki = RcgenPki::new();
    let p256 = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).unwrap();
    let read = pki.read_request(&request(&p256, vec![])).unwrap();
    assert_eq!(read.public_key, p256.subject_public_key_info());
    for other in [&PKCS_ECDSA_P384_SHA384, &PKCS_ED25519] {
        let key = KeyPair::generate_for(other).unwrap();
        let error = pki.read_request(&request(&key, vec![])).unwrap_err();
        assert!(is_validation(&error), "{error:?}");
    }
    let two_blocks = format!("{}{}", request(&p256, vec![]), request(&p256, vec![]));
    assert!(is_validation(&pki.read_request(&two_blocks).unwrap_err()));
    let wrong_label = pem::encode("CERTIFICATE", b"not a request");
    assert!(is_validation(&pki.read_request(&wrong_label).unwrap_err()));
}

#[test]
fn the_san_of_a_request_is_absent_or_a_single_uri() {
    let pki = RcgenPki::new();
    let key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).unwrap();
    assert_eq!(
        pki.read_request(&request(&key, vec![]))
            .unwrap()
            .requested_uri,
        None
    );
    assert_eq!(
        pki.read_request(&request(&key, vec![uri(SPIFFE)]))
            .unwrap()
            .requested_uri
            .as_deref(),
        Some(SPIFFE)
    );
    for names in [
        vec![SanType::DnsName("hermes.test".try_into().unwrap())],
        vec![uri(SPIFFE), uri("spiffe://cerbero.test/hades")],
        vec![
            uri(SPIFFE),
            SanType::DnsName("hermes.test".try_into().unwrap()),
        ],
    ] {
        let error = pki.read_request(&request(&key, names)).unwrap_err();
        assert!(is_validation(&error), "{error:?}");
    }
}

#[test]
fn a_component_certificate_follows_its_profile_with_the_given_lifetime() {
    let pki = RcgenPki::new();
    let now = now();
    let root_key = RootCaKey::generate().unwrap();
    let root = root_key
        .root_certificate(&td(), &Serial::from_be_bytes(&[0x9f; 16]).unwrap(), now)
        .unwrap();
    let intermediate_key = pki.generate_key().unwrap();
    let csr = pki
        .create_request(
            &intermediate_key,
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
            &csr,
            &Serial::from_be_bytes(&[0x9e; 16]).unwrap(),
            now,
        )
        .unwrap();
    let component = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).unwrap();
    let read = pki.read_request(&request(&component, vec![])).unwrap();
    let hermes = SpiffeId::new(td(), IdentityName::parse("hermes").unwrap());
    let profile = CertificateProfile::Leaf {
        spiffe_id: hermes.clone(),
    };
    let not_before = now - Duration::minutes(1);
    let not_after = now + Duration::hours(72);
    let der = pki
        .issue(
            &Issuer {
                certificate: &intermediate,
                key: &intermediate_key,
            },
            &Issuance {
                profile: &profile,
                public_key: &read.public_key,
                serial: &Serial::from_be_bytes(&[0x55; 16]).unwrap(),
                not_before,
                not_after,
            },
        )
        .unwrap();
    assert!(pki.follows_profile(&der, &profile).unwrap());
    assert!(pki.is_signed_by(&der, &intermediate).unwrap());
    let info = pki.read_certificate(&der).unwrap();
    assert_eq!(info.subject, "CN=hermes");
    assert_eq!(info.spiffe_id, Some(hermes));
    assert_eq!((info.not_before, info.not_after), (not_before, not_after));
    assert_eq!(info.public_key, component.subject_public_key_info());
}

#[test]
fn the_call_key_is_an_ed25519_public_key() {
    let pki = RcgenPki::new();
    let ed25519 = KeyPair::generate_for(&PKCS_ED25519).unwrap();
    let text = pem::encode("PUBLIC KEY", &ed25519.subject_public_key_info());
    let key = pki.read_call_key(&text).unwrap();
    assert_eq!(key, CallKey::from_bytes(ed25519.public_key_raw()).unwrap());
    let p256 = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).unwrap();
    for bad in [
        pem::encode("PUBLIC KEY", &p256.subject_public_key_info()),
        format!("{text}{text}"),
        pem::encode("CERTIFICATE", &ed25519.subject_public_key_info()),
        pem::encode("PUBLIC KEY", b"not a key"),
        "not pem".to_string(),
    ] {
        let error = pki.read_call_key(&bad).unwrap_err();
        assert!(is_validation(&error), "{bad}: {error:?}");
    }
}
