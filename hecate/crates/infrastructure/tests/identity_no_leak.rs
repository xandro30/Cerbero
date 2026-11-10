//! T113: no secret of the identities leaks (SC-003, FR-024, FR-028), as `no_secret_leak.rs` of the
//! change 005. After a ceremony, enrollments, renewals, revocations and a replacement of the CA,
//! neither the database (with the access log), nor the logs, nor the replies hold, in any of
//! their encodings, the enrollment tokens or their secrets, the CA passphrases, the root CA keys
//! or their copies, the keys of the intermediates or the private keys of the component. Hécate's
//! own key is only in `hecate_tls`, the accepted risk.

mod common;

use std::os::unix::fs::FileTypeExt;
use std::sync::{Arc, Mutex};

use common::*;
use data_encoding::{BASE64, BASE64URL_NOPAD, HEXLOWER, HEXUPPER};
use hecate_application::ports::{Clock, CryptoProvider, IdentityStore};
use hecate_application::testing::{FakeClock, GOOD_PASSPHRASE, MINIMAL_KDF, initialize};
use hecate_domain::identity::names::{RootFingerprint, TrustDomain};
use hecate_domain::pem;
use hecate_infrastructure::api::routes::{refusals, routes};
use hecate_infrastructure::api::server::SealedCheck;
use hecate_infrastructure::api::supervisor::ApiSupervisor;
use hecate_infrastructure::backup::seal_backup;
use hecate_infrastructure::config::HecateConfig;
use hecate_infrastructure::control::operations::{
    IdentityOperations, request_handler_with_identities,
};
use hecate_infrastructure::control::server::RequestHandler;
use hecate_infrastructure::crypto::SoftwareCryptoProvider;
use hecate_infrastructure::logs::render;
use hecate_infrastructure::pki::RootCaKey;
use hecate_infrastructure::testing::{SqliteStack, TestComponent, short_dir};
use serde_json::{Value, json};
use tracing::{Event, Subscriber};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

const SECOND_CA_PASSPHRASE: &str = "harbor violet engine 77";
const WRONG: &str = "a wrong passphrase!!";

enum Secret {
    Text(String),
    Bytes(Vec<u8>),
}

fn text(secret: &str) -> Secret {
    Secret::Text(secret.to_string())
}

/// The complete groups of base64 that a secret leaves inside a longer encoded text, whatever its
/// place: one for each of the three alignments.
fn base64_inside(raw: &[u8]) -> Vec<Vec<u8>> {
    let mut found = Vec::new();
    for skip in 0..3.min(raw.len()) {
        let groups = (raw.len() - skip) / 3 * 3;
        if groups < 18 {
            continue;
        }
        let chunk = &raw[skip..skip + groups];
        found.push(BASE64.encode(chunk).into_bytes());
        found.push(BASE64URL_NOPAD.encode(chunk).into_bytes());
    }
    found
}

fn variants(secret: &Secret) -> Vec<Vec<u8>> {
    let raw = match secret {
        Secret::Text(text) => text.as_bytes().to_vec(),
        Secret::Bytes(bytes) => bytes.clone(),
    };
    let mut variants = vec![
        HEXLOWER.encode(&raw).into_bytes(),
        HEXUPPER.encode(&raw).into_bytes(),
    ];
    if let Secret::Bytes(bytes) = secret {
        variants.extend(base64_inside(bytes));
    }
    variants.push(raw);
    variants
}

fn contains(blob: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && blob.windows(needle.len()).any(|window| window == needle)
}

/// The blob as it is, without line breaks (PEM) and without escaped ones (PEM inside JSON).
fn readings(blob: &[u8]) -> Vec<Vec<u8>> {
    let unwrapped: Vec<u8> = blob
        .iter()
        .copied()
        .filter(|byte| *byte != b'\n' && *byte != b'\r')
        .collect();
    let unescaped = String::from_utf8_lossy(blob)
        .replace("\\n", "")
        .replace("\\r", "")
        .into_bytes();
    vec![blob.to_vec(), unwrapped, unescaped]
}

fn leaks(blob: &[u8], secrets: &[Secret]) -> usize {
    let readings = readings(blob);
    secrets
        .iter()
        .flat_map(variants)
        .filter(|variant| readings.iter().any(|reading| contains(reading, variant)))
        .count()
}

#[test]
fn the_check_itself_detects_a_planted_secret() {
    let key = RootCaKey::generate().unwrap();
    let pkcs8 = key.pkcs8().expose().to_vec();
    let planted = pem::encode("PRIVATE KEY", &[b"?".as_slice(), &pkcs8].concat());
    let in_json = json!({"reply": planted}).to_string();
    for blob in [planted.as_bytes(), in_json.as_bytes()] {
        assert!(leaks(blob, &[Secret::Bytes(pkcs8.clone())]) > 0);
    }
    let token = "HET1.hermes.0123456789abcdef.c2VjcmV0.ab";
    let logged = format!("{{\"detail\":\"{token}\"}}");
    assert!(leaks(logged.as_bytes(), &[text(token)]) > 0);
    assert_eq!(leaks(b"nothing here", &[Secret::Bytes(pkcs8)]), 0);
}

/// Keeps every log line as the JSON lines logger writes it.
#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<String>>>);

impl<S: Subscriber> Layer<S> for Capture {
    fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
        self.0.lock().unwrap().push(render(event));
    }
}

/// One request on the channel, from a blocking thread as the control server runs them.
async fn ask(handler: &Arc<RequestHandler>, op: &'static str, args: Value) -> Value {
    let handler = handler.clone();
    tokio::task::spawn_blocking(move || call(&handler, op, args))
        .await
        .unwrap()
}

/// What the CLI does: a new root CA key, its certificate, the intermediate it signs and the copy.
struct RootSide {
    key: RootCaKey,
    der: Vec<u8>,
    intermediate: Vec<u8>,
    copy: String,
}

fn root_side(clock: &FakeClock, csr: &str, passphrase: &str) -> RootSide {
    let key = RootCaKey::generate().unwrap();
    let der = key
        .root_certificate(
            &TrustDomain::parse(TRUST_DOMAIN).unwrap(),
            &ca_serial(0x9f),
            clock.now(),
        )
        .unwrap();
    let intermediate = key
        .sign_intermediate(&der, csr, &ca_serial(0x9e), clock.now())
        .unwrap();
    let copy = seal_backup(
        &SoftwareCryptoProvider::new(),
        key.pkcs8(),
        passphrase,
        MINIMAL_KDF,
        &RootFingerprint::of_der(&der),
    )
    .unwrap();
    RootSide {
        key,
        der,
        intermediate,
        copy,
    }
}

/// White-box: the keys of the intermediates, decrypted as Hécate does for each signature.
fn intermediate_keys(stack: &SqliteStack) -> Vec<Vec<u8>> {
    let root = stack.sealing.root().unwrap();
    stack
        .identity_store
        .load_ca_material()
        .unwrap()
        .iter()
        .map(|material| {
            let aad = json!({
                "generation": material.generation,
                "purpose": "ca_intermediate",
                "root": RootFingerprint::of_der(&material.root_certificate).to_string(),
                "version": material.intermediate_version,
            })
            .to_string();
            SoftwareCryptoProvider::new()
                .aead_decrypt(
                    &root,
                    &material.key_nonce,
                    &material.key_ciphertext,
                    aad.as_bytes(),
                )
                .unwrap()
                .expose()
                .to_vec()
        })
        .collect()
}

fn own_key(stack: &SqliteStack) -> Vec<u8> {
    stack
        .identity_store
        .load_own_tls_key()
        .unwrap()
        .unwrap()
        .private_key
        .expose()
        .to_vec()
}

/// The private keys that a component keeps in its directory.
fn component_keys(component: &TestComponent) -> Vec<Secret> {
    ["tls-key.pem", "call-key.pem"]
        .iter()
        .map(|name| {
            let text = std::fs::read_to_string(component.path(name)).unwrap();
            Secret::Bytes(pem::single(&text, "PRIVATE KEY").unwrap())
        })
        .collect()
}

fn token_secrets(token: &str) -> Vec<Secret> {
    let secret = token.split('.').nth(3).unwrap();
    vec![
        text(token),
        text(secret),
        Secret::Bytes(BASE64URL_NOPAD.decode(secret.as_bytes()).unwrap()),
    ]
}

fn copy_secrets(copy: &str) -> Vec<Secret> {
    vec![
        text(copy),
        Secret::Bytes(pem::single(copy, "HECATE ROOT CA KEY").unwrap()),
    ]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_token_passphrase_or_key_reaches_the_database_the_logs_or_the_replies() {
    let capture = Capture::default();
    tracing::subscriber::set_global_default(
        tracing_subscriber::registry()
            .with(capture.clone())
            .with(LevelFilter::INFO),
    )
    .unwrap();
    let base = short_dir();
    let config = HecateConfig::for_dir(base.path().join("data"), MINIMAL_KDF);
    config.prepare().unwrap();
    let clock = Arc::new(FakeClock::new());
    let stack = SqliteStack::open(&config.db_path(), clock.clone());
    initialize(&stack.sealing, GOOD_PASSPHRASE);
    stack
        .sealing
        .unseal_with_passphrase(GOOD_PASSPHRASE)
        .unwrap();
    let sealing = stack.sealing.clone();
    let sealed: SealedCheck = Arc::new(move || sealing.unsealed_epoch().is_err());
    let supervisor = Arc::new(ApiSupervisor::new(
        "127.0.0.1:0".parse().unwrap(),
        stack.ca.clone(),
        routes(stack.identities.clone()),
        sealed,
        refusals(stack.identities.clone()),
    ));
    let runtime = tokio::runtime::Handle::current();
    let ticking = supervisor.clone();
    let handler = Arc::new(request_handler_with_identities(
        stack.sealing.clone(),
        None,
        IdentityOperations {
            ca: stack.ca.clone(),
            identities: stack.identities.clone(),
            changed: Arc::new(move || runtime.block_on(ticking.tick()).unwrap()),
        },
    ));
    let mut replies: Vec<String> = Vec::new();
    let mut with_tokens: Vec<String> = Vec::new();
    let mut tokens: Vec<String> = Vec::new();
    let dirs: Vec<tempfile::TempDir> = (0..3).map(|_| tempfile::tempdir().unwrap()).collect();

    // The ceremony, with the root CA key on the side of the CLI.
    let begun = ask(
        &handler,
        "ca.create.begin",
        json!({"trust_domain": TRUST_DOMAIN, "ca_passphrase": CA_PASSPHRASE}),
    )
    .await;
    replies.push(begun.to_string());
    let first = root_side(
        &clock,
        begun["data"]["intermediate_csr"].as_str().unwrap(),
        CA_PASSPHRASE,
    );
    let confirmed = ask(
        &handler,
        "ca.create.confirm",
        json!({
            "ceremony_id": begun["data"]["ceremony_id"],
            "root_certificate": pem::encode("CERTIFICATE", &first.der),
            "intermediate_certificate": pem::encode("CERTIFICATE", &first.intermediate),
        }),
    )
    .await;
    assert_eq!(confirmed["status"], "completed", "{confirmed}");
    replies.push(confirmed.to_string());
    let addr = supervisor.local_addr().await.unwrap();

    // Enrollments, a refused one, a renewal, the queries and an urgent revocation.
    let mut new_token = async |identity: &str| {
        let created = ask(&handler, "tokens.create", json!({"identity": identity})).await;
        with_tokens.push(created.to_string());
        let token = created["data"]["token"].as_str().unwrap().to_string();
        tokens.push(token.clone());
        token
    };
    let hermes = TestComponent::open(dirs[0].path(), clock.clone()).unwrap();
    let token = new_token("hermes").await;
    let hades_token = new_token("hades").await;
    let mut api = Vec::new();
    api.push(hermes.enroll(addr, &token).await.unwrap());
    let hades = TestComponent::open(dirs[1].path(), clock.clone()).unwrap();
    api.push(hades.enroll(addr, &token).await.unwrap());
    api.push(hades.enroll(addr, &hades_token).await.unwrap());
    api.push(hermes.renew(addr, None).await.unwrap());
    api.push(hermes.keys(addr).await.unwrap());
    let revoked = ask(
        &handler,
        "certificates.revoke",
        json!({"serial": hades.serial().unwrap().as_str(), "urgent": true}),
    )
    .await;
    replies.push(revoked.to_string());
    api.push(hermes.revocations(addr).await.unwrap());
    assert_eq!(
        api.iter().map(|reply| reply.status).collect::<Vec<_>>(),
        vec![200, 401, 200, 200, 200, 200]
    );

    // The replacement of the CA: a wrong proof first.
    let refused = ask(
        &handler,
        "ca.replace.begin",
        json!({"proof": {"passphrase": WRONG}, "ca_passphrase": SECOND_CA_PASSPHRASE}),
    )
    .await;
    assert_eq!(code(&refused), "auth");
    replies.push(refused.to_string());
    let mut keys_of_intermediates = intermediate_keys(&stack);
    let previous_own_key = own_key(&stack);
    let begun = ask(
        &handler,
        "ca.replace.begin",
        json!({"proof": {"passphrase": GOOD_PASSPHRASE}, "ca_passphrase": SECOND_CA_PASSPHRASE}),
    )
    .await;
    replies.push(begun.to_string());
    let second = root_side(
        &clock,
        begun["data"]["intermediate_csr"].as_str().unwrap(),
        SECOND_CA_PASSPHRASE,
    );
    let replaced = ask(
        &handler,
        "ca.replace.confirm",
        json!({
            "ceremony_id": begun["data"]["ceremony_id"],
            "root_certificate": pem::encode("CERTIFICATE", &second.der),
            "intermediate_certificate": pem::encode("CERTIFICATE", &second.intermediate),
        }),
    )
    .await;
    assert_eq!(replaced["status"], "completed", "{replaced}");
    replies.push(replaced.to_string());
    keys_of_intermediates.extend(intermediate_keys(&stack));
    let current_own_key = own_key(&stack);
    let again = TestComponent::open(dirs[2].path(), clock.clone()).unwrap();
    let token = new_token("hermes").await;
    let enrolled = again.enroll(addr, &token).await.unwrap();
    assert_eq!(enrolled.status, 200, "{}", enrolled.body);
    api.push(enrolled);
    replies.extend(api.iter().map(|reply| reply.body.to_string()));
    let status = ask(&handler, "status", json!({})).await;
    replies.push(status.to_string());
    supervisor.shutdown().await;
    drop(handler);
    stack.close();

    let mut secrets = vec![
        text(GOOD_PASSPHRASE),
        text(CA_PASSPHRASE),
        text(SECOND_CA_PASSPHRASE),
        text(WRONG),
    ];
    for side in [&first, &second] {
        secrets.push(Secret::Bytes(side.key.pkcs8().expose().to_vec()));
        secrets.extend(copy_secrets(&side.copy));
    }
    secrets.extend(keys_of_intermediates.into_iter().map(Secret::Bytes));
    for component in [&hermes, &hades, &again] {
        secrets.extend(component_keys(component));
    }
    let token_secrets: Vec<Secret> = tokens
        .iter()
        .flat_map(|token| token_secrets(token))
        .collect();
    let previous_own = Secret::Bytes(previous_own_key);
    let current_own = Secret::Bytes(current_own_key);

    // The files of Hécate: its own key, in `hecate_tls` and nowhere else.
    let db = config.db_path();
    for item in std::fs::read_dir(&config.data_dir).unwrap() {
        let item = item.unwrap();
        if item.file_type().unwrap().is_socket() {
            continue;
        }
        let blob = std::fs::read(item.path()).unwrap();
        let name = item.file_name();
        assert_eq!(leaks(&blob, &secrets), 0, "secret found in {name:?}");
        assert_eq!(leaks(&blob, &token_secrets), 0, "token found in {name:?}");
        assert_eq!(
            leaks(&blob, std::slice::from_ref(&previous_own)),
            0,
            "{name:?}"
        );
    }
    assert!(
        leaks(
            &std::fs::read(&db).unwrap(),
            std::slice::from_ref(&current_own)
        ) > 0
    );
    let without = base.path().join("without-hecate-tls.db");
    std::fs::copy(&db, &without).unwrap();
    let connection = rusqlite::Connection::open(&without).unwrap();
    connection
        .execute_batch("PRAGMA secure_delete = ON; DELETE FROM hecate_tls; VACUUM;")
        .unwrap();
    drop(connection);
    assert_eq!(
        leaks(&std::fs::read(&without).unwrap(), &[current_own]),
        0,
        "Hécate's key is only in hecate_tls"
    );

    // The logs and the replies; only the reply of `tokens.create` carries its token, on purpose.
    let lines = capture.0.lock().unwrap().join("\n");
    assert!(!lines.is_empty(), "the requests leave their log lines");
    let mut everything = secrets;
    everything.push(previous_own);
    everything.push(Secret::Bytes(own_key_from_db(&db)));
    assert_eq!(
        leaks(lines.as_bytes(), &everything),
        0,
        "secret found in the logs"
    );
    assert_eq!(
        leaks(lines.as_bytes(), &token_secrets),
        0,
        "token found in the logs"
    );
    let replies = replies.concat();
    assert_eq!(
        leaks(replies.as_bytes(), &everything),
        0,
        "secret found in a reply"
    );
    assert_eq!(
        leaks(replies.as_bytes(), &token_secrets),
        0,
        "token found in a reply"
    );
    assert_eq!(leaks(with_tokens.concat().as_bytes(), &everything), 0);
}

/// Hécate's key as the database keeps it, read after the end of the process.
fn own_key_from_db(db: &std::path::Path) -> Vec<u8> {
    rusqlite::Connection::open(db)
        .unwrap()
        .query_row(
            "SELECT private_key FROM hecate_tls WHERE id = 1",
            [],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .unwrap()
}
