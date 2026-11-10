//! T069: the verifier of the short call token (contracts/formats.md, research R3): EdDSA only,
//! `iss` the identity of the connection and `aud` Hécate, at most 300 s of life with 30 s of
//! margin, a `jti` of 1 to 128 printable ASCII characters, and the registered key.

use data_encoding::BASE64URL_NOPAD;
use hecate_application::ports::CallTokenVerifier;
use hecate_domain::identity::call_key::CallKey;
use hecate_domain::identity::names::SpiffeId;
use hecate_domain::sealing::errors::HecateError;
use hecate_infrastructure::call_token::JwtCallTokenVerifier;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use rcgen::{KeyPair, PKCS_ED25519};
use serde_json::{Value, json};
use time::macros::datetime;
use time::{Duration, OffsetDateTime};

const NOW: OffsetDateTime = datetime!(2026-11-09 12:00 UTC);
const HERMES: &str = "spiffe://cerbero.test/hermes";
const HECATE: &str = "spiffe://cerbero.test/hecate";

struct Signer {
    pair: KeyPair,
}

impl Signer {
    fn new() -> Self {
        Self {
            pair: KeyPair::generate_for(&PKCS_ED25519).unwrap(),
        }
    }

    fn key(&self) -> CallKey {
        CallKey::from_bytes(self.pair.public_key_raw()).unwrap()
    }

    fn sign(&self, claims: &Value) -> String {
        let header = Header {
            typ: Some("JWT".into()),
            ..Header::new(Algorithm::EdDSA)
        };
        jsonwebtoken::encode(
            &header,
            claims,
            &EncodingKey::from_ed_der(&self.pair.serialize_der()),
        )
        .unwrap()
    }
}

fn claims(iat: OffsetDateTime, lifetime: i64, jti: &str) -> Value {
    json!({
        "iss": HERMES,
        "aud": HECATE,
        "iat": iat.unix_timestamp(),
        "exp": iat.unix_timestamp() + lifetime,
        "jti": jti,
    })
}

fn verify(token: &str, key: &CallKey, now: OffsetDateTime) -> Result<(), HecateError> {
    JwtCallTokenVerifier::new()
        .verify(
            token,
            key,
            &SpiffeId::parse(HERMES).unwrap(),
            &SpiffeId::parse(HECATE).unwrap(),
            now,
        )
        .map(|_| ())
}

fn is_auth(result: Result<(), HecateError>) -> bool {
    matches!(result, Err(HecateError::AuthenticationFailed(_)))
}

#[test]
fn a_token_signed_with_the_registered_key_for_hecate_is_accepted() {
    let signer = Signer::new();
    let token = signer.sign(&claims(NOW, 300, "call-1"));
    let read = JwtCallTokenVerifier::new()
        .verify(
            &token,
            &signer.key(),
            &SpiffeId::parse(HERMES).unwrap(),
            &SpiffeId::parse(HECATE).unwrap(),
            NOW,
        )
        .unwrap();
    assert_eq!(read.issuer.to_string(), HERMES);
    assert_eq!(read.audience.to_string(), HECATE);
    assert_eq!(
        (read.issued_at, read.expires_at),
        (NOW, NOW + Duration::seconds(300))
    );
    assert_eq!(read.jti, "call-1");
}

#[test]
fn only_eddsa_is_accepted() {
    let signer = Signer::new();
    let payload = BASE64URL_NOPAD.encode(claims(NOW, 300, "call-1").to_string().as_bytes());
    let none = format!(
        "{}.{payload}.",
        BASE64URL_NOPAD.encode(br#"{"alg":"none","typ":"JWT"}"#)
    );
    assert!(is_auth(verify(&none, &signer.key(), NOW)));
    let hs256 = jsonwebtoken::encode(
        &Header::new(Algorithm::HS256),
        &claims(NOW, 300, "call-1"),
        &EncodingKey::from_secret(signer.key().bytes()),
    )
    .unwrap();
    assert!(is_auth(verify(&hs256, &signer.key(), NOW)));
    assert!(is_auth(verify("not a token", &signer.key(), NOW)));
}

#[test]
fn the_issuer_is_the_connection_and_the_audience_hecate() {
    let signer = Signer::new();
    let mut other_issuer = claims(NOW, 300, "call-1");
    other_issuer["iss"] = json!("spiffe://cerbero.test/hades");
    assert!(is_auth(verify(
        &signer.sign(&other_issuer),
        &signer.key(),
        NOW
    )));
    let mut other_audience = claims(NOW, 300, "call-1");
    other_audience["aud"] = json!("spiffe://cerbero.test/hermes");
    assert!(is_auth(verify(
        &signer.sign(&other_audience),
        &signer.key(),
        NOW
    )));
    let mut many = claims(NOW, 300, "call-1");
    many["aud"] = json!([HECATE]);
    assert!(is_auth(verify(&signer.sign(&many), &signer.key(), NOW)));
    let mut missing = claims(NOW, 300, "call-1");
    missing.as_object_mut().unwrap().remove("jti");
    assert!(is_auth(verify(&signer.sign(&missing), &signer.key(), NOW)));
}

#[test]
fn it_lives_300_seconds_at_most_with_30_seconds_of_margin() {
    let signer = Signer::new();
    let key = signer.key();
    assert!(verify(&signer.sign(&claims(NOW, 300, "a")), &key, NOW).is_ok());
    assert!(is_auth(verify(
        &signer.sign(&claims(NOW, 301, "a")),
        &key,
        NOW
    )));
    assert!(is_auth(verify(
        &signer.sign(&claims(NOW, -1, "a")),
        &key,
        NOW
    )));
    let token = signer.sign(&claims(NOW, 60, "a"));
    assert!(verify(&token, &key, NOW + Duration::seconds(89)).is_ok());
    assert!(is_auth(verify(&token, &key, NOW + Duration::seconds(90))));
    let early = signer.sign(&claims(NOW + Duration::seconds(30), 60, "a"));
    assert!(verify(&early, &key, NOW).is_ok());
    let future = signer.sign(&claims(NOW + Duration::seconds(31), 60, "a"));
    assert!(is_auth(verify(&future, &key, NOW)));
}

#[test]
fn the_jti_has_1_to_128_printable_ascii_characters() {
    let signer = Signer::new();
    let key = signer.key();
    assert!(verify(&signer.sign(&claims(NOW, 60, &"j".repeat(128))), &key, NOW).is_ok());
    for jti in ["", &"j".repeat(129), "caf\u{e9}", "tab\there"] {
        assert!(
            is_auth(verify(&signer.sign(&claims(NOW, 60, jti)), &key, NOW)),
            "{jti:?}"
        );
    }
}

#[test]
fn a_token_signed_with_another_key_is_refused() {
    let signer = Signer::new();
    let other = Signer::new();
    assert!(is_auth(verify(
        &other.sign(&claims(NOW, 60, "a")),
        &signer.key(),
        NOW
    )));
}
