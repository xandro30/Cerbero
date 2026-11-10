//! T090: `identities.list`, `identities.get`, `identities.disable`, `identities.enable` and
//! `certificates.revoke` on the local channel (contracts/control-protocol.md): the exact fields
//! of `data`, the errors, the repeated orders and the rows of the admin.

mod common;

use common::*;
use hecate_application::ports::Clock;
use hecate_application::testing::{CaRig, GOOD_PASSPHRASE};
use hecate_domain::instant::isoformat;
use hecate_domain::sealing::access_log::{AccessResult, Operation};
use hecate_infrastructure::control::server::RequestHandler;
use hecate_infrastructure::testing::{TempDir, TestComponent};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

fn keys(data: &Value) -> Vec<String> {
    let mut keys: Vec<String> = data.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    keys
}

fn with_ca() -> (CaRig, RequestHandler, Arc<AtomicUsize>) {
    let rig = ca_rig();
    let (handler, changed) = ca_handler(&rig);
    create_ca_through(&handler, &rig);
    (rig, handler, changed)
}

/// A component enrolled through the service, with the directory that keeps its keys.
fn enrolled(rig: &CaRig, identity: &str) -> (TestComponent, TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let component = TestComponent::open(dir.path(), rig.clock.clone()).unwrap();
    let token = rig
        .identities
        .tokens_create(identity)
        .unwrap()
        .token
        .to_string();
    let (body, key) = component.enrollment(&token).unwrap();
    let issued = rig.identities.enroll(Some(&key), &body).unwrap();
    component
        .accept_enrollment(&issued.certificate, &issued.chain)
        .unwrap();
    (component, dir)
}

type Row = (AccessResult, Option<String>, Option<String>, Option<String>);

fn rows(rig: &CaRig, operation: Operation) -> Vec<Row> {
    rig.access_log
        .entries()
        .into_iter()
        .filter(|entry| entry.operation == operation)
        .map(|entry| {
            (
                entry.result,
                entry.subject,
                entry.identity,
                entry.certificate,
            )
        })
        .collect()
}

fn own_serial(handler: &RequestHandler) -> String {
    call(handler, "status", json!({}))["data"]["ca"]["hecate_certificate"]["serial"]
        .as_str()
        .unwrap()
        .to_string()
}

fn some(text: &str) -> Option<String> {
    Some(text.to_string())
}

#[test]
fn the_identity_operations_return_the_fields_of_the_contract() {
    let (rig, handler, _) = with_ca();
    let (hermes, _hermes_dir) = enrolled(&rig, "hermes");
    let serial = hermes.serial().unwrap();
    let listed = call(&handler, "identities.list", json!({}));
    assert_eq!(listed["status"], "completed", "{listed}");
    assert_eq!(keys(&listed["data"]), ["identities"]);
    let not_after = isoformat(rig.clock.now() + time::Duration::hours(72));
    assert_eq!(
        listed["data"]["identities"],
        json!([{"identity": "hermes", "status": "enrolled", "certificate_not_after": not_after}])
    );
    let got = call(&handler, "identities.get", json!({"identity": "hermes"}));
    assert_eq!(keys(&got["data"]), ["certificates", "identity", "status"]);
    assert_eq!(
        (&got["data"]["identity"], &got["data"]["status"]),
        (&json!("hermes"), &json!("enrolled"))
    );
    let certificates = got["data"]["certificates"].as_array().unwrap();
    assert_eq!(certificates.len(), 1);
    assert_eq!(
        keys(&certificates[0]),
        ["not_after", "not_before", "renewable", "serial", "state"]
    );
    assert_eq!(certificates[0]["serial"], serial.as_str());
    assert_eq!(certificates[0]["state"], "valid");
    assert_eq!(certificates[0]["renewable"], true);
    assert_eq!(certificates[0]["not_after"], not_after);
    let disabled = call(
        &handler,
        "identities.disable",
        json!({"identity": "hermes", "urgent": true}),
    );
    assert_eq!(
        disabled["data"],
        json!({"identity": "hermes", "revoked": [serial.as_str()]})
    );
    let enabled = call(&handler, "identities.enable", json!({"identity": "hermes"}));
    assert_eq!(enabled["data"], json!({"identity": "hermes"}));
    let (hades, _hades_dir) = enrolled(&rig, "hades");
    let hades_serial = hades.serial().unwrap();
    let revoked = call(
        &handler,
        "certificates.revoke",
        json!({"serial": hades_serial.as_str()}),
    );
    assert_eq!(
        revoked["data"],
        json!({"serial": hades_serial.as_str(), "reissued": null})
    );
    let admin = some("admin");
    assert_eq!(
        rows(&rig, Operation::IdentityList),
        vec![(AccessResult::Success, admin.clone(), None, None)]
    );
    for operation in [
        Operation::IdentityGet,
        Operation::IdentityDisable,
        Operation::IdentityEnable,
    ] {
        assert_eq!(
            rows(&rig, operation),
            vec![(AccessResult::Success, admin.clone(), some("hermes"), None)],
            "{operation:?}"
        );
    }
    assert_eq!(
        rows(&rig, Operation::CertificateRevoke).pop().unwrap(),
        (
            AccessResult::Success,
            admin,
            some("hades"),
            some(hades_serial.as_str())
        )
    );
}

#[test]
fn a_repeated_order_answers_the_same_fields_without_new_serials() {
    let (rig, handler, _) = with_ca();
    let (hermes, _dir) = enrolled(&rig, "hermes");
    let serial = hermes.serial().unwrap();
    let passive = call(
        &handler,
        "identities.disable",
        json!({"identity": "hermes"}),
    );
    assert_eq!(
        passive["data"],
        json!({"identity": "hermes", "revoked": []})
    );
    let urgent = json!({"identity": "hermes", "urgent": true});
    let first = call(&handler, "identities.disable", urgent.clone());
    assert_eq!(first["data"]["revoked"], json!([serial.as_str()]));
    let again = call(&handler, "identities.disable", urgent);
    assert_eq!(again["data"], json!({"identity": "hermes", "revoked": []}));
    for _ in 0..2 {
        let enabled = call(&handler, "identities.enable", json!({"identity": "hermes"}));
        assert_eq!(enabled["data"], json!({"identity": "hermes"}));
    }
    for urgent in [true, true, false] {
        let revoked = call(
            &handler,
            "certificates.revoke",
            json!({"serial": serial.as_str(), "urgent": urgent}),
        );
        assert_eq!(
            revoked["data"],
            json!({"serial": serial.as_str(), "reissued": null})
        );
    }
}

#[test]
fn revoking_hecates_certificate_urgently_reissues_it_and_tells_the_api() {
    let (rig, handler, changed) = with_ca();
    let before = changes(&changed);
    let own = own_serial(&handler);
    let passive = call(&handler, "certificates.revoke", json!({"serial": own}));
    assert_eq!(code(&passive), "validation");
    for urgent in [false, true] {
        let disabled = call(
            &handler,
            "identities.disable",
            json!({"identity": "hecate", "urgent": urgent}),
        );
        assert_eq!(code(&disabled), "validation");
    }
    assert_eq!(changes(&changed), before);
    let revoked = call(
        &handler,
        "certificates.revoke",
        json!({"serial": own, "urgent": true}),
    );
    assert_eq!(keys(&revoked["data"]), ["reissued", "serial"]);
    assert_eq!(revoked["data"]["serial"], own.as_str());
    let reissued = revoked["data"]["reissued"].as_str().unwrap().to_string();
    assert_ne!(reissued, own);
    assert_eq!(own_serial(&handler), reissued);
    assert_eq!(changes(&changed), before + 1);
    let row = rows(&rig, Operation::CertificateRevoke).pop().unwrap();
    assert_eq!(
        row,
        (
            AccessResult::Success,
            some("admin"),
            some("hecate"),
            some(&own)
        )
    );
}

#[test]
fn unknown_identities_and_serials_are_not_found() {
    let (rig, handler, _) = with_ca();
    rig.identities.tokens_create("pending").unwrap();
    for identity in ["pending", "nobody"] {
        for op in ["identities.get", "identities.disable", "identities.enable"] {
            let reply = call(&handler, op, json!({"identity": identity}));
            assert_eq!(code(&reply), "not_found", "{op} {identity}");
        }
    }
    let unknown = call(
        &handler,
        "certificates.revoke",
        json!({"serial": ca_serial(0x42).as_str(), "urgent": true}),
    );
    assert_eq!(code(&unknown), "not_found");
    assert_eq!(
        rows(&rig, Operation::IdentityGet)[0],
        (AccessResult::NotFound, some("admin"), some("pending"), None)
    );
}

#[test]
fn malformed_arguments_are_validation_and_leave_their_row() {
    let (rig, handler, _) = with_ca();
    let cases = [
        ("identities.get", json!({}), Operation::IdentityGet),
        (
            "identities.get",
            json!({"identity": "Not Valid"}),
            Operation::IdentityGet,
        ),
        (
            "identities.disable",
            json!({"identity": "hermes", "urgent": "yes"}),
            Operation::IdentityDisable,
        ),
        (
            "identities.enable",
            json!({"identity": 7}),
            Operation::IdentityEnable,
        ),
        (
            "certificates.revoke",
            json!({"serial": "not a serial"}),
            Operation::CertificateRevoke,
        ),
        (
            "certificates.revoke",
            json!({"serial": "aa", "urgent": 1}),
            Operation::CertificateRevoke,
        ),
    ];
    for (op, args, _) in &cases {
        let reply = call(&handler, op, args.clone());
        assert_eq!(code(&reply), "validation", "{op} {args}");
    }
    for operation in [
        Operation::IdentityGet,
        Operation::IdentityDisable,
        Operation::IdentityEnable,
        Operation::CertificateRevoke,
    ] {
        let expected = cases.iter().filter(|(_, _, row)| *row == operation).count();
        let refused = rows(&rig, operation);
        assert_eq!(refused.len(), expected, "{operation:?}");
        assert!(refused.iter().all(|(result, subject, _, _)| *result
            == AccessResult::InvalidInput
            && subject.as_deref() == Some("admin")));
    }
}

#[test]
fn sealed_hecate_answers_conflict() {
    let (rig, handler, _) = with_ca();
    let (hermes, _dir) = enrolled(&rig, "hermes");
    let serial = hermes.serial().unwrap();
    rig.sealing.seal().unwrap();
    for (op, args) in [
        ("identities.list", json!({})),
        ("identities.get", json!({"identity": "hermes"})),
        ("identities.disable", json!({"identity": "hermes"})),
        ("identities.enable", json!({"identity": "hermes"})),
        ("certificates.revoke", json!({"serial": serial.as_str()})),
    ] {
        assert_eq!(code(&call(&handler, op, args)), "conflict", "{op}");
    }
    rig.sealing.unseal_with_passphrase(GOOD_PASSPHRASE).unwrap();
}
