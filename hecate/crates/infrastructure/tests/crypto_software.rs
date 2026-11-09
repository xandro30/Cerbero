use hecate_application::ports::CryptoProvider;
use hecate_infrastructure::crypto::SoftwareCryptoProvider;
use proptest::prelude::*;

const MIN_OPS: u64 = 1;
const MIN_MEM: usize = 8192;

fn crypto() -> SoftwareCryptoProvider {
    SoftwareCryptoProvider::new()
}

// py: integration/test_crypto_software.py::test_aead_roundtrip_with_associated_data
#[test]
fn aead_roundtrip_with_associated_data() {
    let crypto = crypto();
    let key = crypto.random_key();
    let (nonce, ciphertext) = crypto.aead_encrypt(&key, b"secret", b"aad").unwrap();
    assert_eq!(nonce.len(), 12);
    let plaintext = crypto
        .aead_decrypt(&key, &nonce, &ciphertext, b"aad")
        .unwrap();
    assert_eq!(plaintext.expose(), b"secret");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(60))]

    // py: integration/test_crypto_software.py::test_aead_any_altered_byte_fails
    #[test]
    fn aead_any_altered_byte_fails(
        plaintext in proptest::collection::vec(any::<u8>(), 1..=64),
        aad in proptest::collection::vec(any::<u8>(), 0..=32),
        which in 0usize..3,
        position in any::<proptest::sample::Index>(),
    ) {
        let crypto = crypto();
        let key = crypto.random_key();
        let (nonce, ciphertext) = crypto.aead_encrypt(&key, &plaintext, &aad).unwrap();
        let mut parts = [nonce, ciphertext, aad];
        if !parts[which].is_empty() {
            let at = position.index(parts[which].len());
            parts[which][at] ^= 0x01;
            let [nonce, ciphertext, aad] = parts;
            prop_assert!(crypto.aead_decrypt(&key, &nonce, &ciphertext, &aad).is_err());
        }
    }
}

// py: integration/test_crypto_software.py::test_aead_wrong_key_fails
#[test]
fn aead_wrong_key_fails() {
    let crypto = crypto();
    let (nonce, ciphertext) = crypto
        .aead_encrypt(&crypto.random_key(), b"x", b"")
        .unwrap();
    assert!(
        crypto
            .aead_decrypt(&crypto.random_key(), &nonce, &ciphertext, b"")
            .is_err()
    );
}

// py: integration/test_crypto_software.py::test_derive_key_is_deterministic_and_depends_on_inputs
#[test]
fn derive_key_is_deterministic_and_depends_on_inputs() {
    let crypto = crypto();
    let salt = crypto.random_bytes(16);
    let first = crypto
        .derive_key("a long passphrase", &salt, MIN_OPS, MIN_MEM)
        .unwrap();
    assert_eq!(first.expose().len(), 32);
    let again = crypto
        .derive_key("a long passphrase", &salt, MIN_OPS, MIN_MEM)
        .unwrap();
    assert_eq!(first.expose(), again.expose());
    let other_passphrase = crypto
        .derive_key("another passphrase", &salt, MIN_OPS, MIN_MEM)
        .unwrap();
    assert_ne!(first.expose(), other_passphrase.expose());
    let other_salt = crypto
        .derive_key(
            "a long passphrase",
            &crypto.random_bytes(16),
            MIN_OPS,
            MIN_MEM,
        )
        .unwrap();
    assert_ne!(first.expose(), other_salt.expose());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(40))]

    // py: integration/test_crypto_software.py::test_shamir_any_pair_recovers_and_one_never
    #[test]
    fn shamir_any_pair_recovers_and_one_never(secret in any::<[u8; 32]>()) {
        let crypto = crypto();
        let key = hecate_domain::sensitive::SecretKey::new(secret);
        let shares = crypto.shamir_split(&key).unwrap();
        let indices: Vec<u8> = shares.iter().map(|(index, _)| *index).collect();
        prop_assert_eq!(indices, vec![1, 2, 3]);
        prop_assert!(shares.iter().all(|(_, values)| values.expose().len() == 32));
        for a in 0..3 {
            for b in 0..3 {
                if a != b {
                    let pair = [
                        (shares[a].0, shares[a].1.expose()),
                        (shares[b].0, shares[b].1.expose()),
                    ];
                    let recovered = crypto.shamir_combine(&pair).unwrap();
                    prop_assert_eq!(recovered.expose(), &secret);
                }
            }
        }
        for (index, values) in &shares {
            prop_assert!(crypto.shamir_combine(&[(*index, values.expose())]).is_err());
        }
    }
}

// py: integration/test_crypto_software.py::test_shamir_pair_from_different_sets_does_not_recover
#[test]
fn shamir_pair_from_different_sets_does_not_recover() {
    let crypto = crypto();
    let secret = crypto.random_key();
    let one = crypto.shamir_split(&secret).unwrap();
    let two = crypto.shamir_split(&secret).unwrap();
    let pair = [(one[0].0, one[0].1.expose()), (two[1].0, two[1].1.expose())];
    let recovered = crypto.shamir_combine(&pair).unwrap();
    assert_ne!(recovered.expose(), secret.expose());
}

// py: integration/test_crypto_software.py::test_random_bytes_length
#[test]
fn random_bytes_length() {
    assert_eq!(crypto().random_bytes(16).len(), 16);
}
