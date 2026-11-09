mod common;

use common::*;
use hecate_application::ports::{EventStore, MaterialStore};
use hecate_application::secrets::service::CreateOptions;
use hecate_application::testing::GOOD_PASSPHRASE;
use serde_json::json;

const PATH: &str = "secret://hermes/telegram/bot_token";
const V1: &str = "SHRED-SENTINEL-first-0123456789";
const V2: &str = "SHRED-SENTINEL-second-0123456789";

// py: integration/test_secret_crypto_shred.py::test_a_destroyed_version_cannot_be_recovered_with_the_root_nor_from_a_later_copy
#[test]
fn a_destroyed_version_cannot_be_recovered_with_the_root_nor_from_a_later_copy() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("hecate.db");
    let stack = ready(&db);
    stack
        .service
        .create(PATH, V1, "string", &no_options())
        .unwrap();
    stack.service.add_version(PATH, V2, &json!(2)).unwrap();
    stack.service.revoke(PATH, &json!(1), &json!(4)).unwrap();
    let before = stack.store.load_secret_material(PATH, 1).unwrap().unwrap();
    stack
        .service
        .destroy(
            PATH,
            &json!(1),
            &json!(5),
            &json!({"passphrase": GOOD_PASSPHRASE}),
        )
        .unwrap();
    stack.close();

    // A copy of the database made after the destruction (the backup the admin keeps).
    let copy = dir.path().join("backup.db");
    std::fs::copy(&db, &copy).unwrap();
    for file in [&db, &copy] {
        let raw = std::fs::read(file).unwrap();
        for piece in [
            &before.value_ciphertext,
            &before.dek_ciphertext,
            &before.dek_nonce,
            &before.value_nonce,
        ] {
            assert!(
                !contains(&raw, piece),
                "material of the destroyed version is still in {file:?}"
            );
        }
    }

    let restored = stack_of(&copy);
    restored
        .sealing
        .unseal_with_passphrase(GOOD_PASSPHRASE)
        .unwrap();
    assert_eq!(restored.store.load_secret_material(PATH, 1).unwrap(), None);
    assert!(
        restored
            .service
            .resolve("hermes", &json!([format!("{PATH}@1")]))
            .is_err()
    );
    assert_eq!(
        restored.service.resolve("hermes", &json!([PATH])).unwrap()[0]
            .value
            .as_str(),
        V2
    );
    let kinds: Vec<String> = restored
        .store
        .load("secret/hermes/telegram/bot_token")
        .unwrap()
        .into_iter()
        .map(|record| record.kind)
        .collect();
    assert!(kinds.contains(&"VersionDestroyed".to_string()));
    assert_eq!(
        kinds.iter().filter(|kind| *kind == "VersionAdded").count(),
        2,
        "the history stays"
    );
    restored.close();
}

fn stack_of(db: &std::path::Path) -> hecate_infrastructure::testing::SqliteStack {
    stack(db)
}

// py: integration/test_secret_crypto_shred.py::test_the_version_destroyed_by_the_maximum_is_erased_as_well
#[test]
fn the_version_destroyed_by_the_maximum_is_erased_as_well() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("hecate.db");
    let stack = ready(&db);
    let path = "secret://hermes/limited";
    let options = CreateOptions {
        max_versions: json!(2),
        ..CreateOptions::default()
    };
    stack.service.create(path, V1, "string", &options).unwrap();
    let material = stack.store.load_secret_material(path, 1).unwrap().unwrap();
    stack.service.add_version(path, V2, &json!(2)).unwrap();
    stack
        .service
        .add_version(path, &format!("{V2}-3"), &json!(4))
        .unwrap();
    stack.close();
    let raw = std::fs::read(&db).unwrap();
    assert!(
        !contains(&raw, &material.dek_ciphertext) && !contains(&raw, &material.value_ciphertext)
    );
}

// py: integration/test_secret_crypto_shred.py::test_the_other_versions_keep_their_material
#[test]
fn the_other_versions_keep_their_material() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("hecate.db");
    let stack = ready(&db);
    stack
        .service
        .create(PATH, V1, "string", &no_options())
        .unwrap();
    stack.service.add_version(PATH, V2, &json!(2)).unwrap();
    let keep = stack.store.load_secret_material(PATH, 2).unwrap().unwrap();
    stack.close();
    assert!(contains(
        &std::fs::read(&db).unwrap(),
        &keep.value_ciphertext
    ));
}
