//! T056: `tokens.create`, `tokens.list` and `tokens.annul` on the local channel
//! (contracts/control-protocol.md): the exact fields of `data`, the token only in the reply of
//! `tokens.create`, the errors and the rows of the admin with the identity.

mod common;

use common::*;
use hecate_application::ports::{Clock, IdentityStore};
use hecate_application::testing::CaRig;
use hecate_domain::events::IdentityEvent;
use hecate_domain::sealing::access_log::{AccessResult, Operation};
use hecate_infrastructure::control::server::RequestHandler;
use serde_json::{Value, json};
use time::Duration;

fn keys(data: &Value) -> Vec<String> {
    let mut keys: Vec<String> = data.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    keys
}

fn with_ca() -> (CaRig, RequestHandler) {
    let rig = ca_rig();
    let (handler, _) = ca_handler(&rig);
    create_ca_through(&handler, &rig);
    (rig, handler)
}

fn create(handler: &RequestHandler, identity: &str) -> Value {
    call(handler, "tokens.create", json!({"identity": identity}))
}

fn rows(rig: &CaRig, operation: Operation) -> Vec<(AccessResult, Option<String>, Option<String>)> {
    rig.access_log
        .entries()
        .into_iter()
        .filter(|entry| entry.operation == operation)
        .map(|entry| (entry.result, entry.subject, entry.identity))
        .collect()
}

#[test]
fn the_token_operations_return_the_fields_of_the_contract() {
    let (rig, handler) = with_ca();
    let created = create(&handler, "hermes");
    assert_eq!(created["status"], "completed", "{created}");
    assert_eq!(
        keys(&created["data"]),
        ["expires_at", "identity", "token", "token_id"]
    );
    let token = created["data"]["token"].as_str().unwrap().to_string();
    assert!(token.starts_with("HET1.hermes."), "{token}");
    let listed = call(&handler, "tokens.list", json!({}));
    assert_eq!(keys(&listed["data"]), ["tokens"]);
    let tokens = listed["data"]["tokens"].as_array().unwrap();
    assert_eq!(tokens.len(), 1);
    assert_eq!(keys(&tokens[0]), ["expires_at", "identity", "token_id"]);
    assert_eq!(tokens[0]["token_id"], created["data"]["token_id"]);
    assert!(
        !listed
            .to_string()
            .contains(token.split('.').nth(3).unwrap())
    );
    let annulled = call(
        &handler,
        "tokens.annul",
        json!({"token_id": created["data"]["token_id"]}),
    );
    assert_eq!(
        annulled["data"],
        json!({"token_id": created["data"]["token_id"]})
    );
    assert!(
        call(&handler, "tokens.list", json!({}))["data"]["tokens"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let hermes = Some("hermes".to_string());
    let admin = Some("admin".to_string());
    assert_eq!(
        rows(&rig, Operation::TokenCreate),
        vec![(AccessResult::Success, admin.clone(), hermes.clone())]
    );
    assert_eq!(
        rows(&rig, Operation::TokenAnnul),
        vec![(AccessResult::Success, admin.clone(), hermes)]
    );
    assert_eq!(rows(&rig, Operation::TokenList).len(), 2);
}

#[test]
fn reserved_or_malformed_identities_are_validation_and_unknown_tokens_not_found() {
    let (rig, handler) = with_ca();
    for identity in ["hecate", "admin", "Not Valid"] {
        assert_eq!(
            code(&create(&handler, identity)),
            "validation",
            "{identity}"
        );
    }
    let missing = call(&handler, "tokens.create", json!({}));
    assert_eq!(code(&missing), "validation");
    let unknown = call(
        &handler,
        "tokens.annul",
        json!({"token_id": "0123456789abcdef"}),
    );
    assert_eq!(code(&unknown), "not_found");
    let malformed = call(&handler, "tokens.annul", json!({"token_id": 7}));
    assert_eq!(code(&malformed), "validation");
    let invalid = rows(&rig, Operation::TokenCreate)
        .into_iter()
        .filter(|(result, _, _)| *result == AccessResult::InvalidInput)
        .count();
    assert_eq!(invalid, 4);
    assert_eq!(rows(&rig, Operation::TokenAnnul).len(), 2);
}

#[test]
fn sealed_without_a_ca_disabled_or_an_expired_intermediate_are_conflict() {
    let rig = ca_rig();
    let (handler, _) = ca_handler(&rig);
    assert_eq!(code(&create(&handler, "hermes")), "conflict");
    create_ca_through(&handler, &rig);
    call(&handler, "seal", json!({}));
    assert_eq!(code(&create(&handler, "hermes")), "conflict");
    call(
        &handler,
        "unseal",
        json!({"passphrase": hecate_application::testing::GOOD_PASSPHRASE}),
    );
    // An identity that enrolled and was disabled, as the operations of story 4 leave it.
    let token = create(&handler, "hermes")["data"]["token"]
        .as_str()
        .unwrap()
        .to_string();
    let parsed = hecate_domain::identity::token::EnrollmentToken::parse(&token).unwrap();
    let now = rig.clock.now();
    let serial = hecate_domain::identity::names::Serial::from_be_bytes(&[5; 16]).unwrap();
    let records: Vec<_> = [
        IdentityEvent::IdentityEnrolled {
            at: now,
            token_id: parsed.id().clone(),
            serial,
        },
        IdentityEvent::IdentityDisabled {
            at: now,
            urgent: false,
        },
    ]
    .iter()
    .map(IdentityEvent::encode)
    .collect();
    rig.store
        .transaction(&mut |writes| {
            writes.append("identity/hermes", &records, 1)?;
            Ok(())
        })
        .unwrap();
    assert_eq!(code(&create(&handler, "hermes")), "conflict");
    rig.clock.advance(Duration::days(366));
    assert_eq!(code(&create(&handler, "hades")), "conflict");
}
