//! The `hecate` CLI (contracts/cli.md): its commands take a terminal, so the tests drive them with a
//! scripted one (research R15).

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::ops::Deref;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::{Parser, Subcommand};
use hecate_application::access_policy::service::AccessPolicyService;
use hecate_application::ca::service::{CaService, random_serial};
use hecate_application::ca::settings::CaSettings;
use hecate_application::identity::service::IdentityService;
use hecate_application::identity::settings::IdentitySettings;
use hecate_application::ports::{CertificateProfile, PkiProvider};
use hecate_application::sealing::service::{INIT_WARNING, SealingService};
use hecate_application::sealing::settings::SealingSettings;
use hecate_application::secrets::service::SecretsService;
use hecate_application::secrets::settings::SecretsSettings;
use hecate_domain::identity::names::{RootFingerprint, TrustDomain};
use hecate_domain::pem;
use hecate_infrastructure::api::routes::{refusals, routes, sealed_check};
use hecate_infrastructure::api::supervisor::ApiSupervisor;
use hecate_infrastructure::backup::{open_backup, seal_backup};
use hecate_infrastructure::call_token::JwtCallTokenVerifier;
use hecate_infrastructure::clock::SystemClock;
use hecate_infrastructure::config::{ConfigError, HecateConfig};
use hecate_infrastructure::control::client::ControlClient;
use hecate_infrastructure::control::operations::{
    IdentityOperations, request_handler_with_identities,
};
use hecate_infrastructure::control::protocol::erase;
use hecate_infrastructure::control::server::ControlServer;
use hecate_infrastructure::crypto::SoftwareCryptoProvider;
use hecate_infrastructure::logs::configure_logging;
use hecate_infrastructure::pki::{RcgenPki, RootCaKey};
use hecate_infrastructure::process::harden_process;
use hecate_infrastructure::signals::stop_requested;
use hecate_infrastructure::store::OpenError;
use hecate_infrastructure::store::ca::SqliteIdentityStore;
use hecate_infrastructure::store::identity::SqliteEnrollAttempts;
use hecate_infrastructure::store::sealing::{SqliteAccessLog, SqliteSealRepository};
use hecate_infrastructure::store::secrets::SqliteSecretStore;
use hecate_infrastructure::tasks;
use serde_json::{Value, json};
use time::OffsetDateTime;
use zeroize::Zeroizing;

/// The user cancelled a prompt (end of input or Ctrl-C), like `EOFError` or `KeyboardInterrupt`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancelled;

pub trait Terminal {
    /// Asks for a visible line.
    fn ask(&mut self, prompt: &str) -> Result<String, Cancelled>;
    /// Asks for a line without echo.
    fn ask_secret(&mut self, prompt: &str) -> Result<Zeroizing<String>, Cancelled>;
    /// Writes a line to stdout.
    fn out(&mut self, line: &str);
    /// Writes a line to stderr.
    fn err(&mut self, line: &str);
}

#[derive(Debug, Parser)]
#[command(name = "hecate", about = "Hécate: secrets and keys")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run Hécate (always starts sealed)
    Serve,
    /// Initialize Hécate (once)
    Init,
    /// Unseal with the admin passphrase or two recovery shares
    Unseal {
        /// Use two recovery shares instead
        #[arg(long)]
        shares: bool,
    },
    /// Manage the admin passphrase
    Passphrase {
        #[command(subcommand)]
        command: PassphraseCommand,
    },
    /// Manage recovery shares
    Shares {
        #[command(subcommand)]
        command: SharesCommand,
    },
    /// Manage secrets (values are written, never read)
    Secrets {
        #[command(subcommand)]
        command: SecretsCommand,
    },
    /// Manage the CA of Cerbero
    Ca {
        #[command(subcommand)]
        command: CaCommand,
    },
    /// Manage the single-use tokens with which components enroll
    Tokens {
        #[command(subcommand)]
        command: TokensCommand,
    },
    /// Manage the identities of the components
    Identities {
        #[command(subcommand)]
        command: IdentitiesCommand,
    },
    /// Manage the certificates of the components and of Hécate
    Certificates {
        #[command(subcommand)]
        command: CertificatesCommand,
    },
    /// Manage the Access Policy: what each identity reads besides its own secrets
    Policies {
        #[command(subcommand)]
        command: PoliciesCommand,
    },
    /// Seal now: the root leaves memory
    Seal,
    /// Show the seal state
    Status,
}

#[derive(Debug, Subcommand)]
pub enum PassphraseCommand {
    /// Set a new passphrase (needs a proof)
    Change {
        /// Prove with two recovery shares
        #[arg(long)]
        with_shares: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum SharesCommand {
    /// Issue a new set of recovery shares (needs a proof)
    Rekey {
        /// Prove with two current shares
        #[arg(long)]
        with_shares: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum CaCommand {
    /// Create the CA: the root CA key stays with you, the intermediate goes to Hécate
    Create {
        /// The trust domain of the SPIFFE IDs, such as cerbero.home
        #[arg(long)]
        trust_domain: String,
        /// Where to write the copy of the root CA key, protected with the CA passphrase
        #[arg(long)]
        out: PathBuf,
    },
    /// Replace the whole CA: a new root CA and intermediate, with a new CA passphrase
    Replace {
        /// Where to write the copy of the new root CA key, protected with the new CA passphrase
        #[arg(long)]
        out: PathBuf,
        /// Prove with two recovery shares instead of the passphrase
        #[arg(long)]
        with_shares: bool,
    },
    /// Renew the intermediate before it expires, with the copy of the root CA key
    RenewIntermediate {
        /// The copy of the root CA key that `hecate ca create` wrote
        #[arg(long)]
        root_key: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
pub enum TokensCommand {
    /// Create a token for an identity; it is shown only once
    Create { identity: String },
    /// List the pending tokens, without their value
    List,
    /// Annul a pending token
    Annul { token_id: String },
}

#[derive(Debug, Subcommand)]
pub enum IdentitiesCommand {
    /// List the enrolled identities with the end of their latest certificate
    List,
    /// Show an identity and its certificates, without keys
    Get { identity: String },
    /// Disable an identity: no renewals, no call keys, no tokens
    Disable {
        identity: String,
        /// Also revoke its valid certificates at once: they enter the revocation list
        #[arg(long)]
        urgent: bool,
    },
    /// Enable a disabled identity: it enrolls again with a new token
    Enable { identity: String },
}

#[derive(Debug, Subcommand)]
pub enum CertificatesCommand {
    /// Revoke a certificate: by default it only stops renewing
    Revoke {
        serial: String,
        /// Revoke it at once: it enters the revocation list and its call key stops signing
        #[arg(long)]
        urgent: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum PoliciesCommand {
    /// Let an identity read one secret of another identity
    GrantRead { identity: String, secret: String },
    /// Stop an identity from reading a secret of another identity
    RevokeRead { identity: String, secret: String },
    /// Let an identity read the aggregate status of Hécate
    GrantStatus { identity: String },
    /// Stop an identity from reading the aggregate status of Hécate
    RevokeStatus { identity: String },
    /// List the identities whose policy has changed
    List,
    /// Show the policy of an identity and its history
    Get { identity: String },
}

#[derive(Debug, Subcommand)]
pub enum SecretsCommand {
    /// Store a new secret (version 1)
    Create {
        /// secret://<identity>/<path>
        path: String,
        /// Type of the value
        #[arg(long = "type", value_parser = ["string", "json", "pem"])]
        value_type: String,
        /// Read the value from a file instead of typing it
        #[arg(long)]
        from_file: Option<PathBuf>,
        /// Versions kept, from 2 to 100 (default 10)
        #[arg(long, allow_negative_numbers = true)]
        max_versions: Option<i64>,
        /// Rotation period, an ISO 8601 duration in days or weeks such as P90D or P12W
        #[arg(long)]
        rotate_every: Option<String>,
        /// Informative expiry, ISO 8601
        #[arg(long)]
        expires_at: Option<String>,
    },
    /// List secret paths with their metadata
    List {
        /// Only the secrets of this identity
        #[arg(long)]
        identity: Option<String>,
    },
    /// Show a secret's metadata and versions (never the value)
    Get { path: String },
    /// Add a new version to a secret
    Put {
        path: String,
        /// Read the value from a file instead of typing it
        #[arg(long)]
        from_file: Option<PathBuf>,
    },
    /// Revoke a version at once
    Revoke {
        path: String,
        #[arg(long, allow_negative_numbers = true)]
        version: i64,
    },
    /// Restore a revoked version (it becomes deprecated)
    Restore {
        path: String,
        #[arg(long, allow_negative_numbers = true)]
        version: i64,
    },
    /// Make a deprecated version the active one
    Promote {
        path: String,
        #[arg(long, allow_negative_numbers = true)]
        version: i64,
    },
    /// Destroy a revoked version for good (needs a proof)
    Destroy {
        path: String,
        #[arg(long, allow_negative_numbers = true)]
        version: i64,
        /// Prove with two recovery shares
        #[arg(long)]
        with_shares: bool,
    },
}

/// Runs a parsed command: a configuration error ends with 2, like the Python version.
pub fn run(
    cli: Cli,
    config: Result<HecateConfig, ConfigError>,
    terminal: &mut dyn Terminal,
) -> i32 {
    let config = match config {
        Ok(config) => config,
        Err(error) => {
            terminal.err(&format!("error: {error}"));
            return 2;
        }
    };
    execute(cli.command, &config, terminal)
}

/// Runs one command; a prompt cancelled outside the share confirmation ends it with 1.
pub fn execute(command: Command, config: &HecateConfig, terminal: &mut dyn Terminal) -> i32 {
    let outcome = match command {
        Command::Serve => Ok(serve(config, terminal)),
        Command::Init => init(config, terminal),
        Command::Unseal { shares } => unseal(config, terminal, shares),
        Command::Passphrase {
            command: PassphraseCommand::Change { with_shares },
        } => passphrase_change(config, terminal, with_shares),
        Command::Shares {
            command: SharesCommand::Rekey { with_shares },
        } => shares_rekey(config, terminal, with_shares),
        Command::Ca {
            command: CaCommand::Create { trust_domain, out },
        } => ca_create(config, terminal, &trust_domain, &out),
        Command::Ca {
            command: CaCommand::RenewIntermediate { root_key },
        } => ca_renew_intermediate(config, terminal, &root_key),
        Command::Ca {
            command: CaCommand::Replace { out, with_shares },
        } => ca_replace(config, terminal, &out, with_shares),
        Command::Tokens { command } => Ok(tokens(config, terminal, command)),
        Command::Identities { command } => Ok(identities(config, terminal, command)),
        Command::Certificates {
            command: CertificatesCommand::Revoke { serial, urgent },
        } => Ok(certificates_revoke(config, terminal, &serial, urgent)),
        Command::Policies { command } => Ok(policies(config, terminal, command)),
        Command::Seal => Ok(seal(config, terminal)),
        Command::Status => Ok(status(config, terminal)),
        Command::Secrets { command } => secrets(config, terminal, command),
    };
    outcome.unwrap_or_else(|Cancelled| {
        terminal.out("cancelled");
        1
    })
}

fn field<'a>(data: &'a Value, key: &str) -> &'a Value {
    data.get(key).unwrap_or(&Value::Null)
}

/// JSON that is zeroed when dropped: the requests and replies of the CLI carry passphrases,
/// shares and secret values.
pub struct ErasedJson(Value);

impl Deref for ErasedJson {
    type Target = Value;

    fn deref(&self) -> &Value {
        &self.0
    }
}

impl Drop for ErasedJson {
    fn drop(&mut self) {
        erase(&mut self.0);
    }
}

/// A value as Python's f-strings print it: a text without quotes, anything else as JSON.
fn shown(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// Python's truth: `false`, `null`, zero and empty values are false.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_some_and(|number| number != 0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(fields) => !fields.is_empty(),
    }
}

fn texts(shares: &[Zeroizing<String>]) -> Value {
    shares
        .iter()
        .map(|share| Value::from(share.as_str()))
        .collect()
}

fn status(config: &HecateConfig, terminal: &mut dyn Terminal) -> i32 {
    let Some(data) = call(config, terminal, "status", json!({})) else {
        return 1;
    };
    terminal.out(&format!("state: {}", shown(field(&data, "state"))));
    if truthy(field(&data, "recommend_passphrase_change")) {
        terminal.out("recommendation: unsealed with recovery shares; set a new passphrase");
    }
    let failures = field(&data, "consecutive_failures");
    if truthy(failures) {
        terminal.out(&format!("failed attempts in a row: {}", shown(failures)));
    }
    let wait = field(&data, "retry_after_s");
    if truthy(wait) {
        terminal.out(&format!("next attempt allowed in {}s", shown(wait)));
    }
    if let Some(expired) = data.get("secrets_expired") {
        terminal.out(&format!("secrets expired: {}", shown(expired)));
        terminal.out(&format!(
            "secrets expiring in the next 7 days: {}",
            shown(field(&data, "secrets_expiring_soon"))
        ));
        terminal.out(&format!(
            "secrets due rotation: {}",
            shown(field(&data, "secrets_due_rotation"))
        ));
    }
    if let Some(ca) = data.get("ca") {
        if !truthy(field(ca, "exists")) {
            terminal.out("ca: none");
            return 0;
        }
        terminal.out(&format!(
            "ca: {} {}",
            shown(field(ca, "trust_domain")),
            shown(field(ca, "root_fingerprint"))
        ));
        let own = field(ca, "hecate_certificate");
        if !own.is_null() {
            terminal.out(&format!(
                "hecate certificate {} until {}",
                shown(field(own, "serial")),
                shown(field(own, "not_after"))
            ));
        }
        let days_left = field(ca, "intermediate_days_left");
        if !days_left.is_null() {
            terminal.out(&format!("intermediate days left: {}", shown(days_left)));
        }
        if let Some(expiring) = ca.get("identities_expiring_24h") {
            terminal.out(&format!(
                "identities expiring within 24h: {}",
                shown(expiring)
            ));
        }
        if let Some(suspicious) = ca.get("suspicious_renewals_24h") {
            terminal.out(&format!(
                "suspicious renewals within 24h: {}",
                shown(suspicious)
            ));
        }
        if let Some(throttled) = ca.get("enrollment_throttled") {
            let answer = if truthy(throttled) { "yes" } else { "no" };
            terminal.out(&format!("enrollment throttled: {answer}"));
        }
    }
    0
}

/// `hecate tokens` (contracts/cli.md): the token is printed once and never again.
fn tokens(config: &HecateConfig, terminal: &mut dyn Terminal, command: TokensCommand) -> i32 {
    match command {
        TokensCommand::Create { identity } => {
            let Some(data) = call(
                config,
                terminal,
                "tokens.create",
                json!({"identity": identity}),
            ) else {
                return 1;
            };
            // Written from a buffer that is zeroed when dropped, like the shares.
            let token = field(&data, "token").as_str().unwrap_or_default();
            let mut line = Zeroizing::new(String::with_capacity(6 + token.len()));
            line.push_str("token ");
            line.push_str(token);
            terminal.out(&line);
            terminal.out(&format!(
                "id {} identity {} expires {}",
                shown(field(&data, "token_id")),
                shown(field(&data, "identity")),
                shown(field(&data, "expires_at"))
            ));
            terminal.out("this token is shown only once");
        }
        TokensCommand::List => {
            let Some(data) = call(config, terminal, "tokens.list", json!({})) else {
                return 1;
            };
            let pending = field(&data, "tokens")
                .as_array()
                .map_or(&[][..], Vec::as_slice);
            if pending.is_empty() {
                terminal.out("no pending tokens");
            }
            for token in pending {
                terminal.out(&format!(
                    "{} {} expires {}",
                    shown(field(token, "token_id")),
                    shown(field(token, "identity")),
                    shown(field(token, "expires_at"))
                ));
            }
        }
        TokensCommand::Annul { token_id } => {
            let Some(data) = call(
                config,
                terminal,
                "tokens.annul",
                json!({"token_id": token_id}),
            ) else {
                return 1;
            };
            terminal.out(&format!("annulled {}", shown(field(&data, "token_id"))));
        }
    }
    0
}

fn items(data: &Value, key: &str) -> Vec<Value> {
    field(data, key).as_array().cloned().unwrap_or_default()
}

/// `hecate identities` (contracts/cli.md).
fn identities(
    config: &HecateConfig,
    terminal: &mut dyn Terminal,
    command: IdentitiesCommand,
) -> i32 {
    match command {
        IdentitiesCommand::List => {
            let Some(data) = call(config, terminal, "identities.list", json!({})) else {
                return 1;
            };
            let listed = items(&data, "identities");
            if listed.is_empty() {
                terminal.out("no identities");
            }
            for identity in &listed {
                terminal.out(&format!(
                    "{} {} certificate until {}",
                    shown(field(identity, "identity")),
                    shown(field(identity, "status")),
                    shown(field(identity, "certificate_not_after"))
                ));
            }
        }
        IdentitiesCommand::Get { identity } => {
            let Some(data) = call(
                config,
                terminal,
                "identities.get",
                json!({"identity": identity}),
            ) else {
                return 1;
            };
            terminal.out(&format!(
                "identity {} {}",
                shown(field(&data, "identity")),
                shown(field(&data, "status"))
            ));
            for certificate in &items(&data, "certificates") {
                let renewable = if truthy(field(certificate, "renewable")) {
                    "yes"
                } else {
                    "no"
                };
                terminal.out(&format!(
                    "{} {} renewable {renewable} {} {}",
                    shown(field(certificate, "serial")),
                    shown(field(certificate, "state")),
                    shown(field(certificate, "not_before")),
                    shown(field(certificate, "not_after"))
                ));
            }
        }
        IdentitiesCommand::Disable { identity, urgent } => {
            let Some(data) = call(
                config,
                terminal,
                "identities.disable",
                json!({"identity": identity, "urgent": urgent}),
            ) else {
                return 1;
            };
            terminal.out(&format!("disabled {}", shown(field(&data, "identity"))));
            if urgent {
                let revoked = items(&data, "revoked").len();
                terminal.out(&format!("revoked {revoked} certificates"));
            }
        }
        IdentitiesCommand::Enable { identity } => {
            let Some(data) = call(
                config,
                terminal,
                "identities.enable",
                json!({"identity": identity}),
            ) else {
                return 1;
            };
            terminal.out(&format!("enabled {}", shown(field(&data, "identity"))));
        }
    }
    0
}

fn yes_no(value: &Value) -> &'static str {
    if truthy(value) { "yes" } else { "no" }
}

/// The line that an order of the Access Policy prints, from the data of its reply.
type OrderLine = fn(&Value) -> String;

/// An order of the Access Policy: the line of what it changed, or of `unchanged`.
fn policy_order(
    config: &HecateConfig,
    terminal: &mut dyn Terminal,
    op: &str,
    args: Value,
    lines: (OrderLine, OrderLine),
) -> i32 {
    let Some(data) = call(config, terminal, op, args) else {
        return 1;
    };
    let (changed, unchanged) = lines;
    let line = if truthy(field(&data, "changed")) {
        changed(&data)
    } else {
        unchanged(&data)
    };
    terminal.out(&line);
    0
}

/// `hecate policies` (contracts/cli.md): the orders and queries of the admin on the Access
/// Policy.
fn policies(config: &HecateConfig, terminal: &mut dyn Terminal, command: PoliciesCommand) -> i32 {
    let text = |data: &Value, key: &str| shown(field(data, key));
    match command {
        PoliciesCommand::GrantRead { identity, secret } => policy_order(
            config,
            terminal,
            "policies.grant_read",
            json!({"identity": identity, "secret": secret}),
            (
                |data| {
                    format!(
                        "granted read {} to {} (policy version {})",
                        shown(field(data, "secret")),
                        shown(field(data, "identity")),
                        shown(field(data, "version"))
                    )
                },
                |data| {
                    format!(
                        "unchanged: {} already reads {}",
                        shown(field(data, "identity")),
                        shown(field(data, "secret"))
                    )
                },
            ),
        ),
        PoliciesCommand::RevokeRead { identity, secret } => policy_order(
            config,
            terminal,
            "policies.revoke_read",
            json!({"identity": identity, "secret": secret}),
            (
                |data| {
                    format!(
                        "revoked read {} from {} (policy version {})",
                        shown(field(data, "secret")),
                        shown(field(data, "identity")),
                        shown(field(data, "version"))
                    )
                },
                |data| {
                    format!(
                        "unchanged: {} does not read {}",
                        shown(field(data, "identity")),
                        shown(field(data, "secret"))
                    )
                },
            ),
        ),
        PoliciesCommand::GrantStatus { identity } => policy_order(
            config,
            terminal,
            "policies.grant_status",
            json!({"identity": identity}),
            (
                |data| {
                    format!(
                        "granted status to {} (policy version {})",
                        shown(field(data, "identity")),
                        shown(field(data, "version"))
                    )
                },
                |data| {
                    format!(
                        "unchanged: {} already has status",
                        shown(field(data, "identity"))
                    )
                },
            ),
        ),
        PoliciesCommand::RevokeStatus { identity } => policy_order(
            config,
            terminal,
            "policies.revoke_status",
            json!({"identity": identity}),
            (
                |data| {
                    format!(
                        "revoked status from {} (policy version {})",
                        shown(field(data, "identity")),
                        shown(field(data, "version"))
                    )
                },
                |data| {
                    format!(
                        "unchanged: {} has no status",
                        shown(field(data, "identity"))
                    )
                },
            ),
        ),
        PoliciesCommand::List => {
            let Some(data) = call(config, terminal, "policies.list", json!({})) else {
                return 1;
            };
            let listed = items(&data, "policies");
            if listed.is_empty() {
                terminal.out("no policies");
            }
            for policy in &listed {
                terminal.out(&format!(
                    "{} version {} status {} reads {}",
                    text(policy, "identity"),
                    text(policy, "version"),
                    yes_no(field(policy, "status")),
                    items(policy, "reads").len()
                ));
            }
            0
        }
        PoliciesCommand::Get { identity } => {
            let Some(data) = call(
                config,
                terminal,
                "policies.get",
                json!({"identity": identity}),
            ) else {
                return 1;
            };
            terminal.out(&format!(
                "policy {} version {} status {}",
                text(&data, "identity"),
                text(&data, "version"),
                yes_no(field(&data, "status"))
            ));
            for secret in &items(&data, "reads") {
                terminal.out(&format!("read {}", shown(secret)));
            }
            let history = items(&data, "history");
            if !history.is_empty() {
                terminal.out("history");
            }
            for change in &history {
                let secret = field(change, "secret");
                let secret = if secret.is_null() {
                    String::new()
                } else {
                    format!(" {}", shown(secret))
                };
                terminal.out(&format!(
                    "{} {} {}{secret} by {}",
                    text(change, "version"),
                    text(change, "at"),
                    text(change, "change"),
                    text(change, "by")
                ));
            }
            0
        }
    }
}

/// `hecate certificates revoke` (contracts/cli.md): Hécate's certificate comes back replaced.
fn certificates_revoke(
    config: &HecateConfig,
    terminal: &mut dyn Terminal,
    serial: &str,
    urgent: bool,
) -> i32 {
    let Some(data) = call(
        config,
        terminal,
        "certificates.revoke",
        json!({"serial": serial, "urgent": urgent}),
    ) else {
        return 1;
    };
    let urgency = if urgent { " urgent" } else { "" };
    terminal.out(&format!(
        "revoked {}{urgency}",
        shown(field(&data, "serial"))
    ));
    let reissued = field(&data, "reissued");
    if !reissued.is_null() {
        terminal.out(&format!("reissued {}", shown(reissued)));
    }
    0
}

const CA_PASSPHRASE_PROMPT: &str = "CA passphrase (nothing is echoed): ";
const COPY_DOES_NOT_OPEN: &str = "error: the saved copy does not open with that passphrase";

/// The operations that end a ceremony that creates a root CA: the creation or the replacement.
struct RootCeremony {
    confirm: &'static str,
    abort: &'static str,
}

const CREATE: RootCeremony = RootCeremony {
    confirm: "ca.create.confirm",
    abort: "ca.create.abort",
};
const REPLACE: RootCeremony = RootCeremony {
    confirm: "ca.replace.confirm",
    abort: "ca.replace.abort",
};

/// A ceremony of the CA that Hécate keeps pending: aborting it leaves its reason in the access
/// log, and the copy that this run wrote goes too, since its CA will not exist.
struct PendingCeremony<'a> {
    config: &'a HecateConfig,
    abort_op: &'static str,
    ceremony_id: Value,
    out: &'a Path,
    written: bool,
}

impl PendingCeremony<'_> {
    fn abort(&self, terminal: &mut dyn Terminal, reason: &str) {
        if self.written {
            let _ = fs::remove_file(self.out);
        }
        call(
            self.config,
            terminal,
            self.abort_op,
            json!({"ceremony_id": self.ceremony_id, "reason": reason}),
        );
    }
}

/// Writes the copy in a new file that only the user can read; an existing file is never replaced.
fn write_copy(out: &Path, copy: &str) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(out)?;
    file.write_all(copy.as_bytes())?;
    file.sync_all()
}

/// Whether the copy at `path` opens with the passphrase and holds the key of the root CA.
fn copy_opens(
    crypto: &SoftwareCryptoProvider,
    path: &str,
    passphrase: &str,
    root_key: &RootCaKey,
    fingerprint: &RootFingerprint,
) -> Result<bool, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("error: cannot read {path}: {}", unreadable(&error)))?;
    let Ok(pkcs8) = open_backup(crypto, &text, passphrase, fingerprint) else {
        return Ok(false);
    };
    let opened = RootCaKey::from_pkcs8(pkcs8).and_then(|key| key.public_key());
    Ok(matches!((opened, root_key.public_key()), (Ok(opened), Ok(root)) if opened == root))
}

/// The root CA and the intermediate that Hécate asks for, signed with a new root CA key.
fn sign_ca(
    trust_domain: &str,
    request: &str,
    crypto: &SoftwareCryptoProvider,
) -> Result<(RootCaKey, Vec<u8>, Vec<u8>), String> {
    let failed = |error: hecate_application::ports::PkiError| format!("error: {error}");
    let trust_domain =
        TrustDomain::parse(trust_domain).map_err(|error| format!("error: {error}"))?;
    let now = OffsetDateTime::now_utc();
    let root_key = RootCaKey::generate().map_err(failed)?;
    let root = root_key
        .root_certificate(&trust_domain, &random_serial(crypto), now)
        .map_err(failed)?;
    let intermediate = root_key
        .sign_intermediate(&root, request, &random_serial(crypto), now)
        .map_err(failed)?;
    Ok((root_key, root, intermediate))
}

fn refuse_existing(terminal: &mut dyn Terminal, out: &Path) -> bool {
    let exists = fs::symlink_metadata(out).is_ok();
    if exists {
        terminal.err(&format!(
            "error: {} already exists; it is not overwritten",
            out.display()
        ));
    }
    exists
}

/// What `ca create` and `ca replace` do after `begin`: a new root CA signs the intermediate, its
/// copy is written and a saved copy is checked with the passphrase before confirming. Returns the
/// data of the confirmation, or `None` once the error is printed and the ceremony aborted.
fn root_ceremony(
    config: &HecateConfig,
    terminal: &mut dyn Terminal,
    ops: &RootCeremony,
    (begun, trust_domain): (&Value, &str),
    passphrase: Zeroizing<String>,
    out: &Path,
) -> Result<Option<ErasedJson>, Cancelled> {
    let mut ceremony = PendingCeremony {
        config,
        abort_op: ops.abort,
        ceremony_id: field(begun, "ceremony_id").clone(),
        out,
        written: false,
    };
    let crypto = SoftwareCryptoProvider::new();
    let request = shown(field(begun, "intermediate_csr"));
    let (root_key, root, intermediate) = match sign_ca(trust_domain, &request, &crypto) {
        Ok(signed) => signed,
        Err(message) => {
            terminal.err(&message);
            ceremony.abort(terminal, "cancelled");
            return Ok(None);
        }
    };
    let fingerprint = RootFingerprint::of_der(&root);
    let written = seal_backup(
        &crypto,
        root_key.pkcs8(),
        &passphrase,
        config.kdf,
        &fingerprint,
    )
    .map_err(|error| error.to_string())
    .and_then(|copy| {
        write_copy(out, &copy)
            .map_err(|error| format!("cannot write {}: {}", out.display(), unreadable(&error)))
    });
    drop(passphrase);
    if let Err(message) = written {
        terminal.err(&format!("error: {message}"));
        ceremony.abort(terminal, "cancelled");
        return Ok(None);
    }
    ceremony.written = true;
    terminal.out(&format!(
        "root key saved to {}; store it on two separate media",
        out.display()
    ));
    let answers = terminal
        .ask("path of a saved copy to verify: ")
        .and_then(|path| Ok((path, terminal.ask_secret(CA_PASSPHRASE_PROMPT)?)));
    let (path, again) = match answers {
        Ok(answers) => answers,
        Err(Cancelled) => {
            ceremony.abort(terminal, "cancelled");
            return Err(Cancelled);
        }
    };
    match copy_opens(&crypto, path.trim(), &again, &root_key, &fingerprint) {
        Ok(true) => {}
        Ok(false) => {
            terminal.err(COPY_DOES_NOT_OPEN);
            ceremony.abort(terminal, "backup_check_failed");
            return Ok(None);
        }
        Err(message) => {
            terminal.err(&message);
            ceremony.abort(terminal, "backup_check_failed");
            return Ok(None);
        }
    }
    drop(again);
    let confirmed = call(
        config,
        terminal,
        ops.confirm,
        json!({
            "ceremony_id": ceremony.ceremony_id,
            "root_certificate": pem::encode("CERTIFICATE", &root),
            "intermediate_certificate": pem::encode("CERTIFICATE", &intermediate),
        }),
    );
    if confirmed.is_none() {
        ceremony.abort(terminal, "cancelled");
    }
    Ok(confirmed)
}

/// `hecate ca create` (contracts/cli.md): the root CA key never leaves this process, Hécate only
/// receives certificates, and the CA exists only once a saved copy opens with the passphrase.
fn ca_create(
    config: &HecateConfig,
    terminal: &mut dyn Terminal,
    trust_domain: &str,
    out: &Path,
) -> Result<i32, Cancelled> {
    if refuse_existing(terminal, out) {
        return Ok(1);
    }
    let passphrase = terminal.ask_secret(CA_PASSPHRASE_PROMPT)?;
    let Some(begun) = call(
        config,
        terminal,
        "ca.create.begin",
        json!({"trust_domain": trust_domain, "ca_passphrase": passphrase.as_str()}),
    ) else {
        return Ok(1);
    };
    let Some(created) = root_ceremony(
        config,
        terminal,
        &CREATE,
        (&begun, trust_domain),
        passphrase,
        out,
    )?
    else {
        return Ok(1);
    };
    terminal.out(&format!(
        "created ca for {}",
        shown(field(&created, "trust_domain"))
    ));
    terminal.out(&format!(
        "root fingerprint {}",
        shown(field(&created, "root_fingerprint"))
    ));
    Ok(0)
}

/// The trust domain that the request of the new intermediate names.
fn trust_domain_of(request: &str) -> Result<String, String> {
    let read = RcgenPki::new()
        .read_request(request)
        .map_err(|error| format!("error: {error}"))?;
    match read
        .common_name
        .as_deref()
        .and_then(CertificateProfile::intermediate_named)
    {
        Some(CertificateProfile::Intermediate { trust_domain, .. }) => Ok(trust_domain.to_string()),
        _ => Err("error: the request is not for an intermediate".to_string()),
    }
}

/// `hecate ca replace` (contracts/cli.md): a proof and the new CA passphrase, then the same
/// exchange as `ca create`; the trust domain stays the one of the CA.
fn ca_replace(
    config: &HecateConfig,
    terminal: &mut dyn Terminal,
    out: &Path,
    with_shares: bool,
) -> Result<i32, Cancelled> {
    if refuse_existing(terminal, out) {
        return Ok(1);
    }
    let proof = ask_proof(terminal, with_shares)?;
    let passphrase = terminal.ask_secret(CA_PASSPHRASE_PROMPT)?;
    let begun = call(
        config,
        terminal,
        "ca.replace.begin",
        json!({"proof": &*proof, "ca_passphrase": passphrase.as_str()}),
    );
    drop(proof);
    let Some(begun) = begun else {
        return Ok(1);
    };
    let trust_domain = match trust_domain_of(&shown(field(&begun, "intermediate_csr"))) {
        Ok(trust_domain) => trust_domain,
        Err(message) => {
            terminal.err(&message);
            call(
                config,
                terminal,
                REPLACE.abort,
                json!({"ceremony_id": field(&begun, "ceremony_id"), "reason": "cancelled"}),
            );
            return Ok(1);
        }
    };
    let Some(replaced) = root_ceremony(
        config,
        terminal,
        &REPLACE,
        (&begun, &trust_domain),
        passphrase,
        out,
    )?
    else {
        return Ok(1);
    };
    terminal.out(&format!(
        "replaced ca for {} generation {}",
        shown(field(&replaced, "trust_domain")),
        shown(field(&replaced, "generation"))
    ));
    terminal.out(&format!(
        "root fingerprint {}",
        shown(field(&replaced, "root_fingerprint"))
    ));
    Ok(0)
}

const RENEWAL_DOES_NOT_OPEN: &str =
    "error: the copy does not open: wrong passphrase or the copy of another CA";
const NOT_THE_ROOT_KEY: &str = "error: the copy does not hold the key of the root CA of Hécate";

/// Discards the pending renewal of the intermediate, leaving its reason in the access log.
fn abort_renewal(
    config: &HecateConfig,
    terminal: &mut dyn Terminal,
    ceremony_id: &Value,
    reason: &str,
) {
    call(
        config,
        terminal,
        "ca.intermediate.abort",
        json!({"ceremony_id": ceremony_id, "reason": reason}),
    );
}

/// The new intermediate signed with the root CA key of the copy, or the reason of the abort and
/// what to print. A copy that does not open does not say whether the passphrase is wrong or it
/// is the copy of another CA (contracts/formats.md).
fn sign_renewal(
    crypto: &SoftwareCryptoProvider,
    copy: &str,
    passphrase: &str,
    begun: &Value,
) -> Result<Vec<u8>, (&'static str, String)> {
    let root = pem::single(&shown(field(begun, "root_certificate")), "CERTIFICATE")
        .map_err(|error| ("cancelled", format!("error: {error}")))?;
    let pkcs8 = open_backup(crypto, copy, passphrase, &RootFingerprint::of_der(&root))
        .map_err(|_| ("wrong_passphrase", RENEWAL_DOES_NOT_OPEN.to_string()))?;
    let wrong_key = || ("wrong_root_key", NOT_THE_ROOT_KEY.to_string());
    let key = RootCaKey::from_pkcs8(pkcs8).map_err(|_| wrong_key())?;
    let root_info = RcgenPki::new()
        .read_certificate(&root)
        .map_err(|error| ("cancelled", format!("error: {error}")))?;
    if key.public_key().map_err(|_| wrong_key())? != root_info.public_key {
        return Err(wrong_key());
    }
    key.sign_intermediate(
        &root,
        &shown(field(begun, "intermediate_csr")),
        &random_serial(crypto),
        OffsetDateTime::now_utc(),
    )
    .map_err(|error| ("cancelled", format!("error: {error}")))
}

/// `hecate ca renew-intermediate` (contracts/cli.md): the copy of the root CA key opens only in
/// this process, signs the new intermediate and is erased; Hécate only receives the certificate.
fn ca_renew_intermediate(
    config: &HecateConfig,
    terminal: &mut dyn Terminal,
    root_key: &Path,
) -> Result<i32, Cancelled> {
    let copy = match fs::read_to_string(root_key) {
        Ok(text) => Zeroizing::new(text),
        Err(error) => {
            terminal.err(&format!(
                "error: cannot read {}: {}",
                root_key.display(),
                unreadable(&error)
            ));
            return Ok(1);
        }
    };
    let Some(begun) = call(config, terminal, "ca.intermediate.begin", json!({})) else {
        return Ok(1);
    };
    let ceremony_id = field(&begun, "ceremony_id").clone();
    let passphrase = match terminal.ask_secret(CA_PASSPHRASE_PROMPT) {
        Ok(passphrase) => passphrase,
        Err(Cancelled) => {
            abort_renewal(config, terminal, &ceremony_id, "cancelled");
            return Err(Cancelled);
        }
    };
    let signed = sign_renewal(&SoftwareCryptoProvider::new(), &copy, &passphrase, &begun);
    drop(passphrase);
    let certificate = match signed {
        Ok(certificate) => certificate,
        Err((reason, message)) => {
            terminal.err(&message);
            abort_renewal(config, terminal, &ceremony_id, reason);
            return Ok(1);
        }
    };
    let Some(renewed) = call(
        config,
        terminal,
        "ca.intermediate.confirm",
        json!({
            "ceremony_id": ceremony_id,
            "intermediate_certificate": pem::encode("CERTIFICATE", &certificate),
        }),
    ) else {
        abort_renewal(config, terminal, &ceremony_id, "cancelled");
        return Ok(1);
    };
    terminal.out(&format!(
        "renewed intermediate version {} until {}",
        shown(field(&renewed, "intermediate_version")),
        shown(field(&renewed, "intermediate_not_after"))
    ));
    Ok(0)
}

/// Why a file cannot be read, in words that do not depend on the platform.
fn unreadable(error: &io::Error) -> String {
    match error.kind() {
        io::ErrorKind::NotFound => "not found".into(),
        io::ErrorKind::PermissionDenied => "permission denied".into(),
        io::ErrorKind::IsADirectory => "is a directory".into(),
        io::ErrorKind::NotADirectory => "not a directory".into(),
        _ => error.to_string(),
    }
}

/// `\r\n` and `\r` become `\n`, as Python's `read_text` does; the text never grows, so the
/// erasable buffer is never reallocated.
fn universal_newlines(text: &str) -> Zeroizing<String> {
    let mut converted = Zeroizing::new(String::with_capacity(text.len()));
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\r' {
            characters.next_if_eq(&'\n');
            converted.push('\n');
        } else {
            converted.push(character);
        }
    }
    converted
}

/// The value of a secret: from a file or typed without echo, never from the command line;
/// `None` (with the error printed) when the file cannot be read.
fn read_value(
    terminal: &mut dyn Terminal,
    from_file: Option<&PathBuf>,
) -> Result<Option<Zeroizing<String>>, Cancelled> {
    let Some(path) = from_file else {
        return terminal.ask_secret("Value (nothing is echoed): ").map(Some);
    };
    let reason = match std::fs::read(path).map(Zeroizing::new) {
        Err(error) => unreadable(&error),
        Ok(bytes) => match std::str::from_utf8(&bytes) {
            Ok(text) => return Ok(Some(universal_newlines(text))),
            Err(_) => "not valid UTF-8".into(),
        },
    };
    terminal.err(&format!("error: cannot read {}: {reason}", path.display()));
    Ok(None)
}

/// The version of the secret that the next change must expect (see `secrets.get`).
fn stream_version(config: &HecateConfig, terminal: &mut dyn Terminal, path: &str) -> Option<Value> {
    call(config, terminal, "secrets.get", json!({"path": path}))
        .map(|data| field(&data, "stream_version").clone())
}

fn change_version(
    config: &HecateConfig,
    terminal: &mut dyn Terminal,
    op: &str,
    done: &str,
    path: &str,
    version: i64,
) -> i32 {
    let Some(expected) = stream_version(config, terminal, path) else {
        return 1;
    };
    let args = json!({"path": path, "version": version, "expected_version": expected});
    if call(config, terminal, op, args).is_none() {
        return 1;
    }
    terminal.out(&format!("version {version} of {path} {done}"));
    0
}

fn secrets(
    config: &HecateConfig,
    terminal: &mut dyn Terminal,
    command: SecretsCommand,
) -> Result<i32, Cancelled> {
    Ok(match command {
        SecretsCommand::Create {
            path,
            value_type,
            from_file,
            max_versions,
            rotate_every,
            expires_at,
        } => {
            let Some(value) = read_value(terminal, from_file.as_ref())? else {
                return Ok(1);
            };
            let mut request =
                json!({"path": path, "value": value.as_str(), "value_type": value_type});
            if let Some(max_versions) = max_versions {
                request["max_versions"] = json!(max_versions);
            }
            if let Some(rotate_every) = rotate_every {
                request["rotate_every"] = json!(rotate_every);
            }
            if let Some(expires_at) = expires_at {
                request["expires_at"] = json!(expires_at);
            }
            let Some(data) = call(config, terminal, "secrets.create", request) else {
                return Ok(1);
            };
            terminal.out(&format!(
                "created {} version {} fingerprint {}",
                shown(field(&data, "path")),
                shown(field(&data, "version")),
                shown(field(&data, "fingerprint"))
            ));
            0
        }
        SecretsCommand::List { identity } => {
            let args = identity.map_or_else(|| json!({}), |identity| json!({"identity": identity}));
            let Some(data) = call(config, terminal, "secrets.list", args) else {
                return Ok(1);
            };
            let items = field(&data, "secrets")
                .as_array()
                .cloned()
                .unwrap_or_default();
            for item in &items {
                let active = match &item["active_version"] {
                    Value::Null => "-".to_string(),
                    version => shown(version),
                };
                let expires = if truthy(&item["expires_at"]) {
                    format!(" expires {}", shown(&item["expires_at"]))
                } else {
                    String::new()
                };
                terminal.out(&format!(
                    "{}  {}  active {active}  versions {}{expires}",
                    shown(&item["path"]),
                    shown(&item["value_type"]),
                    shown(&item["version_count"])
                ));
            }
            if items.is_empty() {
                terminal.out("no secrets");
            }
            0
        }
        SecretsCommand::Get { path } => {
            let Some(data) = call(config, terminal, "secrets.get", json!({"path": path})) else {
                return Ok(1);
            };
            terminal.out(&format!(
                "{}  ({}, owner {}, max versions {})",
                shown(field(&data, "path")),
                shown(field(&data, "value_type")),
                shown(field(&data, "owner")),
                shown(field(&data, "max_versions"))
            ));
            if truthy(field(&data, "rotate_every")) {
                terminal.out(&format!(
                    "rotate every: {}",
                    shown(field(&data, "rotate_every"))
                ));
            }
            if truthy(field(&data, "expires_at")) {
                terminal.out(&format!(
                    "expires at: {}",
                    shown(field(&data, "expires_at"))
                ));
            }
            for version in field(&data, "versions").as_array().into_iter().flatten() {
                terminal.out(&format!(
                    "  version {}  {}  {}  fingerprint {}",
                    shown(&version["version"]),
                    shown(&version["state"]),
                    shown(&version["created_at"]),
                    shown(&version["fingerprint"])
                ));
            }
            0
        }
        SecretsCommand::Put { path, from_file } => {
            let Some(expected) = stream_version(config, terminal, &path) else {
                return Ok(1);
            };
            let Some(value) = read_value(terminal, from_file.as_ref())? else {
                return Ok(1);
            };
            let args = json!({"path": path, "value": value.as_str(), "expected_version": expected});
            let Some(data) = call(config, terminal, "secrets.add_version", args) else {
                return Ok(1);
            };
            terminal.out(&format!(
                "added version {} to {path} fingerprint {}",
                shown(field(&data, "version")),
                shown(field(&data, "fingerprint"))
            ));
            0
        }
        SecretsCommand::Revoke { path, version } => change_version(
            config,
            terminal,
            "secrets.revoke",
            "revoked",
            &path,
            version,
        ),
        SecretsCommand::Restore { path, version } => change_version(
            config,
            terminal,
            "secrets.restore",
            "restored",
            &path,
            version,
        ),
        SecretsCommand::Promote { path, version } => change_version(
            config,
            terminal,
            "secrets.promote",
            "promoted",
            &path,
            version,
        ),
        SecretsCommand::Destroy {
            path,
            version,
            with_shares,
        } => {
            let Some(expected) = stream_version(config, terminal, &path) else {
                return Ok(1);
            };
            terminal.out(&format!(
                "Destroying version {version} of {path} erases its value for good."
            ));
            let proof = ask_proof(terminal, with_shares)?;
            let args = json!({"path": path, "version": version, "expected_version": expected, "proof": &*proof});
            if call(config, terminal, "secrets.destroy", args).is_none() {
                return Ok(1);
            }
            terminal.out(&format!("version {version} of {path} destroyed"));
            0
        }
    })
}

/// The new passphrase typed twice; `None` (with the error printed) when they differ.
fn ask_new_passphrase(terminal: &mut dyn Terminal) -> Result<Option<Zeroizing<String>>, Cancelled> {
    let first = terminal.ask_secret("New passphrase (12 characters or more): ")?;
    let again = terminal.ask_secret("Repeat the passphrase: ")?;
    if *again != *first {
        terminal.err("error: the passphrases do not match");
        return Ok(None);
    }
    Ok(Some(first))
}

fn ask_shares(terminal: &mut dyn Terminal) -> Result<Vec<Zeroizing<String>>, Cancelled> {
    (1..=2)
        .map(|number| terminal.ask_secret(&format!("Recovery share {number} of 2: ")))
        .collect()
}

/// Two of the shares just shown, typed back to prove they were saved.
fn confirm_shares(terminal: &mut dyn Terminal) -> Result<Vec<Zeroizing<String>>, Cancelled> {
    (1..=2)
        .map(|number| terminal.ask_secret(&format!("Share {number} of 2: ")))
        .collect()
}

fn ask_proof(terminal: &mut dyn Terminal, with_shares: bool) -> Result<ErasedJson, Cancelled> {
    Ok(ErasedJson(if with_shares {
        json!({"shares": texts(&ask_shares(terminal)?)})
    } else {
        json!({"passphrase": terminal.ask_secret("Current passphrase: ")?.as_str()})
    }))
}

/// Each share on its own line, written from a buffer that is zeroed when dropped.
fn print_shares(terminal: &mut dyn Terminal, shares: &Value) {
    for (number, share) in shares.as_array().into_iter().flatten().enumerate() {
        let share = share.as_str().unwrap_or_default();
        let mut line = Zeroizing::new(String::with_capacity(16 + share.len()));
        line.push_str("  share ");
        line.push_str(&(number + 1).to_string());
        line.push_str(": ");
        line.push_str(share);
        terminal.out(&line);
    }
}

fn init(config: &HecateConfig, terminal: &mut dyn Terminal) -> Result<i32, Cancelled> {
    terminal.out(INIT_WARNING);
    if terminal
        .ask("Type 'yes' to continue: ")?
        .trim()
        .to_lowercase()
        != "yes"
    {
        terminal.out("cancelled");
        return Ok(1);
    }
    let Some(passphrase) = ask_new_passphrase(terminal)? else {
        return Ok(1);
    };
    let Some(begun) = call(
        config,
        terminal,
        "init.begin",
        json!({"passphrase": passphrase.as_str()}),
    ) else {
        return Ok(1);
    };
    terminal.out("\nRecovery shares (any two recover Hécate). They are shown only once:");
    terminal.out("Keep each one in a different place and do not store them together.\n");
    print_shares(terminal, field(&begun, "shares"));
    terminal.out("\nTo confirm you saved them, enter two of them (nothing is echoed).");
    let init_id = field(&begun, "init_id").clone();
    let given = match confirm_shares(terminal) {
        Ok(given) => given,
        Err(Cancelled) => {
            call(config, terminal, "init.abort", json!({"init_id": init_id}));
            terminal.err("\ninitialization aborted: nothing was saved");
            return Ok(1);
        }
    };
    let confirmed = call(
        config,
        terminal,
        "init.confirm",
        json!({"init_id": init_id, "shares": texts(&given)}),
    );
    if confirmed.is_none() {
        call(config, terminal, "init.abort", json!({"init_id": init_id}));
        return Ok(1);
    }
    terminal.out("Hécate is initialized and sealed. Unseal it with: hecate unseal");
    terminal.out("Clear this terminal's scrollback: the shares were displayed on it.");
    Ok(0)
}

fn unseal(
    config: &HecateConfig,
    terminal: &mut dyn Terminal,
    shares: bool,
) -> Result<i32, Cancelled> {
    let args = if shares {
        json!({"shares": texts(&ask_shares(terminal)?)})
    } else {
        json!({"passphrase": terminal.ask_secret("Passphrase: ")?.as_str()})
    };
    let Some(data) = call(config, terminal, "unseal", args) else {
        return Ok(1);
    };
    terminal.out(&format!("state: {}", shown(field(&data, "state"))));
    if truthy(field(&data, "recommend_passphrase_change")) {
        terminal.out(
            "Unsealed with recovery shares: set a new passphrase with: hecate passphrase change --with-shares",
        );
    }
    Ok(0)
}

fn passphrase_change(
    config: &HecateConfig,
    terminal: &mut dyn Terminal,
    with_shares: bool,
) -> Result<i32, Cancelled> {
    let proof = ask_proof(terminal, with_shares)?;
    let Some(new) = ask_new_passphrase(terminal)? else {
        return Ok(1);
    };
    let args = json!({"proof": &*proof, "new_passphrase": new.as_str()});
    let Some(data) = call(config, terminal, "passphrase.change", args) else {
        return Ok(1);
    };
    terminal.out(&format!(
        "passphrase changed (configuration version {})",
        shown(field(&data, "config_version"))
    ));
    Ok(0)
}

fn shares_rekey(
    config: &HecateConfig,
    terminal: &mut dyn Terminal,
    with_shares: bool,
) -> Result<i32, Cancelled> {
    let proof = ask_proof(terminal, with_shares)?;
    let Some(begun) = call(
        config,
        terminal,
        "shares.rekey.begin",
        json!({"proof": &*proof}),
    ) else {
        return Ok(1);
    };
    terminal.out("\nNew recovery shares (any two recover Hécate). The old ones stop working once you confirm:");
    print_shares(terminal, field(&begun, "shares"));
    terminal.out("\nTo confirm you saved them, enter two of them (nothing is echoed).");
    let rekey_id = field(&begun, "rekey_id").clone();
    let given = match confirm_shares(terminal) {
        Ok(given) => given,
        Err(Cancelled) => {
            call(
                config,
                terminal,
                "shares.rekey.abort",
                json!({"rekey_id": rekey_id}),
            );
            terminal.err("\nrekey aborted: the old shares still work");
            return Ok(1);
        }
    };
    let args = json!({"rekey_id": rekey_id, "shares": texts(&given)});
    let Some(data) = call(config, terminal, "shares.rekey.confirm", args) else {
        call(
            config,
            terminal,
            "shares.rekey.abort",
            json!({"rekey_id": rekey_id}),
        );
        return Ok(1);
    };
    terminal.out(&format!(
        "new recovery shares active (configuration version {})",
        shown(field(&data, "config_version"))
    ));
    terminal.out("Clear this terminal's scrollback: the shares were displayed on it.");
    Ok(0)
}

fn seal(config: &HecateConfig, terminal: &mut dyn Terminal) -> i32 {
    let Some(data) = call(config, terminal, "seal", json!({})) else {
        return 1;
    };
    terminal.out(&format!("state: {}", shown(field(&data, "state"))));
    0
}

/// Runs Hécate, always sealed, until SIGINT or SIGTERM: the root is released, then the socket
/// closes, and the exit is 0. With a CA, the API listens while Hécate's certificate is valid.
fn serve(config: &HecateConfig, terminal: &mut dyn Terminal) -> i32 {
    configure_logging();
    if let Err(error) = config.prepare() {
        terminal.err(&format!(
            "error: cannot prepare {}: {error}",
            config.data_dir.display()
        ));
        return 1;
    }
    harden_process();
    let db_path = config.db_path();
    let opened = (|| -> Result<_, OpenError> {
        Ok((
            SqliteSealRepository::open(&db_path)?,
            SqliteAccessLog::open(&db_path)?,
            SqliteSecretStore::open(&db_path)?,
            SqliteIdentityStore::open(&db_path)?,
            SqliteEnrollAttempts::open(&db_path)?,
        ))
    })();
    let (repo, access_log, store, identity_store, enroll_attempts) = match opened {
        Ok(opened) => opened,
        Err(error) => {
            terminal.err(&format!(
                "error: cannot open {}: {error}",
                db_path.display()
            ));
            return 1;
        }
    };
    let access_log = Arc::new(access_log);
    let service = Arc::new(SealingService::new(
        Arc::new(SoftwareCryptoProvider::new()),
        Arc::new(repo),
        access_log.clone(),
        Arc::new(SystemClock),
        SealingSettings::new(config.kdf),
    ));
    let store = Arc::new(store);
    let secrets = Arc::new(SecretsService::new(
        service.clone(),
        store.clone(),
        Arc::new(SoftwareCryptoProvider::new()),
        access_log.clone(),
        Arc::new(SystemClock),
        SecretsSettings::default(),
    ));
    let identity_store = Arc::new(identity_store);
    let pki = Arc::new(RcgenPki::new());
    let ca = Arc::new(CaService::new(
        service.clone(),
        identity_store.clone(),
        pki.clone(),
        Arc::new(SoftwareCryptoProvider::new()),
        access_log.clone(),
        Arc::new(SystemClock),
        CaSettings::new(config.cert_lifetime_hours),
    ));
    let policies = Arc::new(AccessPolicyService::new(
        service.clone(),
        identity_store.clone(),
        store,
        access_log.clone(),
        Arc::new(SystemClock),
    ));
    let identities = Arc::new(IdentityService::new(
        service.clone(),
        ca.clone(),
        policies.clone(),
        secrets.clone(),
        identity_store,
        Arc::new(enroll_attempts),
        pki,
        Arc::new(SoftwareCryptoProvider::new()),
        access_log,
        Arc::new(SystemClock),
        Arc::new(JwtCallTokenVerifier::new()),
        IdentitySettings::new(config.enroll_token_minutes),
    ));
    let served = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .and_then(|runtime| {
            runtime.block_on(async {
                let supervisor = Arc::new(ApiSupervisor::new(
                    config.api_addr,
                    ca.clone(),
                    routes(identities.clone()),
                    sealed_check(service.clone()),
                    refusals(identities.clone()),
                ));
                let runtime = tokio::runtime::Handle::current();
                let refreshed = supervisor.clone();
                // The channel runs each operation on a blocking thread, which may wait for this.
                let changed = Arc::new(move || {
                    if let Err(error) = runtime.block_on(refreshed.tick()) {
                        tracing::error!(target: "hecate.api", reason = %error, "cannot refresh the API");
                    }
                });
                let handler = Arc::new(request_handler_with_identities(
                    service.clone(),
                    Some(secrets),
                    IdentityOperations {
                        ca: ca.clone(),
                        identities: identities.clone(),
                        policies: policies.clone(),
                        changed,
                    },
                ));
                let server = ControlServer::bind(handler, &config.socket_path)?;
                if let Err(error) = supervisor.refresh().await {
                    tracing::error!(target: "hecate.api", reason = %error, "cannot start the API");
                }
                let periodic = tasks::spawn_periodic(
                    tasks::PERIOD,
                    ca.clone(),
                    identities.clone(),
                    supervisor.clone(),
                );
                let mut failure: Option<io::Error> = None;
                server
                    .serve_until(async {
                        if let Err(error) = stop_requested().await {
                            failure = Some(error);
                        }
                        service.shutdown();
                    })
                    .await;
                periodic.stop().await;
                supervisor.shutdown().await;
                failure.map_or(Ok(()), Err)
            })
        });
    service.shutdown();
    match served {
        Ok(()) => 0,
        Err(error) => {
            terminal.err(&format!("error: {error}"));
            1
        }
    }
}

/// Sends an operation; prints the error and returns `None` if it failed.
pub fn call(
    config: &HecateConfig,
    terminal: &mut dyn Terminal,
    op: &str,
    args: Value,
) -> Option<ErasedJson> {
    let mut reply = match ControlClient::new(&config.socket_path).request(op, args) {
        Ok(reply) => ErasedJson(reply),
        Err(error) => {
            terminal.err(&format!("error: {error}"));
            return None;
        }
    };
    if reply["status"] != "completed" {
        let error = &reply["error"];
        let retry = error
            .get("retry_after_s")
            .map(|seconds| format!(" (retry in {seconds}s)"))
            .unwrap_or_default();
        let state = match reply["data"].get("state").and_then(Value::as_str) {
            Some(state) if !state.is_empty() => format!(" [state: {state}]"),
            _ => String::new(),
        };
        let code = error["code"].as_str().unwrap_or("internal");
        let message = error["message"].as_str().unwrap_or("internal error");
        terminal.err(&format!("error ({code}): {message}{retry}{state}"));
        return None;
    }
    match reply.0.get_mut("data").map(Value::take) {
        Some(data @ Value::Object(_)) => Some(ErasedJson(data)),
        Some(mut other) => {
            erase(&mut other);
            None
        }
        None => None,
    }
}
