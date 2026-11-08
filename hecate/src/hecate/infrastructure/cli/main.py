import argparse
import asyncio
import getpass
import signal
import sys
from pathlib import Path

from hecate.application.sealing.service import INIT_WARNING, SealingService
from hecate.application.sealing.settings import SealingSettings
from hecate.application.secrets.service import SecretsService
from hecate.infrastructure.clock import SystemClock
from hecate.infrastructure.config import ConfigError, HecateConfig
from hecate.infrastructure.control.client import ControlClient, ControlUnavailable
from hecate.infrastructure.control.server import ControlServer, RequestHandler
from hecate.infrastructure.crypto.software import SoftwareCryptoProvider
from hecate.infrastructure.logs import configure_logging
from hecate.infrastructure.process import harden_process
from hecate.infrastructure.store.sqlite import SqliteAccessLog, SqliteSealRepository, SqliteSecretStore


def _client(config: HecateConfig) -> ControlClient:
    return ControlClient(config.socket_path)


def _call(config: HecateConfig, op: str, args: dict) -> dict | None:
    """Send an operation; print the error and return None if it failed."""
    try:
        reply = _client(config).request(op, args)
    except ControlUnavailable as exc:
        print(f"error: {exc}", file=sys.stderr)
        return None
    if reply["status"] != "completed":
        error = reply["error"]
        extra = f" (retry in {error['retry_after_s']}s)" if "retry_after_s" in error else ""
        state = f" [state: {reply['data']['state']}]" if reply["data"].get("state") else ""
        print(f"error ({error['code']}): {error['message']}{extra}{state}", file=sys.stderr)
        return None
    return reply["data"]


def cmd_status(config: HecateConfig, _args: argparse.Namespace) -> int:
    data = _call(config, "status", {})
    if data is None:
        return 1
    print(f"state: {data['state']}")
    if data["recommend_passphrase_change"]:
        print("recommendation: unsealed with recovery shares; set a new passphrase")
    if data["consecutive_failures"]:
        print(f"failed attempts in a row: {data['consecutive_failures']}")
    if data["retry_after_s"]:
        print(f"next attempt allowed in {data['retry_after_s']}s")
    if "secrets_expired" in data:
        print(f"secrets expired: {data['secrets_expired']}")
        print(f"secrets expiring in the next 7 days: {data['secrets_expiring_soon']}")
    return 0


def _ask_new_passphrase() -> str | None:
    first = getpass.getpass("New passphrase (12 characters or more): ")
    if getpass.getpass("Repeat the passphrase: ") != first:
        print("error: the passphrases do not match", file=sys.stderr)
        return None
    return first


def cmd_init(config: HecateConfig, _args: argparse.Namespace) -> int:
    print(INIT_WARNING)
    if input("Type 'yes' to continue: ").strip().lower() != "yes":
        print("cancelled")
        return 1
    passphrase = _ask_new_passphrase()
    if passphrase is None:
        return 1
    begun = _call(config, "init.begin", {"passphrase": passphrase})
    if begun is None:
        return 1
    print("\nRecovery shares (any two recover Hécate). They are shown only once:")
    print("Keep each one in a different place and do not store them together.\n")
    for number, share in enumerate(begun["shares"], 1):
        print(f"  share {number}: {share}")
    print("\nTo confirm you saved them, enter two of them (nothing is echoed).")
    try:
        given = [getpass.getpass(f"Share {n} of 2: ") for n in (1, 2)]
    except (KeyboardInterrupt, EOFError):
        _call(config, "init.abort", {"init_id": begun["init_id"]})
        print("\ninitialization aborted: nothing was saved", file=sys.stderr)
        return 1
    if _call(config, "init.confirm", {"init_id": begun["init_id"], "shares": given}) is None:
        _call(config, "init.abort", {"init_id": begun["init_id"]})
        return 1
    print("Hécate is initialized and sealed. Unseal it with: hecate unseal")
    print("Clear this terminal's scrollback: the shares were displayed on it.")
    return 0


def _ask_shares(count: int = 2) -> list[str]:
    return [getpass.getpass(f"Recovery share {n} of {count}: ") for n in range(1, count + 1)]


def cmd_unseal(config: HecateConfig, args: argparse.Namespace) -> int:
    if getattr(args, "shares", False):
        data = _call(config, "unseal", {"shares": _ask_shares()})
    else:
        data = _call(config, "unseal", {"passphrase": getpass.getpass("Passphrase: ")})
    if data is None:
        return 1
    print(f"state: {data['state']}")
    if data["recommend_passphrase_change"]:
        print("Unsealed with recovery shares: set a new passphrase with: hecate passphrase change --with-shares")
    return 0


def cmd_passphrase_change(config: HecateConfig, args: argparse.Namespace) -> int:
    if args.with_shares:
        proof = {"shares": _ask_shares()}
    else:
        proof = {"passphrase": getpass.getpass("Current passphrase: ")}
    new = _ask_new_passphrase()
    if new is None:
        return 1
    data = _call(config, "passphrase.change", {"proof": proof, "new_passphrase": new})
    if data is None:
        return 1
    print(f"passphrase changed (configuration version {data['config_version']})")
    return 0


def cmd_shares_rekey(config: HecateConfig, args: argparse.Namespace) -> int:
    if args.with_shares:
        proof = {"shares": _ask_shares()}
    else:
        proof = {"passphrase": getpass.getpass("Current passphrase: ")}
    begun = _call(config, "shares.rekey.begin", {"proof": proof})
    if begun is None:
        return 1
    print("\nNew recovery shares (any two recover Hécate). The old ones stop working once you confirm:")
    for number, share in enumerate(begun["shares"], 1):
        print(f"  share {number}: {share}")
    print("\nTo confirm you saved them, enter two of them (nothing is echoed).")
    try:
        given = [getpass.getpass(f"Share {n} of 2: ") for n in (1, 2)]
    except (KeyboardInterrupt, EOFError):
        _call(config, "shares.rekey.abort", {"rekey_id": begun["rekey_id"]})
        print("\nrekey aborted: the old shares still work", file=sys.stderr)
        return 1
    data = _call(config, "shares.rekey.confirm", {"rekey_id": begun["rekey_id"], "shares": given})
    if data is None:
        _call(config, "shares.rekey.abort", {"rekey_id": begun["rekey_id"]})
        return 1
    print(f"new recovery shares active (configuration version {data['config_version']})")
    print("Clear this terminal's scrollback: the shares were displayed on it.")
    return 0


def cmd_seal(config: HecateConfig, _args: argparse.Namespace) -> int:
    data = _call(config, "seal", {})
    if data is None:
        return 1
    print(f"state: {data['state']}")
    return 0


def _read_value(from_file: str | None) -> str | None:
    """The value of a secret: from a file or typed without echo, never from the command line."""
    if from_file:
        try:
            return Path(from_file).read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError) as exc:
            print(f"error: cannot read {from_file}: {exc.__class__.__name__}", file=sys.stderr)
            return None
    return getpass.getpass("Value (nothing is echoed): ")


def cmd_secrets_create(config: HecateConfig, args: argparse.Namespace) -> int:
    value = _read_value(args.from_file)
    if value is None:
        return 1
    request = {"path": args.path, "value": value, "value_type": args.type}
    for option in ("max_versions", "rotate_every", "expires_at"):
        if getattr(args, option) is not None:
            request[option] = getattr(args, option)
    data = _call(config, "secrets.create", request)
    if data is None:
        return 1
    print(f"created {data['path']} version {data['version']} fingerprint {data['fingerprint']}")
    return 0


def cmd_secrets_list(config: HecateConfig, args: argparse.Namespace) -> int:
    data = _call(config, "secrets.list", {"identity": args.identity} if args.identity else {})
    if data is None:
        return 1
    for item in data["secrets"]:
        active = item["active_version"] if item["active_version"] is not None else "-"
        expires = f" expires {item['expires_at']}" if item["expires_at"] else ""
        print(f"{item['path']}  {item['value_type']}  active {active}  versions {item['version_count']}{expires}")
    if not data["secrets"]:
        print("no secrets")
    return 0


def cmd_secrets_get(config: HecateConfig, args: argparse.Namespace) -> int:
    data = _call(config, "secrets.get", {"path": args.path})
    if data is None:
        return 1
    print(f"{data['path']}  ({data['value_type']}, owner {data['owner']}, max versions {data['max_versions']})")
    if data["rotate_every"]:
        print(f"rotate every: {data['rotate_every']}")
    if data["expires_at"]:
        print(f"expires at: {data['expires_at']}")
    for version in data["versions"]:
        print(f"  version {version['version']}  {version['state']}  {version['created_at']}  "
              f"fingerprint {version['fingerprint']}")
    return 0


def _read_stream_version(config: HecateConfig, path: str) -> int | None:
    """The version of the secret that the next change must expect (see `secrets.get`)."""
    data = _call(config, "secrets.get", {"path": path})
    return None if data is None else data["stream_version"]


def cmd_secrets_put(config: HecateConfig, args: argparse.Namespace) -> int:
    expected = _read_stream_version(config, args.path)
    if expected is None:
        return 1
    value = _read_value(args.from_file)
    if value is None:
        return 1
    data = _call(config, "secrets.add_version",
                 {"path": args.path, "value": value, "expected_version": expected})
    if data is None:
        return 1
    print(f"added version {data['version']} to {args.path} fingerprint {data['fingerprint']}")
    return 0


def _cmd_secrets_change(op: str, done: str):
    def run(config: HecateConfig, args: argparse.Namespace) -> int:
        expected = _read_stream_version(config, args.path)
        if expected is None:
            return 1
        data = _call(config, op, {"path": args.path, "version": args.version, "expected_version": expected})
        if data is None:
            return 1
        print(f"version {args.version} of {args.path} {done}")
        return 0
    return run


def cmd_secrets_destroy(config: HecateConfig, args: argparse.Namespace) -> int:
    expected = _read_stream_version(config, args.path)
    if expected is None:
        return 1
    print(f"Destroying version {args.version} of {args.path} erases its value for good.")
    if args.with_shares:
        proof = {"shares": _ask_shares()}
    else:
        proof = {"passphrase": getpass.getpass("Current passphrase: ")}
    data = _call(config, "secrets.destroy", {
        "path": args.path, "version": args.version, "expected_version": expected, "proof": proof,
    })
    if data is None:
        return 1
    print(f"version {args.version} of {args.path} destroyed")
    return 0


async def _serve(server: ControlServer, service: SealingService) -> None:
    await server.start()
    stop = asyncio.Event()
    loop = asyncio.get_running_loop()
    for sig in (signal.SIGINT, signal.SIGTERM):
        loop.add_signal_handler(sig, stop.set)
    await stop.wait()
    service.shutdown()
    await server.stop()


def cmd_serve(config: HecateConfig, _args: argparse.Namespace) -> int:
    configure_logging()
    config.prepare()
    harden_process()
    repo = SqliteSealRepository(config.db_path)
    access_log = SqliteAccessLog(config.db_path)
    service = SealingService(
        crypto=SoftwareCryptoProvider(),
        repo=repo,
        access_log=access_log,
        clock=SystemClock(),
        settings=SealingSettings(kdf=config.kdf),
    )
    store = SqliteSecretStore(config.db_path)
    secrets = SecretsService(
        sealing=service, store=store, crypto=SoftwareCryptoProvider(),
        access_log=access_log, clock=SystemClock(),
    )
    server = ControlServer(RequestHandler(service, secrets), config.socket_path)
    try:
        asyncio.run(_serve(server, service))
    finally:
        repo.close()
        access_log.close()
        store.close()
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="hecate", description="Hécate: secrets and keys")
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("serve", help="run Hécate (always starts sealed)").set_defaults(func=cmd_serve)
    sub.add_parser("init", help="initialize Hécate (once)").set_defaults(func=cmd_init)
    unseal = sub.add_parser("unseal", help="unseal with the admin passphrase or two recovery shares")
    unseal.add_argument("--shares", action="store_true", help="use two recovery shares instead")
    unseal.set_defaults(func=cmd_unseal)
    passphrase = sub.add_parser("passphrase", help="manage the admin passphrase").add_subparsers(
        dest="passphrase_command", required=True
    )
    change = passphrase.add_parser("change", help="set a new passphrase (needs a proof)")
    change.add_argument("--with-shares", action="store_true", help="prove with two recovery shares")
    change.set_defaults(func=cmd_passphrase_change)
    shares = sub.add_parser("shares", help="manage recovery shares").add_subparsers(
        dest="shares_command", required=True
    )
    rekey = shares.add_parser("rekey", help="issue a new set of recovery shares (needs a proof)")
    rekey.add_argument("--with-shares", action="store_true", help="prove with two current shares")
    rekey.set_defaults(func=cmd_shares_rekey)
    secrets = sub.add_parser("secrets", help="manage secrets (values are written, never read)").add_subparsers(
        dest="secrets_command", required=True
    )
    create = secrets.add_parser("create", help="store a new secret (version 1)")
    create.add_argument("path", help="secret://<identity>/<path>")
    create.add_argument("--type", required=True, choices=["string", "json", "pem"], help="type of the value")
    create.add_argument("--from-file", help="read the value from a file instead of typing it")
    create.add_argument("--max-versions", dest="max_versions", type=int, help="versions kept, from 2 to 100 (default 10)")
    create.add_argument("--rotate-every", dest="rotate_every", help="informative rotation period, e.g. 90d")
    create.add_argument("--expires-at", dest="expires_at", help="informative expiry, ISO 8601")
    create.set_defaults(func=cmd_secrets_create)
    listing = secrets.add_parser("list", help="list secret paths with their metadata")
    listing.add_argument("--identity", help="only the secrets of this identity")
    listing.set_defaults(func=cmd_secrets_list)
    get = secrets.add_parser("get", help="show a secret's metadata and versions (never the value)")
    get.add_argument("path")
    get.set_defaults(func=cmd_secrets_get)
    put = secrets.add_parser("put", help="add a new version to a secret")
    put.add_argument("path")
    put.add_argument("--from-file", help="read the value from a file instead of typing it")
    put.set_defaults(func=cmd_secrets_put)
    for name, past, summary in (
        ("revoke", "revoked", "revoke a version at once"),
        ("restore", "restored", "restore a revoked version (it becomes deprecated)"),
        ("promote", "promoted", "make a deprecated version the active one"),
    ):
        change = secrets.add_parser(name, help=summary)
        change.add_argument("path")
        change.add_argument("--version", type=int, required=True)
        change.set_defaults(func=_cmd_secrets_change(f"secrets.{name}", past))
    destroy = secrets.add_parser("destroy", help="destroy a revoked version for good (needs a proof)")
    destroy.add_argument("path")
    destroy.add_argument("--version", type=int, required=True)
    destroy.add_argument("--with-shares", action="store_true", help="prove with two recovery shares")
    destroy.set_defaults(func=cmd_secrets_destroy)
    sub.add_parser("seal", help="seal now: the root leaves memory").set_defaults(func=cmd_seal)
    sub.add_parser("status", help="show the seal state").set_defaults(func=cmd_status)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        config = HecateConfig.from_env()
    except ConfigError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2
    return args.func(config, args)


if __name__ == "__main__":
    sys.exit(main())
