//! The component of the tests as a command (quickstart §5 and §6), against the API at
//! `HECATE_API_ADDR` (`127.0.0.1:8443` by default). It keeps its keys and certificates in
//! `--state <dir>`, which only its user reads.
//!
//! - `component enroll --token <token> --state <dir>`: `--asks-for <SPIFFE ID>` makes a new
//!   request that asks for that name, and `--repeat` sends again the last enrollment.
//! - `component renew --state <dir>`: `--from <serial>` shows an older certificate, a second
//!   attempt with it.
//! - `component keys --state <dir>` and `component key <kid> --state <dir>`: the published call
//!   keys.
//! - `component revocations --state <dir>`: the CRLs, which it keeps, with their revoked serials.
//! - `component resolve <reference>... --state <dir>`: the values of its secrets, each with its
//!   version and its cache times.
//! - `component status --state <dir>`: the aggregate status of Hécate, with the status
//!   permission.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use hecate_domain::identity::names::{Serial, SpiffeId};
use hecate_domain::instant::isoformat;
use hecate_infrastructure::clock::SystemClock;
use hecate_infrastructure::testing::{ComponentError, Reply, TestComponent, read_crl};

const USAGE: &str = "usage: component enroll --token <token> --state <dir> [--asks-for <id>] [--repeat]\n       component renew --state <dir> [--from <serial>]\n       component keys --state <dir>\n       component key <kid> --state <dir>\n       component revocations --state <dir>\n       component resolve <reference>... --state <dir>\n       component status --state <dir>";

/// The fields of the aggregate status, in the order of the contract.
const STATUS_FIELDS: [&str; 8] = [
    "sealed",
    "secrets_expired",
    "secrets_expiring_soon",
    "secrets_due_rotation",
    "identities_expiring_24h",
    "suspicious_renewals_24h",
    "intermediate_days_left",
    "enrollment_throttled",
];

fn value_of(args: &[String], flag: &str) -> Option<String> {
    let at = args.iter().position(|arg| arg == flag)?;
    args.get(at + 1).cloned()
}

fn fail(message: impl std::fmt::Display, code: u8) -> ExitCode {
    eprintln!("error: {message}");
    ExitCode::from(code)
}

async fn enroll(
    component: &TestComponent,
    addr: SocketAddr,
    args: &[String],
) -> Result<Reply, ExitCode> {
    let token = value_of(args, "--token").ok_or_else(|| fail(USAGE, 2))?;
    if let Some(name) = value_of(args, "--asks-for") {
        let asked = SpiffeId::parse(&name).map_err(|error| fail(error, 2))?;
        component
            .new_request(Some(&asked))
            .map_err(|error| fail(error, 1))?;
    }
    let sent = if args.iter().any(|arg| arg == "--repeat") {
        component.repeat_last(addr, &token).await
    } else {
        component.enroll(addr, &token).await
    };
    sent.map_err(|error: ComponentError| fail(error, 1))
}

async fn renew(
    component: &TestComponent,
    addr: SocketAddr,
    args: &[String],
) -> Result<Reply, ExitCode> {
    let from = value_of(args, "--from")
        .map(|serial| Serial::parse(&serial))
        .transpose()
        .map_err(|error| fail(error, 2))?;
    component
        .renew(addr, from.as_ref())
        .await
        .map_err(|error| fail(error, 1))
}

/// The arguments after the command that are neither a flag nor the value of one.
fn positional(args: &[String]) -> Vec<&str> {
    let mut found = Vec::new();
    let mut rest = args.iter().skip(1);
    while let Some(arg) = rest.next() {
        if arg.starts_with("--") {
            rest.next();
        } else {
            found.push(arg.as_str());
        }
    }
    found
}

fn text(value: &serde_json::Value) -> &str {
    value.as_str().unwrap_or_default()
}

fn print_key(key: &serde_json::Value) {
    println!(
        "{} {} {} until {}",
        text(&key["kid"]),
        text(&key["cerbero_identity"]),
        text(&key["cerbero_status"]),
        text(&key["cerbero_not_after"])
    );
}

fn print_data(command: &str, data: &serde_json::Value) {
    match command {
        "keys" => {
            for key in data["keys"].as_array().into_iter().flatten() {
                print_key(key);
            }
            println!("expires {}", text(&data["expires_at"]));
        }
        "key" => print_key(&data["key"]),
        "resolve" => {
            for secret in data["secrets"].as_array().into_iter().flatten() {
                println!(
                    "{} version {} cache {}s/{}s: {}",
                    text(&secret["reference"]),
                    secret["version"],
                    secret["cache_ttl_soft_s"],
                    secret["cache_ttl_hard_s"],
                    text(&secret["value"])
                );
            }
        }
        "status" => {
            for field in STATUS_FIELDS {
                println!("{field} {}", data[field]);
            }
        }
        verb => println!(
            "{}ed {} serial {} until {}",
            verb.trim_end_matches('e'),
            text(&data["identity"]),
            text(&data["serial"]),
            text(&data["not_after"])
        ),
    }
}

/// The CRLs that the component keeps: each with its number, its `nextUpdate` and its serials.
fn print_revocations(component: &TestComponent, data: &serde_json::Value) -> ExitCode {
    let crls = match component.crls() {
        Ok(crls) => crls,
        Err(error) => return fail(error, 1),
    };
    for der in &crls {
        let crl = match read_crl(der) {
            Ok(crl) => crl,
            Err(error) => return fail(error, 1),
        };
        let until = crl.next_update.map(isoformat).unwrap_or_default();
        println!("crl {} until {until}", crl.number);
        for serial in &crl.revoked {
            println!("revoked {serial}");
        }
    }
    println!("expires {}", text(&data["expires_at"]));
    ExitCode::SUCCESS
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(state) = value_of(&args, "--state") else {
        return fail(USAGE, 2);
    };
    let addr = std::env::var("HECATE_API_ADDR").unwrap_or_else(|_| "127.0.0.1:8443".into());
    let Ok(addr) = addr.parse::<SocketAddr>() else {
        return fail(
            "HECATE_API_ADDR must be an IP address and a port, such as 127.0.0.1:8443",
            2,
        );
    };
    let component = match TestComponent::open(&PathBuf::from(state), Arc::new(SystemClock)) {
        Ok(component) => component,
        Err(error) => return fail(error, 1),
    };
    let command = args.first().map(String::as_str);
    let sent = match command {
        Some("enroll") => enroll(&component, addr, &args).await,
        Some("renew") => renew(&component, addr, &args).await,
        Some("keys") => component.keys(addr).await.map_err(|error| fail(error, 1)),
        Some("status") => component.status(addr).await.map_err(|error| fail(error, 1)),
        Some("revocations") => component
            .revocations(addr)
            .await
            .map_err(|error| fail(error, 1)),
        Some("key") => match args.get(1) {
            Some(kid) => component
                .key(addr, kid)
                .await
                .map_err(|error| fail(error, 1)),
            None => return fail(USAGE, 2),
        },
        Some("resolve") => match positional(&args).as_slice() {
            [] => return fail(USAGE, 2),
            references => component
                .resolve(addr, references)
                .await
                .map_err(|error| fail(error, 1)),
        },
        _ => return fail(USAGE, 2),
    };
    let reply = match sent {
        Ok(reply) => reply,
        Err(code) => return code,
    };
    if reply.status == 200 {
        if command == Some("revocations") {
            return print_revocations(&component, &reply.body["data"]);
        }
        print_data(command.unwrap_or_default(), &reply.body["data"]);
        return ExitCode::SUCCESS;
    }
    let error = &reply.body["error"];
    eprintln!(
        "error ({}): {}",
        error["code"].as_str().unwrap_or("?"),
        error["message"].as_str().unwrap_or_default()
    );
    ExitCode::from(1)
}
