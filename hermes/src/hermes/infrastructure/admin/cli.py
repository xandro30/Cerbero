"""The CLI `hermes` (contracts/cli.md): `serve` runs the service; the other orders talk to it through the local channel.

Exit codes: 0 done; 1 the operation failed, was cancelled or Hermes cannot be reached; 2 wrong usage or a variable
that does not fit.
"""

from __future__ import annotations

import argparse
import getpass
import os
import sys
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Any, TextIO

from hermes.infrastructure.admin.client import AdminClient, AdminFailure, AdminUnreachable
from hermes.infrastructure.settings import SettingsError, load_cli_settings, load_serve_settings


@dataclass
class Context:
    env: Mapping[str, str]
    out: TextIO
    err: TextIO
    inp: TextIO

    def call(self, method: str, path: str, body: Any = None) -> dict[str, Any]:
        settings = load_cli_settings(self.env)
        return AdminClient(settings.admin_socket).call(method, path, body)


@dataclass(frozen=True)
class Command:
    name: str
    help: str
    configure: Callable[[argparse.ArgumentParser], None]
    run: Callable[[argparse.Namespace, Context], int]


def _no_arguments(_: argparse.ArgumentParser) -> None:
    return None


def _serve(_: argparse.Namespace, context: Context) -> int:
    settings = load_serve_settings(context.env)
    from hermes.infrastructure.main import run_serve

    return run_serve(settings, context.err)


class Cancelled(Exception):
    """A question without an answer: the CLI writes `cancelled` and exits with 1."""


def _enrollment_token(context: Context) -> str:
    """Asked without echo in a terminal, or read from the standard input; never an argument (research R4)."""
    if context.inp.isatty():
        try:
            token = getpass.getpass("Enrollment token: ")
        except EOFError, KeyboardInterrupt:
            raise Cancelled from None
    else:
        token = context.inp.readline()
    token = token.strip()
    if not token:
        raise Cancelled
    return token


def _enroll(_: argparse.Namespace, context: Context) -> int:
    data = context.call("POST", "/identity/enroll", {"token": _enrollment_token(context)})
    context.out.write(f"enrolled {data['spiffe_id']} serial {data['serial']} until {data['not_after']}\n")
    return 0


SYMBOLS = {"create": "+", "update": "~", "delete": "-"}


def _change_line(change: dict[str, Any]) -> str:
    line = f"{SYMBOLS[change['action']]} {change['kind']} {change['name']}"
    return f"{line} ({change['detail']})" if change.get("detail") else line


def _configure_config(parser: argparse.ArgumentParser) -> None:
    orders = parser.add_subparsers(dest="config_order", required=True, metavar="<order>")
    apply = orders.add_parser("apply", help="Apply a configuration file after showing its changes")
    apply.add_argument("file")
    apply.add_argument("--yes", action="store_true", help="Apply without asking")
    export = orders.add_parser("export", help="Write the configuration in force as a file")
    export.add_argument("--out", help="A new file, created with permissions 0600; it is never overwritten")


def _config_apply(args: argparse.Namespace, context: Context) -> int:
    try:
        text = Path(args.file).read_bytes().decode("utf-8")
    except OSError as error:
        context.err.write(f"error: cannot read {args.file}: {error.strerror or error}\n")
        return 1
    except UnicodeDecodeError:
        context.err.write(f"error: cannot read {args.file}: the file is not UTF-8 text\n")
        return 1
    plan = context.call("POST", "/config/plan", {"yaml": text})
    changes = plan["changes"]
    if not changes:
        context.out.write("no changes\n")
        return 0
    for change in changes:
        context.out.write(_change_line(change) + "\n")
    if not args.yes:
        context.out.write(f"apply these {len(changes)} changes? [yes/no] ")
        context.out.flush()
        if context.inp.readline().strip() != "yes":
            raise Cancelled
    applied = context.call("POST", "/config/apply", {"yaml": text, "plan_digest": plan["plan_digest"]})
    context.out.write(f"applied {len(applied['changes'])} changes\n")
    return 0


def _config_export(args: argparse.Namespace, context: Context) -> int:
    exported: str = context.call("GET", "/config/export")["yaml"]
    if args.out is None:
        context.out.write(exported)
        return 0
    target = Path(args.out)
    try:
        descriptor = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    except FileExistsError:
        context.err.write(f"error: {target} already exists; it is not overwritten\n")
        return 1
    except OSError as error:
        context.err.write(f"error: cannot write {target}: {error.strerror or error}\n")
        return 1
    with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
        handle.write(exported)
    context.out.write(f"exported to {target}\n")
    return 0


def _config(args: argparse.Namespace, context: Context) -> int:
    return _config_apply(args, context) if args.config_order == "apply" else _config_export(args, context)


def _configure_contacts(parser: argparse.ArgumentParser) -> None:
    orders = parser.add_subparsers(dest="contacts_order", required=True, metavar="<order>")
    code = orders.add_parser("code", help="A one-time code for the channel of a contact in an instance")
    code.add_argument("contact")
    code.add_argument("--instance", required=True)


def _contacts(args: argparse.Namespace, context: Context) -> int:
    data = context.call("POST", "/contacts/code", {"contact": args.contact, "instance": args.instance})
    context.out.write(f"code {data['code']} for {args.contact} on {args.instance} expires {data['expires_at']}\n")
    context.out.write("send it to the bot from the person's Telegram account\n")
    return 0


def _configure_health(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--live", action="store_true", help="Exit with 0 while the process answers, ready or not")


def _health(args: argparse.Namespace, context: Context) -> int:
    data = context.call("GET", "/health")
    if args.live:
        context.out.write("live\n")
        return 0
    if data["ready"]:
        context.out.write("ready\n")
        return 0
    context.out.write(f"not ready: {data.get('reason', 'unknown')}\n")
    return 1


COMMANDS: dict[str, Command] = {
    "serve": Command("serve", "Run Hermes in the foreground until SIGINT or SIGTERM", _no_arguments, _serve),
    "enroll": Command("enroll", "Enroll Hermes in Hécate with an enrollment token", _no_arguments, _enroll),
    "config": Command("config", "Apply or export the configuration", _configure_config, _config),
    "contacts": Command("contacts", "Verify the channels of the contacts", _configure_contacts, _contacts),
    "health": Command(
        "health", "Whether Hermes is ready, or with --live whether it answers", _configure_health, _health
    ),
}


def build_parser(commands: Mapping[str, Command]) -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="hermes", description="Hermes: third-party libraries with a contract.")
    orders = parser.add_subparsers(dest="order", required=True, metavar="<order>")
    for command in commands.values():
        command.configure(orders.add_parser(command.name, help=command.help))
    return parser


def main(
    argv: Sequence[str] | None = None,
    *,
    env: Mapping[str, str] | None = None,
    commands: Mapping[str, Command] | None = None,
    stdin: TextIO | None = None,
) -> int:
    known = commands if commands is not None else COMMANDS
    args = build_parser(known).parse_args(argv)
    context = Context(
        env=os.environ if env is None else env, out=sys.stdout, err=sys.stderr, inp=stdin if stdin else sys.stdin
    )
    try:
        return known[args.order].run(args, context)
    except Cancelled:
        context.err.write("cancelled\n")
        return 1
    except SettingsError as error:
        context.err.write(f"error: {error}\n")
        return 2
    except AdminUnreachable as error:
        context.err.write(f"error: cannot reach hermes at {error.socket}: {error.reason}\n")
        return 1
    except AdminFailure as error:
        context.err.write(f"error: {error.code}: {error.message}\n")
        for detail in error.details:
            context.err.write(f"error: {detail.get('path', '')}: {detail.get('message', '')}\n")
        return 1
