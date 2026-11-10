//! T052: secrets of the reserved identities (FR-027): `hecate` and `admin` take no new secrets nor
//! versions; what was stored before keeps its revocation and destruction; any other identity can
//! prepare its secrets before its enrollment.

mod common;

use common::*;
use hecate_application::ports::{SecretMaterial, SecretStore};
use hecate_application::secrets::service::CreateOptions;
use hecate_application::testing::{GOOD_PASSPHRASE, START, SecretsRig};
use hecate_domain::events::EventRecord;
use hecate_domain::sealing::access_log::{AccessResult, Operation};
use hecate_domain::secrets::secret::SecretEvent;
use serde_json::json;

/// A secret of a reserved identity stored before the reservation existed, as feature 002 allowed.
fn legacy_secret(rig: &SecretsRig, path: &str) {
    let owner = path
        .trim_start_matches("secret://")
        .split('/')
        .next()
        .unwrap()
        .to_string();
    let records: Vec<EventRecord> = [
        SecretEvent::SecretCreated {
            at: START,
            path: path.to_string(),
            owner,
            value_type: "string".into(),
            max_versions: 10,
            rotate_every: None,
            expires_at: None,
        },
        SecretEvent::VersionAdded {
            at: START,
            version: 1,
            fingerprint: "aaaa-bbbb-cccc-dddd".into(),
            key_version: 1,
        },
    ]
    .iter()
    .map(SecretEvent::encode)
    .collect();
    let material = SecretMaterial {
        key_version: 1,
        value_nonce: vec![1; 12],
        value_ciphertext: vec![2; 32],
        dek_nonce: vec![3; 12],
        dek_ciphertext: vec![4; 48],
    };
    rig.store
        .transaction(&mut |writes| {
            writes.append(&stream_of(path), &records, 0)?;
            writes.save_secret_material(path, 1, &material)?;
            Ok(())
        })
        .unwrap();
}

#[test]
fn reserved_identities_take_no_new_secrets_nor_versions() {
    let rig = secrets();
    for path in ["secret://hecate/tls", "secret://admin/notes"] {
        let created = rig
            .service
            .create(path, "value", "string", &CreateOptions::default())
            .err()
            .unwrap();
        assert!(is_invalid(&created), "{path}: {created:?}");
        legacy_secret(&rig, path);
        let added = rig
            .service
            .add_version(path, "value", &json!(2))
            .err()
            .unwrap();
        assert!(is_invalid(&added), "{path}: {added:?}");
        assert_eq!(events(&rig, &stream_of(path)).len(), 2);
    }
    let refused = rig
        .secret_rows()
        .into_iter()
        .filter(|row| row.result == AccessResult::InvalidInput)
        .count();
    assert_eq!(refused, 4);
}

#[test]
fn what_was_stored_before_keeps_its_revocation_and_destruction() {
    let rig = secrets();
    let path = "secret://hecate/legacy";
    legacy_secret(&rig, path);
    let revoked = rig.service.revoke(path, &json!(1), &json!(2)).unwrap();
    assert_eq!(revoked, 3);
    rig.service
        .destroy(
            path,
            &json!(1),
            &json!(3),
            &json!({"passphrase": GOOD_PASSPHRASE}),
        )
        .unwrap();
    assert!(!has_material(&rig, path, 1));
    let destroyed = rig.secret_rows().into_iter().any(|row| {
        row.operation == Operation::SecretDestroy && row.result == AccessResult::Success
    });
    assert!(destroyed);
}

#[test]
fn other_identities_prepare_their_secrets_before_their_enrollment() {
    let rig = secrets();
    let created = create(&rig, "secret://hermes/token", "value", "string");
    assert_eq!(created.version, 1);
}
