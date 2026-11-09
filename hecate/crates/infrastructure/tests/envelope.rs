mod common;

use common::*;
use hecate_application::ports::SecretMaterial;
use hecate_application::secrets::envelope::Envelope;
use hecate_domain::sensitive::SecretKey;
use proptest::prelude::*;

const PATH: &str = "secret://hermes/token";

fn root() -> SecretKey {
    SecretKey::new(std::array::from_fn(|i| i as u8))
}

fn envelope() -> Envelope {
    Envelope::new(crypto())
}

fn scope_key(envelope: &Envelope) -> SecretKey {
    envelope.new_scope_key(&root(), "hermes", 1).unwrap().0
}

fn flip(data: &[u8], index: usize) -> Vec<u8> {
    let mut flipped = data.to_vec();
    let at = index % flipped.len();
    flipped[at] ^= 0x01;
    flipped
}

// py: integration/test_envelope.py::test_scope_key_is_created_and_opened_with_the_root
#[test]
fn scope_key_is_created_and_opened_with_the_root() {
    let envelope = envelope();
    let (key, wrapped) = envelope.new_scope_key(&root(), "hermes", 1).unwrap();
    assert_ne!(key.expose(), root().expose());
    let opened = envelope
        .open_scope_key(&root(), "hermes", 1, &wrapped)
        .unwrap();
    assert_eq!(opened.expose(), key.expose());
    assert!(!contains(&wrapped.ciphertext, key.expose()));
}

// py: integration/test_envelope.py::test_scope_key_does_not_open_with_another_root_identity_or_version
#[test]
fn scope_key_does_not_open_with_another_root_identity_or_version() {
    let envelope = envelope();
    let (_, wrapped) = envelope.new_scope_key(&root(), "hermes", 1).unwrap();
    assert!(
        envelope
            .open_scope_key(&SecretKey::new([0; 32]), "hermes", 1, &wrapped)
            .is_err()
    );
    assert!(
        envelope
            .open_scope_key(&root(), "echidna", 1, &wrapped)
            .is_err()
    );
    assert!(
        envelope
            .open_scope_key(&root(), "hermes", 2, &wrapped)
            .is_err()
    );
}

// py: integration/test_envelope.py::test_each_scope_key_is_different
#[test]
fn each_scope_key_is_different() {
    let envelope = envelope();
    assert_ne!(scope_key(&envelope).expose(), scope_key(&envelope).expose());
}

// py: integration/test_envelope.py::test_seals_and_opens_a_value_through_the_version_key
#[test]
fn seals_and_opens_a_value_through_the_version_key() {
    let envelope = envelope();
    let key = scope_key(&envelope);
    let material = envelope
        .seal_value(&key, PATH, 1, "hermes", 1, b"s3cret-value")
        .unwrap();
    assert!(!contains(&material.value_ciphertext, b"s3cret-value"));
    assert_eq!(material.key_version, 1);
    let opened = envelope
        .open_value(&key, PATH, 1, "hermes", &material)
        .unwrap();
    assert_eq!(opened.expose(), b"s3cret-value");
}

// py: integration/test_envelope.py::test_every_version_gets_its_own_key
#[test]
fn every_version_gets_its_own_key() {
    let envelope = envelope();
    let key = scope_key(&envelope);
    let first = envelope
        .seal_value(&key, PATH, 1, "hermes", 1, b"same")
        .unwrap();
    let second = envelope
        .seal_value(&key, PATH, 1, "hermes", 1, b"same")
        .unwrap();
    assert_ne!(first.dek_ciphertext, second.dek_ciphertext);
    assert_ne!(first.value_ciphertext, second.value_ciphertext);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(60))]

    // py: integration/test_envelope.py::test_altering_any_byte_prevents_decryption
    #[test]
    fn altering_any_byte_prevents_decryption(value in prop::collection::vec(any::<u8>(), 1..=200), index in 0usize..=10_000) {
        let envelope = envelope();
        let key = scope_key(&envelope);
        let material = envelope.seal_value(&key, PATH, 1, "hermes", 1, &value).unwrap();
        let opened = envelope.open_value(&key, PATH, 1, "hermes", &material).unwrap();
        prop_assert_eq!(opened.expose(), value.as_slice());
        let altered = [
            SecretMaterial { value_nonce: flip(&material.value_nonce, index), ..material.clone() },
            SecretMaterial { value_ciphertext: flip(&material.value_ciphertext, index), ..material.clone() },
            SecretMaterial { dek_nonce: flip(&material.dek_nonce, index), ..material.clone() },
            SecretMaterial { dek_ciphertext: flip(&material.dek_ciphertext, index), ..material.clone() },
        ];
        for altered in &altered {
            prop_assert!(envelope.open_value(&key, PATH, 1, "hermes", altered).is_err());
        }
    }

    // py: integration/test_envelope.py::test_fingerprint_does_not_contain_the_value
    #[test]
    fn fingerprint_does_not_contain_the_value(value in prop::collection::vec(any::<u8>(), 4..=64)) {
        let fingerprint = envelope().fingerprint(&SecretKey::new([0; 32]), &value).replace('-', "");
        let hex: String = value.iter().map(|byte| format!("{byte:02x}")).collect();
        prop_assert!(!fingerprint.contains(&hex));
    }
}

// py: integration/test_envelope.py::test_moving_the_encrypted_data_elsewhere_prevents_decryption
#[test]
fn moving_the_encrypted_data_elsewhere_prevents_decryption() {
    let envelope = envelope();
    let key = scope_key(&envelope);
    let material = envelope
        .seal_value(&key, PATH, 1, "hermes", 1, b"value")
        .unwrap();
    for (path, version, identity, key_version) in [
        ("secret://hermes/other", 1, "hermes", 1),
        (PATH, 2, "hermes", 1),
        (PATH, 1, "echidna", 1),
        (PATH, 1, "hermes", 2),
    ] {
        let moved = SecretMaterial {
            key_version,
            ..material.clone()
        };
        assert!(
            envelope
                .open_value(&key, path, version, identity, &moved)
                .is_err()
        );
    }
}

// py: integration/test_envelope.py::test_another_scope_key_cannot_open_the_value
#[test]
fn another_scope_key_cannot_open_the_value() {
    let envelope = envelope();
    let material = envelope
        .seal_value(&scope_key(&envelope), PATH, 1, "hermes", 1, b"value")
        .unwrap();
    assert!(
        envelope
            .open_value(&scope_key(&envelope), PATH, 1, "hermes", &material)
            .is_err()
    );
}

// py: integration/test_envelope.py::test_fingerprint_is_deterministic_for_the_same_identity_and_value
#[test]
fn fingerprint_is_deterministic_for_the_same_identity_and_value() {
    let envelope = envelope();
    let key = scope_key(&envelope);
    assert_eq!(
        envelope.fingerprint(&key, b"value"),
        envelope.fingerprint(&key, b"value")
    );
    assert_ne!(
        envelope.fingerprint(&key, b"value"),
        envelope.fingerprint(&key, b"value2")
    );
}

// py: integration/test_envelope.py::test_fingerprint_differs_between_identities_and_has_the_documented_shape
#[test]
fn fingerprint_differs_between_identities_and_has_the_documented_shape() {
    let envelope = envelope();
    let key = scope_key(&envelope);
    let other = envelope.new_scope_key(&root(), "echidna", 1).unwrap().0;
    let fingerprint = envelope.fingerprint(&key, b"value");
    assert_ne!(fingerprint, envelope.fingerprint(&other, b"value"));
    let groups: Vec<&str> = fingerprint.split('-').collect();
    assert!(
        groups.len() == 4
            && groups
                .iter()
                .all(|group| group.len() == 4 && u16::from_str_radix(group, 16).is_ok())
    );
    assert_eq!(fingerprint, fingerprint.to_lowercase());
}
