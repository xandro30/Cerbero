#![allow(dead_code)]

use std::path::PathBuf;

use serde_json::Value;

pub fn vectors_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/vectors")
}

pub fn vectors(name: &str) -> Value {
    let text = std::fs::read_to_string(vectors_dir().join(name)).unwrap();
    serde_json::from_str(&text).unwrap()
}

pub fn hex(value: &Value) -> Vec<u8> {
    data_encoding::HEXLOWER
        .decode(value.as_str().unwrap().as_bytes())
        .unwrap()
}

use std::sync::Arc;

use hecate_application::ports::CryptoProvider;
use hecate_application::sealing::service::SealingService;
use hecate_application::testing::{GOOD_PASSPHRASE, SealingRig, initialize};
use hecate_infrastructure::control::operations::request_handler;
use hecate_infrastructure::control::server::{LineHandler, RequestHandler};
use hecate_infrastructure::crypto::SoftwareCryptoProvider;
use serde_json::json;

pub fn crypto() -> Arc<dyn CryptoProvider> {
    Arc::new(SoftwareCryptoProvider::new())
}

pub fn rig() -> SealingRig {
    SealingRig::new(crypto())
}

/// A handler over an initialized (sealed) service; returns the shares too.
pub fn initialized_handler() -> (SealingRig, RequestHandler, Vec<String>) {
    let rig = rig();
    let shares = initialize(&rig.service, GOOD_PASSPHRASE);
    let handler = request_handler(rig.service.clone(), None);
    (rig, handler, shares)
}

pub fn handler_for(service: Arc<SealingService>) -> RequestHandler {
    request_handler(service, None)
}

/// Sends one request through the handler, as the socket would.
pub fn call(handler: &RequestHandler, op: &str, args: Value) -> Value {
    let raw = json!({"request_id": "r", "op": op, "args": args}).to_string();
    serde_json::from_slice(&handler.handle(raw.as_bytes())).unwrap()
}

pub fn code(reply: &Value) -> &str {
    reply["error"]["code"].as_str().unwrap_or("")
}

use hecate_application::secrets::settings::SecretsSettings;
use hecate_application::testing::SecretsRig;

pub fn secrets_rig(unsealed: bool) -> SecretsRig {
    SecretsRig::new(crypto(), unsealed, SecretsSettings::default())
}

/// The channel with both services, as `hecate serve` builds it.
pub fn secrets_handler(rig: &SecretsRig) -> RequestHandler {
    request_handler(rig.sealing.clone(), Some(rig.service.clone()))
}

pub fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

use std::path::Path;

use hecate_application::testing::FakeClock;
use hecate_infrastructure::testing::SqliteStack;

/// The real adapters over one database file, with a fixed clock.
pub fn stack(db: &Path) -> SqliteStack {
    SqliteStack::open(db, Arc::new(FakeClock::new()))
}

/// A stack initialized and unsealed with the passphrase.
pub fn ready(db: &Path) -> SqliteStack {
    let stack = stack(db);
    initialize(&stack.sealing, GOOD_PASSPHRASE);
    stack
        .sealing
        .unseal_with_passphrase(GOOD_PASSPHRASE)
        .unwrap();
    stack
}

pub fn no_options() -> hecate_application::secrets::service::CreateOptions {
    hecate_application::secrets::service::CreateOptions::default()
}
