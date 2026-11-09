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
