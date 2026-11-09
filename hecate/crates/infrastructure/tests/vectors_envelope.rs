//! `envelope.json`, `aad.json` and `fingerprint.json`: the layers that Python encrypted, opened
//! and reproduced by Rust.

mod common;

use aws_lc_rs::aead::{AES_256_GCM, Aad, LessSafeKey, Nonce, UnboundKey};
use common::*;
use hecate_application::ports::{KeyMaterial, SecretMaterial};
use hecate_application::secrets::envelope::{Envelope, dek_aad, scope_aad, value_aad};
use hecate_domain::sealing::protection::associated_data;
use hecate_domain::sensitive::SecretKey;
use serde_json::Value;

fn key(value: &Value) -> SecretKey {
    SecretKey::from_slice(&hex(value)).unwrap()
}

fn text(value: &Value) -> &str {
    value.as_str().unwrap()
}

fn seal_with_nonce(key: &[u8], nonce: &[u8], plaintext: &[u8], aad: &[u8]) -> Vec<u8> {
    let cipher = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, key).unwrap());
    let mut sealed = plaintext.to_vec();
    cipher
        .seal_in_place_append_tag(
            Nonce::try_assume_unique_for_key(nonce).unwrap(),
            Aad::from(aad),
            &mut sealed,
        )
        .unwrap();
    sealed
}

#[test]
fn the_associated_data_of_every_layer_is_the_same_as_in_python() {
    let cases = vectors("aad.json")["cases"].as_array().unwrap().clone();
    assert_eq!(cases.len(), 7);
    for case in cases {
        let number = |name: &str| case[name].as_u64().unwrap();
        let built = match text(&case["layer"]) {
            "root" => associated_data(text(&case["protection"]), number("config_version")),
            "scope_key" => scope_aad(text(&case["identity"]), number("key_version")),
            "dek" => dek_aad(
                text(&case["path"]),
                number("version"),
                text(&case["scope"]),
                number("key_version"),
            ),
            "value" => value_aad(text(&case["path"]), number("version"), text(&case["scope"])),
            other => panic!("unknown layer {other}"),
        };
        assert_eq!(built, text(&case["aad"]).as_bytes(), "{case}");
    }
}

#[test]
fn a_fixed_nonce_gives_the_same_ciphertexts_as_in_python() {
    let cases = vectors("envelope.json")["cases"]
        .as_array()
        .unwrap()
        .clone();
    let case = cases.iter().find(|case| case["kind"] == "fixed").unwrap();
    let (identity, path) = (text(&case["identity"]), text(&case["path"]));
    let (version, key_version) = (
        case["version"].as_u64().unwrap(),
        case["key_version"].as_u64().unwrap(),
    );
    let scope = hex(&case["scope_key"]);
    let dek = hex(&case["dek"]);
    assert_eq!(
        seal_with_nonce(
            &hex(&case["root"]),
            &hex(&case["scope_key_nonce"]),
            &scope,
            &scope_aad(identity, key_version)
        ),
        hex(&case["scope_key_ciphertext"])
    );
    assert_eq!(
        seal_with_nonce(
            &scope,
            &hex(&case["dek_nonce"]),
            &dek,
            &dek_aad(path, version, identity, key_version)
        ),
        hex(&case["dek_ciphertext"])
    );
    assert_eq!(
        seal_with_nonce(
            &dek,
            &hex(&case["value_nonce"]),
            text(&case["value"]).as_bytes(),
            &value_aad(path, version, identity)
        ),
        hex(&case["value_ciphertext"])
    );
}

#[test]
fn rust_opens_the_scope_keys_and_values_that_python_sealed() {
    let envelope = Envelope::new(crypto());
    let cases = vectors("envelope.json")["cases"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(cases.len(), 3);
    for case in cases {
        let (identity, path) = (text(&case["identity"]), text(&case["path"]));
        let (version, key_version) = (
            case["version"].as_u64().unwrap(),
            case["key_version"].as_u64().unwrap(),
        );
        let wrapped = KeyMaterial {
            nonce: hex(&case["scope_key_nonce"]),
            ciphertext: hex(&case["scope_key_ciphertext"]),
        };
        let scope = envelope
            .open_scope_key(&key(&case["root"]), identity, key_version, &wrapped)
            .unwrap();
        assert_eq!(
            scope.expose().as_slice(),
            hex(&case["scope_key"]).as_slice()
        );
        let material = SecretMaterial {
            key_version,
            value_nonce: hex(&case["value_nonce"]),
            value_ciphertext: hex(&case["value_ciphertext"]),
            dek_nonce: hex(&case["dek_nonce"]),
            dek_ciphertext: hex(&case["dek_ciphertext"]),
        };
        let value = envelope
            .open_value(&scope, path, version, identity, &material)
            .unwrap();
        assert_eq!(
            value.expose(),
            text(&case["value"]).as_bytes(),
            "{}",
            case["kind"]
        );
    }
}

#[test]
fn fingerprints_are_the_same_as_in_python() {
    let envelope = Envelope::new(crypto());
    let cases = vectors("fingerprint.json")["cases"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(cases.len(), 4);
    for case in cases {
        let fingerprint =
            envelope.fingerprint(&key(&case["scope_key"]), text(&case["value"]).as_bytes());
        assert_eq!(fingerprint, text(&case["fingerprint"]));
    }
}
