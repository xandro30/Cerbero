mod common;

use hecate_application::ports::CryptoProvider;
use hecate_infrastructure::crypto::SoftwareCryptoProvider;

#[test]
fn passphrase_keys_match_the_python_vectors() {
    let crypto = SoftwareCryptoProvider::new();
    let vectors = common::vectors("kdf.json");
    let cases = vectors["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 3);
    for case in cases {
        let key = crypto
            .derive_key(
                case["passphrase"].as_str().unwrap(),
                &common::hex(&case["salt"]),
                case["opslimit"].as_u64().unwrap(),
                case["memlimit"].as_u64().unwrap() as usize,
            )
            .unwrap();
        assert_eq!(
            key.expose().as_slice(),
            common::hex(&case["key"]).as_slice()
        );
    }
}
