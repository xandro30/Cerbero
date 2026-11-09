use std::io::{self, BufRead, Write};
use std::process::ExitCode;
use std::sync::mpsc::{self, Receiver};

use clap::Parser;
use hecate_cli::{Cancelled, Cli, Terminal};
use hecate_infrastructure::config::HecateConfig;
use nix::sys::termios::{LocalFlags, SetArg, Termios, tcgetattr, tcsetattr};
use zeroize::Zeroizing;

enum Input {
    Line(Zeroizing<String>),
    End,
    Interrupted,
}

/// The real terminal: stdin lines, without echo when stdin is a terminal; the end of the input
/// and Ctrl-C cancel the prompt.
struct SystemTerminal {
    input: Receiver<Input>,
}

impl SystemTerminal {
    fn new() -> Self {
        let (sender, input) = mpsc::channel();
        let lines = sender.clone();
        std::thread::spawn(move || {
            let stdin = io::stdin();
            loop {
                // Room for a long line from the start, so that reading it does not leave copies behind.
                let mut line = Zeroizing::new(String::with_capacity(4096));
                match stdin.lock().read_line(&mut line) {
                    Ok(0) | Err(_) => {
                        let _ = lines.send(Input::End);
                        return;
                    }
                    Ok(_) => {
                        let text = line.trim_end_matches(['\n', '\r']).to_string();
                        if lines.send(Input::Line(Zeroizing::new(text))).is_err() {
                            return;
                        }
                    }
                }
            }
        });
        let _ = hecate_infrastructure::signals::on_interrupt(move || {
            let _ = sender.send(Input::Interrupted);
        });
        Self { input }
    }

    fn next_line(&mut self) -> Result<Zeroizing<String>, Cancelled> {
        match self.input.recv() {
            Ok(Input::Line(line)) => Ok(line),
            Ok(Input::End | Input::Interrupted) | Err(_) => Err(Cancelled),
        }
    }
}

/// Echo off while it lives; the terminal is restored when dropped.
struct EchoOff(Option<Termios>);

impl EchoOff {
    fn new() -> Self {
        let stdin = io::stdin();
        if !nix::unistd::isatty(&stdin).unwrap_or(false) {
            return Self(None);
        }
        let Ok(original) = tcgetattr(&stdin) else {
            return Self(None);
        };
        let mut quiet = original.clone();
        quiet.local_flags.remove(LocalFlags::ECHO);
        if tcsetattr(&stdin, SetArg::TCSANOW, &quiet).is_err() {
            return Self(None);
        }
        Self(Some(original))
    }
}

impl Drop for EchoOff {
    fn drop(&mut self) {
        if let Some(original) = &self.0 {
            let _ = tcsetattr(io::stdin(), SetArg::TCSANOW, original);
        }
    }
}

impl Terminal for SystemTerminal {
    fn ask(&mut self, prompt: &str) -> Result<String, Cancelled> {
        print!("{prompt}");
        let _ = io::stdout().flush();
        self.next_line().map(|line| line.to_string())
    }

    fn ask_secret(&mut self, prompt: &str) -> Result<Zeroizing<String>, Cancelled> {
        eprint!("{prompt}");
        let _ = io::stderr().flush();
        let echo = EchoOff::new();
        let line = self.next_line();
        drop(echo);
        eprintln!();
        line
    }

    fn out(&mut self, line: &str) {
        println!("{line}");
    }

    fn err(&mut self, line: &str) {
        eprintln!("{line}");
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let mut terminal = SystemTerminal::new();
    let code = hecate_cli::run(cli, HecateConfig::from_env(), &mut terminal);
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}
