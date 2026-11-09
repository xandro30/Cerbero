//! `root.json`: the root protected by Python, read, opened and stored again by Rust.

mod common;

use aws_lc_rs::aead::{AES_256_GCM, Aad, LessSafeKey, Nonce, UnboundKey};
use hecate_application::ports::{CryptoProvider, SealRepository};
use hecate_domain::sensitive::SecretKey;
use hecate_infrastructure::crypto::SoftwareCryptoProvider;
use hecate_infrastructure::store::sealing::{
    SqliteSealRepository, dump_passphrase, dump_recovery, load_passphrase, load_recovery,
};
use rusqlite::params;
use serde_json::Value;

fn cases() -> Vec<Value> {
    common::vectors("root.json")["cases"]
        .as_array()
        .unwrap()
        .clone()
}

fn text(value: &Value) -> &str {
    value.as_str().unwrap()
}

fn json(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

fn passphrase_key(case: &Value) -> SecretKey {
    let protection = &case["passphrase_protection"];
    SoftwareCryptoProvider::new()
        .derive_key(
            text(&case["passphrase"]),
            &common::hex(&protection["salt"]),
            protection["kdf_opslimit"].as_u64().unwrap(),
            usize::try_from(protection["kdf_memlimit"].as_u64().unwrap()).unwrap(),
        )
        .unwrap()
}

fn recovery_key(case: &Value) -> SecretKey {
    SecretKey::from_slice(&common::hex(&case["recovery_key"])).unwrap()
}

fn seal_with_nonce(key: &SecretKey, nonce: &[u8], plaintext: &[u8], aad: &[u8]) -> Vec<u8> {
    let cipher = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, key.expose()).unwrap());
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
fn the_associated_data_is_the_same_as_in_python() {
    for case in cases() {
        let passphrase = load_passphrase(text(&case["passphrase_protection"]["stored"])).unwrap();
        let recovery = load_recovery(text(&case["recovery_protection"]["stored"])).unwrap();
        assert_eq!(
            passphrase.aad(),
            text(&case["passphrase_protection"]["aad"]).as_bytes()
        );
        assert_eq!(
            recovery.aad(),
            text(&case["recovery_protection"]["aad"]).as_bytes()
        );
    }
}

#[test]
fn a_fixed_nonce_gives_the_same_ciphertext_as_in_python() {
    let case = cases()
        .into_iter()
        .find(|case| case["kind"] == "fixed")
        .unwrap();
    let root = common::hex(&case["root"]);
    for (protection, key) in [
        (&case["passphrase_protection"], passphrase_key(&case)),
        (&case["recovery_protection"], recovery_key(&case)),
    ] {
        let sealed = seal_with_nonce(
            &key,
            &common::hex(&protection["nonce"]),
            &root,
            text(&protection["aad"]).as_bytes(),
        );
        assert_eq!(sealed, common::hex(&protection["ciphertext"]));
    }
}

#[test]
fn rust_opens_the_root_that_python_protected() {
    let crypto = SoftwareCryptoProvider::new();
    for case in cases() {
        let root = common::hex(&case["root"]);
        let passphrase = load_passphrase(text(&case["passphrase_protection"]["stored"])).unwrap();
        let opened = crypto
            .aead_decrypt(
                &passphrase_key(&case),
                &passphrase.nonce,
                &passphrase.ciphertext,
                &passphrase.aad(),
            )
            .unwrap();
        assert_eq!(opened.expose(), root.as_slice(), "{}", case["kind"]);
        let recovery = load_recovery(text(&case["recovery_protection"]["stored"])).unwrap();
        let opened = crypto
            .aead_decrypt(
                &recovery_key(&case),
                &recovery.nonce,
                &recovery.ciphertext,
                &recovery.aad(),
            )
            .unwrap();
        assert_eq!(opened.expose(), root.as_slice(), "{}", case["kind"]);
    }
}

#[test]
fn rust_reads_the_stored_protections_and_writes_the_same_json_content() {
    for case in cases() {
        let vector = &case["passphrase_protection"];
        let passphrase = load_passphrase(text(&vector["stored"])).unwrap();
        assert_eq!(passphrase.version, vector["version"].as_u64().unwrap());
        assert_eq!(passphrase.kdf, text(&vector["kdf"]));
        assert_eq!(
            passphrase.kdf_opslimit,
            vector["kdf_opslimit"].as_u64().unwrap()
        );
        assert_eq!(
            passphrase.kdf_memlimit,
            vector["kdf_memlimit"].as_u64().unwrap()
        );
        assert_eq!(passphrase.salt, common::hex(&vector["salt"]));
        assert_eq!(passphrase.aead, text(&vector["aead"]));
        assert_eq!(passphrase.nonce, common::hex(&vector["nonce"]));
        assert_eq!(passphrase.ciphertext, common::hex(&vector["ciphertext"]));
        assert_eq!(
            json(&dump_passphrase(&passphrase)),
            json(text(&vector["stored"]))
        );

        let vector = &case["recovery_protection"];
        let recovery = load_recovery(text(&vector["stored"])).unwrap();
        assert_eq!(recovery.version, vector["version"].as_u64().unwrap());
        assert_eq!(recovery.share_set_id, common::hex(&vector["share_set_id"]));
        assert_eq!(
            u64::from(recovery.threshold),
            vector["threshold"].as_u64().unwrap()
        );
        assert_eq!(
            u64::from(recovery.share_count),
            vector["share_count"].as_u64().unwrap()
        );
        assert_eq!(recovery.aead, text(&vector["aead"]));
        assert_eq!(recovery.nonce, common::hex(&vector["nonce"]));
        assert_eq!(recovery.ciphertext, common::hex(&vector["ciphertext"]));
        assert_eq!(
            json(&dump_recovery(&recovery)),
            json(text(&vector["stored"]))
        );
    }
}

#[test]
fn a_seal_config_row_stored_by_python_is_read_and_saved_back_with_the_same_content() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hecate.db");
    let repo = SqliteSealRepository::open(&path).unwrap();
    let case = &cases()[0];
    let passphrase = text(&case["passphrase_protection"]["stored"]);
    let recovery = text(&case["recovery_protection"]["stored"]);
    let raw = rusqlite::Connection::open(&path).unwrap();
    raw.execute(
        "INSERT INTO seal_config (id, config_version, initialized_at, updated_at, passphrase, recovery)
         VALUES (1, 1, ?1, ?1, ?2, ?3)",
        params!["2026-11-07T12:00:00+00:00", passphrase, recovery],
    )
    .unwrap();

    let config = repo.load_config().unwrap().unwrap();
    assert_eq!(config.passphrase, load_passphrase(passphrase).unwrap());
    assert_eq!(config.recovery, load_recovery(recovery).unwrap());
    repo.transaction(&mut |writes| writes.save_config(&config))
        .unwrap();

    let (saved_passphrase, saved_recovery, initialized_at): (String, String, String) = raw
        .query_row(
            "SELECT passphrase, recovery, initialized_at FROM seal_config WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(json(&saved_passphrase), json(passphrase));
    assert_eq!(json(&saved_recovery), json(recovery));
    assert_eq!(initialized_at, "2026-11-07T12:00:00+00:00");
}
