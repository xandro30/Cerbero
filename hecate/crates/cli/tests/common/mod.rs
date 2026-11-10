#![allow(dead_code)]

use std::collections::VecDeque;

use clap::Parser;
use hecate_cli::{Cancelled, Cli, Terminal, execute};
use hecate_infrastructure::config::HecateConfig;
use zeroize::Zeroizing;

pub enum Answer {
    Line(String),
    Cancel,
}

pub fn line(text: &str) -> Answer {
    Answer::Line(text.to_string())
}

/// Scripted prompts; with `shares_from_output`, a prompt that starts with "Share" is answered with
/// the shares the CLI has just printed, in order.
#[derive(Default)]
pub struct Scripted {
    pub answers: VecDeque<Answer>,
    pub shares_from_output: bool,
    pub taken: usize,
    pub out: Vec<String>,
    pub err: Vec<String>,
}

impl Scripted {
    pub fn new(answers: Vec<Answer>) -> Self {
        Self {
            answers: answers.into(),
            ..Self::default()
        }
    }

    pub fn echoing_shares(answers: Vec<Answer>) -> Self {
        Self {
            shares_from_output: true,
            ..Self::new(answers)
        }
    }

    pub fn script(&mut self, answers: Vec<Answer>) {
        self.answers = answers.into();
    }

    fn next(&mut self) -> Result<String, Cancelled> {
        match self.answers.pop_front().expect("no scripted answer left") {
            Answer::Line(text) => Ok(text),
            Answer::Cancel => Err(Cancelled),
        }
    }

    /// Everything written to stdout since the last call.
    pub fn take_out(&mut self) -> String {
        std::mem::take(&mut self.out).join("\n")
    }

    pub fn take_err(&mut self) -> String {
        std::mem::take(&mut self.err).join("\n")
    }
}

pub fn shares_printed(out: &str) -> Vec<String> {
    out.lines()
        .filter(|line| line.trim().starts_with("share "))
        .map(|line| line.split_once(": ").unwrap().1.trim().to_string())
        .collect()
}

impl Terminal for Scripted {
    fn ask(&mut self, _prompt: &str) -> Result<String, Cancelled> {
        self.next()
    }

    fn ask_secret(&mut self, prompt: &str) -> Result<Zeroizing<String>, Cancelled> {
        if self.shares_from_output && prompt.starts_with("Share") {
            self.taken += 1;
            let printed = shares_printed(&self.out.join("\n"));
            return Ok(Zeroizing::new(printed[self.taken - 1].clone()));
        }
        self.next().map(Zeroizing::new)
    }

    fn out(&mut self, line: &str) {
        self.out.push(line.to_string());
    }

    fn err(&mut self, line: &str) {
        self.err.push(line.to_string());
    }
}

/// Runs `hecate <args>` against the configuration, inside this process.
pub fn hecate(config: &HecateConfig, terminal: &mut Scripted, args: &[&str]) -> i32 {
    let cli = Cli::try_parse_from(std::iter::once("hecate").chain(args.iter().copied())).unwrap();
    execute(cli.command, config, terminal)
}

use std::sync::{Arc, Mutex, PoisonError};

use hecate_application::testing::{CaRig, MINIMAL_KDF};
use hecate_infrastructure::control::operations::{
    IdentityOperations, request_handler_with_identities,
};
use hecate_infrastructure::control::server::LineHandler;
use hecate_infrastructure::crypto::SoftwareCryptoProvider;
use hecate_infrastructure::pki::RcgenPki;
use hecate_infrastructure::testing::{ServerThread, short_dir};
use tempfile::TempDir;

pub const TRUST_DOMAIN: &str = "cerbero.test";
pub const CA_PASSPHRASE: &str = "orchid lantern quarry 42";

/// Every request line that reaches the server, to look for what must never cross the channel.
pub struct Recording {
    inner: Arc<dyn LineHandler>,
    pub lines: Mutex<Vec<Vec<u8>>>,
}

impl LineHandler for Recording {
    fn handle(&self, raw: &[u8]) -> Zeroizing<Vec<u8>> {
        self.lines
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(raw.to_vec());
        self.inner.handle(raw)
    }
}

/// A running, unsealed Hécate with a CA service on in-memory storage; the fields drop in order.
pub struct CaRunning {
    _server: ServerThread,
    pub config: HecateConfig,
    pub rig: CaRig,
    pub recording: Arc<Recording>,
    pub base: TempDir,
}

pub fn ca_running() -> CaRunning {
    let base = short_dir();
    let config = HecateConfig::for_dir(base.path().join("data"), MINIMAL_KDF);
    config.prepare().unwrap();
    let rig = CaRig::new(
        Arc::new(SoftwareCryptoProvider::new()),
        Arc::new(RcgenPki::new()),
        Arc::new(hecate_infrastructure::call_token::JwtCallTokenVerifier::new()),
    );
    // The CLI signs with the time of the system, as Hécate does outside the tests: the clock of
    // the rig moves just past it.
    let behind =
        time::OffsetDateTime::now_utc() - hecate_application::ports::Clock::now(rig.clock.as_ref());
    rig.clock.advance(behind + time::Duration::minutes(1));
    let handler = request_handler_with_identities(
        rig.sealing.clone(),
        None,
        IdentityOperations {
            ca: rig.service.clone(),
            identities: rig.identities.clone(),
            changed: Arc::new(|| {}),
        },
    );
    let recording = Arc::new(Recording {
        inner: Arc::new(handler),
        lines: Mutex::new(Vec::new()),
    });
    let server = ServerThread::start(recording.clone(), &config.socket_path);
    CaRunning {
        _server: server,
        config,
        rig,
        recording,
        base,
    }
}

/// `hecate ca create` through the CLI, with a copy that opens.
pub fn create_ca(env: &CaRunning) {
    let out = env.base.path().join("root-ca.pem");
    let mut terminal = Scripted::new(vec![
        line(CA_PASSPHRASE),
        line(out.to_str().unwrap()),
        line(CA_PASSPHRASE),
    ]);
    let code = hecate(
        &env.config,
        &mut terminal,
        &[
            "ca",
            "create",
            "--trust-domain",
            TRUST_DOMAIN,
            "--out",
            out.to_str().unwrap(),
        ],
    );
    assert_eq!(code, 0, "{}", terminal.take_err());
}
