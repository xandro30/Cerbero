import argparse

import pytest

from hecate.infrastructure.cli import main as cli
from hecate.infrastructure.config import HecateConfig
from hecate.infrastructure.control.server import ControlServer, RequestHandler
from tests.fakes import GOOD_PASSPHRASE, ServerThread, initialize, make_service, short_dir

NS = argparse.Namespace()


@pytest.fixture
def running():
    with short_dir() as base:
        config = HecateConfig.for_dir(base / "data")
        config.prepare()
        server = ControlServer(RequestHandler(make_service()), config.socket_path)
        with ServerThread(server):
            yield config


def script(monkeypatch, answers):
    """Feed getpass/input from a list; an Exception instance in the list is raised."""
    it = iter(answers)

    def next_answer(*_a, **_k):
        value = next(it)
        if isinstance(value, BaseException):
            raise value
        return value

    monkeypatch.setattr("getpass.getpass", next_answer)
    monkeypatch.setattr("builtins.input", next_answer)


def shares_printed(out: str) -> list[str]:
    return [line.split(": ", 1)[1].strip() for line in out.splitlines() if line.strip().startswith("share ")]


class Terminal:
    """Scripted prompts; when the CLI asks for shares, answer with the ones it just printed."""

    def __init__(self, capsys, monkeypatch, answers):
        self.capsys, self.answers, self.out, self.taken = capsys, iter(answers), "", 0
        monkeypatch.setattr("builtins.input", lambda *_a: next(self.answers))
        monkeypatch.setattr("getpass.getpass", self.getpass)

    def getpass(self, prompt=""):
        if prompt.startswith("Share"):
            self.out += self.capsys.readouterr().out
            self.taken += 1
            return shares_printed(self.out)[self.taken - 1]
        return next(self.answers)


def test_init_flow_confirms_with_two_shares(running, monkeypatch, capsys):
    terminal = Terminal(capsys, monkeypatch, ["yes", GOOD_PASSPHRASE, GOOD_PASSPHRASE])
    assert cli.cmd_init(running, NS) == 0
    out = terminal.out + capsys.readouterr().out
    assert len(shares_printed(out)) == 3
    assert "initialized and sealed" in out
    assert cli.cmd_status(running, NS) == 0
    assert "state: sealed" in capsys.readouterr().out


def test_init_requires_consent_and_matching_passphrases(running, monkeypatch, capsys):
    script(monkeypatch, ["no"])
    assert cli.cmd_init(running, NS) == 1
    script(monkeypatch, ["yes", GOOD_PASSPHRASE, "another passphrase"])
    assert cli.cmd_init(running, NS) == 1
    assert "do not match" in capsys.readouterr().err
    assert cli.cmd_status(running, NS) == 0
    assert "state: uninitialized" in capsys.readouterr().out


def test_interrupting_the_confirmation_aborts_the_initialization(running, monkeypatch, capsys):
    script(monkeypatch, ["yes", GOOD_PASSPHRASE, GOOD_PASSPHRASE, KeyboardInterrupt()])
    assert cli.cmd_init(running, NS) == 1
    assert "aborted" in capsys.readouterr().err
    cli.cmd_status(running, NS)
    assert "state: uninitialized" in capsys.readouterr().out


def test_cli_reports_when_hecate_is_not_running(tmp_path, capsys):
    config = HecateConfig.for_dir(tmp_path)
    assert cli.cmd_status(config, NS) == 1
    assert "cannot reach" in capsys.readouterr().err


@pytest.fixture
def initialized_env():
    with short_dir() as base:
        config = HecateConfig.for_dir(base / "data")
        config.prepare()
        service = make_service()
        shares = initialize(service)
        server = ControlServer(RequestHandler(service), config.socket_path)
        with ServerThread(server):
            yield config, shares


@pytest.fixture
def running_initialized(initialized_env):
    return initialized_env[0]


@pytest.fixture
def shares(initialized_env):
    return initialized_env[1]


def test_unseal_with_the_right_passphrase(running_initialized, monkeypatch, capsys):
    script(monkeypatch, [GOOD_PASSPHRASE])
    assert cli.cmd_unseal(running_initialized, NS) == 0
    assert "state: unsealed" in capsys.readouterr().out


def test_wrong_passphrases_then_the_wait_is_reported(running_initialized, monkeypatch, capsys):
    script(monkeypatch, ["wrong wrong wrong"] * 3 + [GOOD_PASSPHRASE])
    for _ in range(3):
        assert cli.cmd_unseal(running_initialized, NS) == 1
    assert "error (auth)" in capsys.readouterr().err
    assert cli.cmd_unseal(running_initialized, NS) == 1
    err = capsys.readouterr().err
    assert "error (rate_limited)" in err and "retry in 60s" in err
    assert cli.cmd_status(running_initialized, NS) == 0
    assert "next attempt allowed in" in capsys.readouterr().out


def test_unseal_with_shares_recommends_changing_the_passphrase(running_initialized, shares, monkeypatch, capsys):
    script(monkeypatch, [shares[2], shares[0]])
    assert cli.cmd_unseal(running_initialized, argparse.Namespace(shares=True)) == 0
    out = capsys.readouterr().out
    assert "state: unsealed" in out and "passphrase change --with-shares" in out


def test_passphrase_change_with_shares_then_new_passphrase_unseals(running_initialized, shares, monkeypatch, capsys):
    new = "a brand new passphrase"
    script(monkeypatch, [shares[0], shares[1]])
    assert cli.cmd_unseal(running_initialized, argparse.Namespace(shares=True)) == 0
    script(monkeypatch, [shares[1], shares[2], new, new])
    assert cli.cmd_passphrase_change(running_initialized, argparse.Namespace(with_shares=True)) == 0
    assert "passphrase changed" in capsys.readouterr().out
    assert cli.cmd_seal(running_initialized, NS) == 0
    script(monkeypatch, [new])
    assert cli.cmd_unseal(running_initialized, argparse.Namespace(shares=False)) == 0


def test_passphrase_change_with_the_current_passphrase_and_mismatched_new_one(running_initialized, monkeypatch, capsys):
    script(monkeypatch, [GOOD_PASSPHRASE])
    cli.cmd_unseal(running_initialized, argparse.Namespace(shares=False))
    script(monkeypatch, [GOOD_PASSPHRASE, "a brand new passphrase", "another new passphrase"])
    assert cli.cmd_passphrase_change(running_initialized, argparse.Namespace(with_shares=False)) == 1
    assert "do not match" in capsys.readouterr().err


def test_shares_rekey_flow_replaces_the_shares(running_initialized, shares, monkeypatch, capsys):
    terminal = Terminal(capsys, monkeypatch, [GOOD_PASSPHRASE])
    # unseal first: the prompt "Passphrase: " does not start with "Share", so it uses the script
    assert cli.cmd_unseal(running_initialized, argparse.Namespace(shares=False)) == 0
    terminal.answers = iter([GOOD_PASSPHRASE])
    assert cli.cmd_shares_rekey(running_initialized, argparse.Namespace(with_shares=False)) == 0
    out = terminal.out + capsys.readouterr().out
    new = shares_printed(out)
    assert len(new) == 3 and set(new).isdisjoint(shares)
    assert "new recovery shares active" in out
    assert cli.cmd_seal(running_initialized, NS) == 0
    script(monkeypatch, [shares[0], shares[1]])  # the old ones no longer open it
    assert cli.cmd_unseal(running_initialized, argparse.Namespace(shares=True)) == 1
    script(monkeypatch, [new[0], new[2]])
    assert cli.cmd_unseal(running_initialized, argparse.Namespace(shares=True)) == 0


def test_interrupting_the_rekey_confirmation_keeps_the_old_shares(running_initialized, shares, monkeypatch, capsys):
    script(monkeypatch, [GOOD_PASSPHRASE])
    cli.cmd_unseal(running_initialized, argparse.Namespace(shares=False))
    script(monkeypatch, [GOOD_PASSPHRASE, KeyboardInterrupt()])
    assert cli.cmd_shares_rekey(running_initialized, argparse.Namespace(with_shares=False)) == 1
    assert "old shares still work" in capsys.readouterr().err
    cli.cmd_seal(running_initialized, NS)
    script(monkeypatch, [shares[0], shares[1]])
    assert cli.cmd_unseal(running_initialized, argparse.Namespace(shares=True)) == 0
