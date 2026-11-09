use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use aws_lc_rs::rand::{SecureRandom, SystemRandom};
use serde_json::{Value, json};
use zeroize::Zeroizing;

use crate::control::protocol::{ErasingWriter, append_erasing, erase};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ControlUnavailable(pub String);

/// The CLI's side of the local channel: one request and one reply per connection.
pub struct ControlClient {
    path: PathBuf,
    timeout: Duration,
}

impl ControlClient {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            timeout: Duration::from_secs(600),
        }
    }

    pub fn request(&self, op: &str, args: Value) -> Result<Value, ControlUnavailable> {
        let unreachable =
            || ControlUnavailable(format!("cannot reach Hécate at {}", self.path.display()));
        // The arguments may carry a passphrase, shares or a value: once written, they are zeroed.
        let mut message = json!({"request_id": request_id(), "op": op, "args": args});
        let mut line = Zeroizing::new(Vec::new());
        let written = serde_json::to_writer(ErasingWriter(&mut line), &message);
        erase(&mut message);
        written.map_err(|_| ControlUnavailable("cannot write the request".into()))?;
        append_erasing(&mut line, b"\n");
        let mut stream = UnixStream::connect(&self.path).map_err(|_| unreachable())?;
        stream
            .set_read_timeout(Some(self.timeout))
            .and_then(|()| stream.set_write_timeout(Some(self.timeout)))
            .and_then(|()| stream.write_all(&line))
            .map_err(|_| unreachable())?;
        let reply = read_line(&mut stream).map_err(|_| unreachable())?;
        if reply.is_empty() {
            return Err(ControlUnavailable(
                "Hécate closed the connection without answering".into(),
            ));
        }
        serde_json::from_slice(&reply)
            .map_err(|_| ControlUnavailable("Hécate answered with an invalid reply".into()))
    }
}

/// The reply line, in a buffer that is zeroed when dropped: replies carry shares.
fn read_line(stream: &mut UnixStream) -> io::Result<Zeroizing<Vec<u8>>> {
    let mut line = Zeroizing::new(Vec::new());
    let mut chunk = Zeroizing::new([0u8; 8192]);
    loop {
        let read = stream.read(chunk.as_mut())?;
        if read == 0 {
            return Ok(line);
        }
        let piece = &chunk[..read];
        if let Some(newline) = piece.iter().position(|byte| *byte == b'\n') {
            append_erasing(&mut line, &piece[..=newline]);
            return Ok(line);
        }
        append_erasing(&mut line, piece);
    }
}

/// A random UUID (version 4) for the request.
fn request_id() -> String {
    let mut bytes = [0u8; 16];
    let _ = SystemRandom::new().fill(&mut bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}
