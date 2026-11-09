//! The local channel's server (research R9): a private Unix socket, one JSON request per line, the
//! peer's credentials checked on Linux and the operations served one at a time.

use std::collections::HashMap;
use std::fs::{self, Permissions};
use std::future::Future;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use hecate_application::errors::ServiceError;
use hecate_domain::sealing::errors::HecateError;
use nix::sys::stat::{Mode, umask};
use serde_json::{Map, Value};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Mutex;
use zeroize::Zeroizing;

use crate::control::protocol::{self, Envelope, MAX_LINE, parse_request};
use crate::process::Os;

/// Answers one request line with one reply line, in a buffer that is zeroed when dropped.
pub trait LineHandler: Send + Sync {
    fn handle(&self, raw: &[u8]) -> Zeroizing<Vec<u8>>;
}

/// Why a recognized operation did not complete.
#[derive(Debug)]
pub enum OpError {
    /// Malformed arguments, refused before reaching the service; the refusal is recorded.
    Arg(String),
    Service(ServiceError),
}

impl From<ServiceError> for OpError {
    fn from(error: ServiceError) -> Self {
        Self::Service(error)
    }
}

impl From<HecateError> for OpError {
    fn from(error: HecateError) -> Self {
        Self::Service(error.into())
    }
}

pub type OpResult = Result<Value, OpError>;
pub type Operation = Box<dyn Fn(&Map<String, Value>) -> OpResult + Send + Sync>;
pub type RefusalRecorder = Box<dyn Fn(&str, &Map<String, Value>) + Send + Sync>;

/// A required text argument, as the Python channel checks it.
pub fn text_arg(args: &Map<String, Value>, key: &str) -> Result<String, OpError> {
    match args.get(key) {
        Some(Value::String(text)) => Ok(text.clone()),
        _ => Err(OpError::Arg(format!("'{key}' must be a string"))),
    }
}

/// A required list of texts.
pub fn texts_arg(args: &Map<String, Value>, key: &str) -> Result<Vec<String>, OpError> {
    let invalid = || OpError::Arg(format!("'{key}' must be a list of strings"));
    let Some(Value::Array(items)) = args.get(key) else {
        return Err(invalid());
    };
    items
        .iter()
        .map(|item| item.as_str().map(str::to_string).ok_or_else(invalid))
        .collect()
}

/// The table of operations, filled by the sealing and secrets features.
#[derive(Default)]
pub struct RequestHandler {
    ops: HashMap<&'static str, Operation>,
    refusals: Option<RefusalRecorder>,
}

impl RequestHandler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, op: &'static str, operation: Operation) {
        self.ops.insert(op, operation);
    }

    pub fn on_refused(&mut self, recorder: RefusalRecorder) {
        self.refusals = Some(recorder);
    }

    pub fn handle_request(&self, raw: &[u8]) -> Envelope {
        let started = Instant::now();
        let (envelope, op) = match parse_request(raw) {
            Err(error) => (
                protocol::failed(None, &error.into(), started),
                "?".to_string(),
            ),
            Ok(mut request) => {
                let outcome = match self.ops.get(request.op.as_str()) {
                    None => Err(OpError::from(HecateError::invalid(format!(
                        "unknown operation '{}'",
                        request.op
                    )))),
                    Some(operation) => operation(&request.args),
                };
                let envelope = match outcome {
                    Ok(data) => protocol::completed(request.request_id, data, started),
                    Err(OpError::Arg(message)) => {
                        if let Some(record) = &self.refusals {
                            record(&request.op, &request.args);
                        }
                        protocol::failed(
                            request.request_id,
                            &HecateError::invalid(message).into(),
                            started,
                        )
                    }
                    Err(OpError::Service(error)) => {
                        let envelope = protocol::failed(request.request_id, &error, started);
                        if envelope
                            .error
                            .as_ref()
                            .is_some_and(|body| body.code == "internal")
                        {
                            tracing::error!(target: "hecate.control", op = %request.op, exception = error.kind(), "unexpected error");
                        }
                        envelope
                    }
                };
                // The arguments may carry a passphrase, shares or a value: zeroed before they go.
                request.args.values_mut().for_each(protocol::erase);
                (envelope, request.op)
            }
        };
        let result = envelope.error.as_ref().map_or("ok", |body| body.code);
        tracing::info!(target: "hecate.control", op = %op, result, "request");
        envelope
    }
}

impl LineHandler for RequestHandler {
    fn handle(&self, raw: &[u8]) -> Zeroizing<Vec<u8>> {
        let mut envelope = self.handle_request(raw);
        let line = protocol::encode(&envelope);
        protocol::erase(&mut envelope.data);
        line
    }
}

/// On Linux only the user of the process may connect; elsewhere the 0600 socket in a 0700
/// directory is the barrier.
pub fn peer_is_same_user(os: Os, peer_uid: impl FnOnce() -> io::Result<u32>, own_uid: u32) -> bool {
    match os {
        Os::Other => true,
        Os::Linux => peer_uid().is_ok_and(|uid| uid == own_uid),
    }
}

enum Line {
    End,
    Request(Zeroizing<Vec<u8>>),
    TooLarge,
}

/// Reads request lines into buffers that are zeroed when dropped, also when they grow: requests
/// carry passphrases, shares and secret values.
struct LineReader<R> {
    inner: R,
    pending: Zeroizing<Vec<u8>>,
}

impl<R: AsyncRead + Unpin> LineReader<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            pending: Zeroizing::new(Vec::new()),
        }
    }

    async fn next_line(&mut self) -> io::Result<Line> {
        let mut chunk = Zeroizing::new([0u8; 8192]);
        loop {
            if let Some(newline) = self.pending.iter().position(|byte| *byte == b'\n') {
                if newline + 1 > MAX_LINE {
                    return Ok(Line::TooLarge);
                }
                let mut line = Zeroizing::new(Vec::with_capacity(newline + 1));
                line.extend_from_slice(&self.pending[..=newline]);
                self.pending.drain(..=newline);
                return Ok(Line::Request(line));
            }
            if self.pending.len() > MAX_LINE {
                return Ok(Line::TooLarge);
            }
            let read = self.inner.read(chunk.as_mut()).await?;
            if read == 0 {
                if self.pending.is_empty() {
                    return Ok(Line::End);
                }
                let line = std::mem::replace(&mut self.pending, Zeroizing::new(Vec::new()));
                return Ok(Line::Request(line));
            }
            protocol::append_erasing(&mut self.pending, &chunk[..read]);
        }
    }
}

/// Serves one connection: a rejected peer is closed without reading anything.
pub async fn serve_connection(
    stream: UnixStream,
    allowed: bool,
    handler: Arc<dyn LineHandler>,
    turn: Arc<Mutex<()>>,
) {
    if !allowed {
        tracing::warn!(target: "hecate.control", "connection from another user rejected");
        return;
    }
    let (reader, mut writer) = stream.into_split();
    let mut reader = LineReader::new(reader);
    loop {
        match reader.next_line().await {
            Ok(Line::Request(line)) => {
                let reply = {
                    let _turn = turn.lock().await;
                    let handler = Arc::clone(&handler);
                    tokio::task::spawn_blocking(move || handler.handle(&line)).await
                };
                let Ok(reply) = reply else { return };
                if writer.write_all(&reply[..]).await.is_err() {
                    return;
                }
            }
            Ok(Line::TooLarge) => {
                let too_large = HecateError::invalid("request too large").into();
                let reply = protocol::encode(&protocol::failed(None, &too_large, Instant::now()));
                let _ = writer.write_all(&reply[..]).await;
                return;
            }
            Ok(Line::End) | Err(_) => return,
        }
    }
}

pub struct ControlServer {
    listener: UnixListener,
    path: PathBuf,
    handler: Arc<dyn LineHandler>,
    turn: Arc<Mutex<()>>,
}

impl ControlServer {
    /// Binds the socket, born 0600; must run inside a tokio runtime.
    pub fn bind(handler: Arc<dyn LineHandler>, path: &Path) -> io::Result<Self> {
        remove_stale_socket(path)?;
        let previous = umask(Mode::from_bits_truncate(0o177));
        let bound = UnixListener::bind(path);
        umask(previous);
        let listener = bound?;
        fs::set_permissions(path, Permissions::from_mode(0o600))?;
        Ok(Self {
            listener,
            path: path.to_path_buf(),
            handler,
            turn: Arc::new(Mutex::new(())),
        })
    }

    /// Serves until `stop` completes, then removes the socket.
    pub async fn serve_until(self, stop: impl Future<Output = ()>) {
        tokio::pin!(stop);
        let own_uid = nix::unistd::geteuid().as_raw();
        loop {
            tokio::select! {
                accepted = self.listener.accept() => {
                    let Ok((stream, _)) = accepted else { continue };
                    let allowed = peer_is_same_user(
                        Os::current(),
                        || stream.peer_cred().map(|credentials| credentials.uid()),
                        own_uid,
                    );
                    tokio::spawn(serve_connection(
                        stream,
                        allowed,
                        Arc::clone(&self.handler),
                        Arc::clone(&self.turn),
                    ));
                }
                () = &mut stop => break,
            }
        }
        let _ = fs::remove_file(&self.path);
    }
}

fn remove_stale_socket(path: &Path) -> io::Result<()> {
    if fs::symlink_metadata(path).is_err() {
        return Ok(());
    }
    match std::os::unix::net::UnixStream::connect(path) {
        Err(_) => fs::remove_file(path),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AddrInUse,
            format!("another Hécate is already listening on {}", path.display()),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn lines(input: &[u8]) -> Vec<String> {
        let mut reader = LineReader::new(input);
        let mut lines = Vec::new();
        loop {
            match reader.next_line().await.unwrap() {
                Line::End => return lines,
                Line::TooLarge => {
                    lines.push("too large".into());
                    return lines;
                }
                Line::Request(line) => lines.push(String::from_utf8(line.to_vec()).unwrap()),
            }
        }
    }

    #[tokio::test]
    async fn requests_are_read_line_by_line_with_what_is_left_at_the_end() {
        assert_eq!(lines(b"one\ntwo\nlast").await, ["one\n", "two\n", "last"]);
        assert!(lines(b"").await.is_empty());
    }

    #[tokio::test]
    async fn a_line_over_one_mebibyte_is_too_large() {
        let mut big = vec![b'x'; MAX_LINE];
        big.push(b'\n');
        assert_eq!(lines(&big).await, ["too large"]);
        let fits = [vec![b'x'; MAX_LINE - 1], b"\n".to_vec()].concat();
        assert_eq!(lines(&fits).await.len(), 1);
        assert_eq!(lines(&vec![b'x'; MAX_LINE + 1]).await, ["too large"]);
    }
}
