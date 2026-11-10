//! The OpenAPI contracts of `shared/contracts` (research R11): valid OpenAPI 3.1 in JSON, every
//! `$ref` resolved, and the helper that checks the responses of the API.

use hecate_infrastructure::testing::{Contract, contracts_dir};
use serde_json::{Value, json};

const CODES: [&str; 9] = [
    "validation",
    "auth",
    "not_found",
    "conflict",
    "rate_limited",
    "provider_unavailable",
    "timeout",
    "unavailable",
    "internal",
];

fn document(name: &str) -> Value {
    let text = std::fs::read_to_string(contracts_dir().join(name)).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn references(value: &Value, found: &mut Vec<String>) {
    match value {
        Value::Object(members) => {
            for (key, member) in members {
                match (key.as_str(), member) {
                    ("$ref", Value::String(reference)) => found.push(reference.clone()),
                    _ => references(member, found),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|item| references(item, found)),
        _ => {}
    }
}

fn failed(code: &str) -> Value {
    let mut body = json!({
        "status": "failed",
        "data": {},
        "error": {"code": code, "message": "a message", "retryable": false},
        "meta": {"request_id": "req-1", "duration_ms": 3},
    });
    match code {
        "rate_limited" => {
            body["error"]["retryable"] = json!(true);
            body["error"]["retry_after_s"] = json!(60);
        }
        "timeout" => body["error"]["outcome_unknown"] = json!(true),
        "unavailable" | "internal" | "provider_unavailable" => {
            body["error"]["retryable"] = json!(true);
        }
        _ => {}
    }
    body
}

#[test]
fn both_contracts_are_openapi_3_1_in_json() {
    for name in ["common.v1.json", "hecate.v1.json"] {
        let document = document(name);
        assert_eq!(document["openapi"], "3.1.0", "{name}");
        assert_eq!(document["info"]["version"], "1.0.0", "{name}");
        assert_eq!(
            document["info"]["license"]["identifier"],
            "AGPL-3.0-or-later"
        );
    }
}

#[test]
fn every_reference_resolves() {
    let common = document("common.v1.json");
    for (name, own) in [
        ("common.v1.json", &common),
        ("hecate.v1.json", &document("hecate.v1.json")),
    ] {
        let mut found = Vec::new();
        references(own, &mut found);
        assert!(!found.is_empty(), "{name}");
        for reference in found {
            let (file, pointer) = reference.split_once('#').unwrap();
            let target = match file {
                "" => own,
                "common.v1.json" => &common,
                other => panic!("{name}: unexpected file {other}"),
            };
            assert!(target.pointer(pointer).is_some(), "{name}: {reference}");
        }
    }
}

#[test]
fn an_envelope_of_each_of_the_nine_codes_validates() {
    let contract = Contract::load();
    let failed_schema = contract.common_schema("Failed");
    for code in CODES {
        contract
            .check_schema(&failed_schema, &failed(code))
            .unwrap();
    }
    assert!(
        contract
            .check_schema(&failed_schema, &failed("teapot"))
            .is_err()
    );
    let mut extra = failed("auth");
    extra["error"]["stack"] = json!("trace");
    assert!(contract.check_schema(&failed_schema, &extra).is_err());
}

#[test]
fn a_response_is_checked_against_its_operation_and_status() {
    let contract = Contract::load();
    let capabilities = json!({
        "status": "completed",
        "data": {
            "component": "hecate",
            "api": "v1",
            "operations": ["identities:enroll", "certificates:renew", "revocations", "keys",
                           "keys/{kid}", "capabilities"],
        },
        "error": null,
        "meta": {"request_id": "req-1", "duration_ms": 1},
    });
    contract.assert_valid("getCapabilities", 200, &capabilities);
    let mut extra = capabilities.clone();
    extra["data"]["extra"] = json!(1);
    assert!(contract.check("getCapabilities", 200, &extra).is_err());
    contract.assert_valid("getCapabilities", 503, &failed("unavailable"));
    assert!(
        contract
            .check("getCapabilities", 401, &failed("unavailable"))
            .is_err()
    );
    assert!(
        contract
            .check("getCapabilities", 404, &failed("not_found"))
            .is_err()
    );
    assert!(
        contract
            .check("noSuchOperation", 200, &capabilities)
            .is_err()
    );
    contract.assert_valid("enrollIdentity", 429, &failed("rate_limited"));
    let mut without_wait = failed("rate_limited");
    without_wait["error"]
        .as_object_mut()
        .unwrap()
        .remove("retry_after_s");
    assert!(
        contract
            .check("enrollIdentity", 429, &without_wait)
            .is_err()
    );
}
