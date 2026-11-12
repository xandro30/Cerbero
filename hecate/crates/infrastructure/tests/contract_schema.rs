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

// --- Feature 006: the resolution and the status (T002) ------------------------------------------

fn completed(data: Value) -> Value {
    json!({
        "status": "completed",
        "data": data,
        "error": null,
        "meta": {"request_id": "req-1", "duration_ms": 251},
    })
}

fn resolved() -> Value {
    let secret = |reference: &str, version: u64| {
        json!({
            "reference": reference,
            "version": version,
            "value": "123456:ABC-DEF",
            "cache_ttl_soft_s": 300,
            "cache_ttl_hard_s": 3600,
        })
    };
    completed(json!({
        "secrets": [
            secret("secret://hermes/telegram/bot_token", 4),
            secret("secret://hades/smtp/password@3", 3),
        ],
    }))
}

fn aggregate_status() -> Value {
    completed(json!({
        "sealed": false,
        "secrets_expired": 0,
        "secrets_expiring_soon": 1,
        "secrets_due_rotation": 2,
        "identities_expiring_24h": 0,
        "suspicious_renewals_24h": 0,
        "intermediate_days_left": null,
        "enrollment_throttled": false,
    }))
}

#[test]
fn a_resolution_request_carries_one_to_a_hundred_references() {
    let contract = Contract::load();
    let schema = contract.hecate_schema("ResolveRequest");
    let request = |count: usize| {
        let references: Vec<String> = (0..count)
            .map(|n| format!("secret://hermes/service/key_{n}"))
            .collect();
        json!({ "references": references })
    };
    for count in [1, 100] {
        contract.check_schema(&schema, &request(count)).unwrap();
    }
    for count in [0, 101] {
        assert!(
            contract.check_schema(&schema, &request(count)).is_err(),
            "{count}"
        );
    }
    let repeated = json!({
        "references": ["secret://hades/smtp/password@3", "secret://hades/smtp/password@3"],
    });
    contract.check_schema(&schema, &repeated).unwrap();
    for wrong in [
        "secret://hermes",
        "secret://Hermes/key",
        "secret://hermes/Key",
        "secret://hermes/key@0",
        "secret://hermes/key@01",
        "secret://hermes/key@1000000000",
        "hermes/key",
    ] {
        let request = json!({ "references": [wrong] });
        assert!(contract.check_schema(&schema, &request).is_err(), "{wrong}");
    }
    let mut extra = request(1);
    extra["idempotency_key"] = json!("k");
    assert!(contract.check_schema(&schema, &extra).is_err());
}

#[test]
fn a_resolution_follows_its_contract() {
    let contract = Contract::load();
    contract.assert_valid("resolveSecrets", 200, &resolved());
    let mut extra = resolved();
    extra["data"]["secrets"][0]["fingerprint"] = json!("a fingerprint");
    assert!(contract.check("resolveSecrets", 200, &extra).is_err());
    let mut missing = resolved();
    missing["data"]["secrets"][1]
        .as_object_mut()
        .unwrap()
        .remove("cache_ttl_hard_s");
    assert!(contract.check("resolveSecrets", 200, &missing).is_err());
    let mut empty = resolved();
    empty["data"]["secrets"] = json!([]);
    assert!(contract.check("resolveSecrets", 200, &empty).is_err());
    for (status, code) in [
        (400, "validation"),
        (401, "auth"),
        (404, "not_found"),
        (500, "internal"),
        (503, "unavailable"),
    ] {
        contract.assert_valid("resolveSecrets", status, &failed(code));
    }
    assert!(
        contract
            .check("resolveSecrets", 409, &failed("conflict"))
            .is_err()
    );
    assert!(
        contract
            .check("resolveSecrets", 429, &failed("rate_limited"))
            .is_err()
    );
}

#[test]
fn a_status_follows_its_contract() {
    let contract = Contract::load();
    contract.assert_valid("getStatus", 200, &aggregate_status());
    let mut sealed = aggregate_status();
    sealed["data"]["sealed"] = json!(true);
    sealed["data"]["intermediate_days_left"] = json!(30);
    contract.assert_valid("getStatus", 200, &sealed);
    let mut beyond = aggregate_status();
    beyond["data"]["intermediate_days_left"] = json!(31);
    assert!(contract.check("getStatus", 200, &beyond).is_err());
    let mut extra = aggregate_status();
    extra["data"]["identities"] = json!(["hermes"]);
    assert!(contract.check("getStatus", 200, &extra).is_err());
    let mut missing = aggregate_status();
    missing["data"]
        .as_object_mut()
        .unwrap()
        .remove("secrets_due_rotation");
    assert!(contract.check("getStatus", 200, &missing).is_err());
    for (status, code) in [(400, "validation"), (401, "auth"), (500, "internal")] {
        contract.assert_valid("getStatus", status, &failed(code));
    }
    assert!(
        contract
            .check("getStatus", 503, &failed("unavailable"))
            .is_err()
    );
    assert!(
        contract
            .check("getStatus", 404, &failed("not_found"))
            .is_err()
    );
}

#[test]
fn the_capabilities_may_list_the_resolution_and_the_status() {
    let contract = Contract::load();
    let capabilities = completed(json!({
        "component": "hecate",
        "api": "v1",
        "operations": ["identities:enroll", "certificates:renew", "revocations", "keys",
                       "keys/{kid}", "secrets:resolve", "status", "capabilities"],
    }));
    contract.assert_valid("getCapabilities", 200, &capabilities);
}
