//! The `hecate` CLI (contracts/cli.md): its commands take a terminal, so the tests drive them with a
//! scripted one (research R15).

use std::io;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use hecate_application::sealing::service::{INIT_WARNING, SealingService};
use hecate_application::sealing::settings::SealingSettings;
use hecate_application::secrets::service::SecretsService;
use hecate_application::secrets::settings::SecretsSettings;
use hecate_infrastructure::clock::SystemClock;
use hecate_infrastructure::config::{ConfigError, HecateConfig};
use hecate_infrastructure::control::client::ControlClient;
use hecate_infrastructure::control::operations::request_handler;
use hecate_infrastructure::control::protocol::erase;
use hecate_infrastructure::control::server::ControlServer;
use hecate_infrastructure::crypto::SoftwareCryptoProvider;
use hecate_infrastructure::logs::configure_logging;
use hecate_infrastructure::process::harden_process;
use hecate_infrastructure::signals::stop_requested;
use hecate_infrastructure::store::sealing::{SqliteAccessLog, SqliteSealRepository};
use hecate_infrastructure::store::secrets::SqliteSecretStore;
use serde_json::{Value, json};
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
        /// Informative rotation period, e.g. 90d
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
    }
    0
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
/// closes, and the exit is 0.
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
    let (repo, access_log, store) = match (
        SqliteSealRepository::open(&db_path),
        SqliteAccessLog::open(&db_path),
        SqliteSecretStore::open(&db_path),
    ) {
        (Ok(repo), Ok(access_log), Ok(store)) => (repo, access_log, store),
        (Err(error), _, _) | (_, Err(error), _) | (_, _, Err(error)) => {
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
    let secrets = Arc::new(SecretsService::new(
        service.clone(),
        Arc::new(store),
        Arc::new(SoftwareCryptoProvider::new()),
        access_log,
        Arc::new(SystemClock),
        SecretsSettings::default(),
    ));
    let handler = Arc::new(request_handler(service.clone(), Some(secrets)));
    let served = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .and_then(|runtime| {
            runtime.block_on(async {
                let server = ControlServer::bind(handler, &config.socket_path)?;
                let mut failure: Option<io::Error> = None;
                server
                    .serve_until(async {
                        if let Err(error) = stop_requested().await {
                            failure = Some(error);
                        }
                        service.shutdown();
                    })
                    .await;
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
