"""Test vectors of change 005, generated with the Python Hécate of main@0287c38 before it is deleted.

Usage, from the repository root:

    uv run --package cerbero-hecate python specs/005-hecate-rust/vectors/generate.py hecate/tests/vectors

It writes the vector files of research R5 into the given directory and the Unicode 17 table of
printable characters into hecate/crates/domain/src/printable_table.rs (research R7).
"""

import base64
import binascii
import datetime as dt
import json
import math
import re
import sys
import tempfile
import unicodedata
from pathlib import Path

from cryptography.hazmat.primitives.ciphers.aead import AESGCM

from hecate.application.secrets.envelope import Envelope
from hecate.application.secrets.service import _instant
from hecate.domain.events import encode_event
from hecate.domain.keys.key import ScopeKeyCreated
from hecate.domain.sealing.errors import InvalidInput
from hecate.domain.sealing.protection import (
    AEAD_NAME,
    KDF_NAME,
    PassphraseProtection,
    RecoveryProtection,
    associated_data,
)
from hecate.domain.secrets.secret import (
    SecretCreated,
    VersionAdded,
    VersionDeprecated,
    VersionDestroyed,
    VersionPromoted,
    VersionRestored,
    VersionRevoked,
)
from hecate.domain.secrets.value_type import ValueType
from hecate.infrastructure.crypto.software import SoftwareCryptoProvider
from hecate.infrastructure.store import sqlite as store

SOURCE = "main@0287c38"
GENERATOR = "specs/005-hecate-rust/vectors/generate.py"
TABLE = Path(__file__).resolve().parents[3] / "hecate/crates/domain/src/printable_table.rs"

MIN_OPS, MIN_MEM = 1, 8192
MODERATE_OPS, MODERATE_MEM = 3, 256 << 20
PASSPHRASE = "correct horse battery staple"
PATH = "secret://hermes/telegram/family_bot/bot_token"

# FR-018: standard JSON is what serde_json reads as a value (at most 127 nesting levels).
JSON_DEPTH_LIMIT = 127
ASCII_SPACE = " \t\n\r\x0b\x0c"
_PEM_STRICT = re.compile(
    r"-----BEGIN ([A-Z0-9 ]+)-----\s*\n(?P<body>[A-Za-z0-9+/=\s]+?)\s*\n-----END \1-----", re.ASCII
)
_EXTENDED_ISO = re.compile(
    r"\d{4}-\d{2}-\d{2}"
    r"(?:[T ](?:[01]\d|2[0-3]):[0-5]\d(?::[0-5]\d(?:\.\d+)?)?(?:Z|[+-](?:[01]\d|2[0-3]):[0-5]\d)?)?",
    re.ASCII,
)

crypto = SoftwareCryptoProvider()
envelope = Envelope(crypto)


def fixed(size: int, start: int) -> bytes:
    return bytes((start + i) % 256 for i in range(size))


def write(directory: Path, name: str, cases: object) -> None:
    document = {
        "generator": GENERATOR,
        "source": SOURCE,
        "python": sys.version.split()[0],
        "unicode": unicodedata.unidata_version,
        "cases": cases,
    }
    text = json.dumps(document, indent=2, ensure_ascii=False) + "\n"
    (directory / name).write_text(text, encoding="utf-8")


def kdf_cases() -> list[dict]:
    cases = []
    for passphrase, salt, ops, mem in [
        (PASSPHRASE, fixed(16, 0), MIN_OPS, MIN_MEM),
        ("contraseña con eñe y 🔐", fixed(16, 100), MIN_OPS, MIN_MEM),
        (PASSPHRASE, fixed(16, 200), MODERATE_OPS, MODERATE_MEM),
    ]:
        key = crypto.derive_key(passphrase, salt, ops, mem)
        cases.append(
            {"passphrase": passphrase, "salt": salt.hex(), "opslimit": ops, "memlimit": mem, "key": key.hex()}
        )
    return cases


def protection_fields(protection: PassphraseProtection | RecoveryProtection) -> dict:
    fields = {k: (v.hex() if isinstance(v, bytes) else v) for k, v in vars(protection).items()}
    return {**fields, "aad": protection.aad().decode(), "stored": store._dump(protection)}


def root_cases() -> list[dict]:
    root = fixed(32, 7)
    salt = fixed(16, 30)
    key = crypto.derive_key(PASSPHRASE, salt, MIN_OPS, MIN_MEM)
    nonce = fixed(12, 60)
    passphrase = PassphraseProtection(
        version=1, kdf=KDF_NAME, kdf_opslimit=MIN_OPS, kdf_memlimit=MIN_MEM, salt=salt, aead=AEAD_NAME,
        nonce=nonce, ciphertext=AESGCM(key).encrypt(nonce, root, associated_data("passphrase", 1)),
    )
    recovery_key = fixed(32, 90)
    recovery_nonce = fixed(12, 140)
    recovery = RecoveryProtection(
        version=1, share_set_id=fixed(4, 130), threshold=2, share_count=3, aead=AEAD_NAME,
        nonce=recovery_nonce,
        ciphertext=AESGCM(recovery_key).encrypt(recovery_nonce, root, associated_data("recovery", 1)),
    )
    cases = [{
        "kind": "fixed", "root": root.hex(), "passphrase": PASSPHRASE, "recovery_key": recovery_key.hex(),
        "passphrase_protection": protection_fields(passphrase), "recovery_protection": protection_fields(recovery),
    }]

    root = crypto.random_bytes(32)
    salt = crypto.random_bytes(16)
    key = crypto.derive_key(PASSPHRASE, salt, MIN_OPS, MIN_MEM)
    nonce, ciphertext = crypto.aead_encrypt(key, root, associated_data("passphrase", 3))
    passphrase = PassphraseProtection(
        version=3, kdf=KDF_NAME, kdf_opslimit=MIN_OPS, kdf_memlimit=MIN_MEM, salt=salt, aead=AEAD_NAME,
        nonce=nonce, ciphertext=ciphertext,
    )
    recovery_key = crypto.random_bytes(32)
    nonce, ciphertext = crypto.aead_encrypt(recovery_key, root, associated_data("recovery", 3))
    recovery = RecoveryProtection(
        version=3, share_set_id=crypto.random_bytes(4), threshold=2, share_count=3, aead=AEAD_NAME,
        nonce=nonce, ciphertext=ciphertext,
    )
    cases.append({
        "kind": "random", "root": root.hex(), "passphrase": PASSPHRASE, "recovery_key": recovery_key.hex(),
        "passphrase_protection": protection_fields(passphrase), "recovery_protection": protection_fields(recovery),
    })
    return cases


def aad_cases() -> list[dict]:
    other = "secret://tenaro-web/db/password"
    return [
        {"layer": "root", "protection": "passphrase", "config_version": 1,
         "aad": associated_data("passphrase", 1).decode()},
        {"layer": "root", "protection": "recovery", "config_version": 12,
         "aad": associated_data("recovery", 12).decode()},
        {"layer": "scope_key", "identity": "hermes", "key_version": 1,
         "aad": Envelope._scope_aad("hermes", 1).decode()},
        {"layer": "scope_key", "identity": "tenaro-web", "key_version": 3,
         "aad": Envelope._scope_aad("tenaro-web", 3).decode()},
        {"layer": "dek", "path": PATH, "version": 3, "scope": "hermes", "key_version": 1,
         "aad": Envelope._dek_aad(PATH, 3, "hermes", 1).decode()},
        {"layer": "dek", "path": other, "version": 100, "scope": "tenaro-web", "key_version": 2,
         "aad": Envelope._dek_aad(other, 100, "tenaro-web", 2).decode()},
        {"layer": "value", "path": PATH, "version": 3, "scope": "hermes",
         "aad": Envelope._value_aad(PATH, 3, "hermes").decode()},
    ]


def envelope_cases() -> list[dict]:
    root, scope_key, dek = fixed(32, 11), fixed(32, 51), fixed(32, 91)
    scope_nonce, dek_nonce, value_nonce = fixed(12, 1), fixed(12, 21), fixed(12, 41)
    identity, version, key_version = "hermes", 2, 1
    value = "token-ñ-🔑"
    scope_aad = Envelope._scope_aad(identity, key_version)
    dek_aad = Envelope._dek_aad(PATH, version, identity, key_version)
    value_aad = Envelope._value_aad(PATH, version, identity)
    cases = [{
        "kind": "fixed", "root": root.hex(), "identity": identity, "key_version": key_version,
        "path": PATH, "version": version, "value": value,
        "scope_key": scope_key.hex(), "scope_key_nonce": scope_nonce.hex(),
        "scope_key_ciphertext": AESGCM(root).encrypt(scope_nonce, scope_key, scope_aad).hex(),
        "dek": dek.hex(), "dek_nonce": dek_nonce.hex(),
        "dek_ciphertext": AESGCM(scope_key).encrypt(dek_nonce, dek, dek_aad).hex(),
        "value_nonce": value_nonce.hex(),
        "value_ciphertext": AESGCM(dek).encrypt(value_nonce, value.encode(), value_aad).hex(),
    }]
    root = crypto.random_bytes(32)
    for identity, path, version, value in [
        ("hermes", PATH, 1, "plain value"),
        ("tenaro-web", "secret://tenaro-web/db/password", 7, "contraseña"),
    ]:
        scope_key, key_material = envelope.new_scope_key(root, identity, 1)
        material = envelope.seal_value(scope_key, path, version, identity, 1, value.encode())
        cases.append({
            "kind": "random", "root": root.hex(), "identity": identity, "key_version": 1,
            "path": path, "version": version, "value": value,
            "scope_key": scope_key.hex(), "scope_key_nonce": key_material.nonce.hex(),
            "scope_key_ciphertext": key_material.ciphertext.hex(),
            "dek_nonce": material.dek_nonce.hex(), "dek_ciphertext": material.dek_ciphertext.hex(),
            "value_nonce": material.value_nonce.hex(), "value_ciphertext": material.value_ciphertext.hex(),
        })
    return cases


def fingerprint_cases() -> list[dict]:
    cases = []
    for scope_key, value in [
        (fixed(32, 3), "hunter2"),
        (fixed(32, 3), "contraseña 🔑"),
        (fixed(32, 200), "hunter2"),
        (fixed(32, 0), "a" * 1000),
    ]:
        cases.append(
            {"scope_key": scope_key.hex(), "value": value,
             "fingerprint": envelope.fingerprint(scope_key, value.encode())}
        )
    return cases


def events_cases() -> list[dict]:
    at = dt.datetime(2026, 11, 8, 12, 30, 15, 123456, tzinfo=dt.UTC)
    at_whole = dt.datetime(2026, 11, 8, 12, 30, tzinfo=dt.UTC)
    expires = dt.datetime(2027, 6, 1, 9, 0, tzinfo=dt.timezone(dt.timedelta(hours=2)))
    events = [
        SecretCreated(at, PATH, "hermes", "string", 10, "cada 30 días", expires),
        SecretCreated(at_whole, "secret://tenaro-web/db/password", "tenaro-web", "json", 2, None, None),
        VersionAdded(at, 1, "1a2b-3c4d-5e6f-7a8b", 1),
        VersionDeprecated(at, 1),
        VersionRevoked(at, 2),
        VersionRestored(at, 3),
        VersionPromoted(at, 4),
        VersionDestroyed(at, 5, "admin"),
        VersionDestroyed(at_whole, 6, "max_versions"),
        ScopeKeyCreated(at, "hermes", 1),
    ]
    cases = []
    for event in events:
        type_name, data, instant = encode_event(event)
        cases.append({"type": type_name, "data": json.dumps(data, sort_keys=True), "at": instant.isoformat()})
    return cases


def instants_cases() -> list[dict]:
    cases = []
    for value in [
        dt.datetime(2026, 11, 8, 12, 0, tzinfo=dt.UTC),
        dt.datetime(2026, 11, 8, 12, 0, 0, 1, tzinfo=dt.UTC),
        dt.datetime(2027, 6, 1, 9, 0, 0, 123456, tzinfo=dt.timezone(dt.timedelta(hours=2))),
        dt.datetime(2026, 1, 2, 3, 4, 5, tzinfo=dt.timezone(-dt.timedelta(hours=5, minutes=30))),
    ]:
        cases.append({
            "kind": "isoformat", "year": value.year, "month": value.month, "day": value.day,
            "hour": value.hour, "minute": value.minute, "second": value.second,
            "microsecond": value.microsecond, "offset_seconds": int(value.utcoffset().total_seconds()),
            "text": value.isoformat(),
        })
    for text in [
        "2027-01-01T00:00:00+00:00", "2027-01-01T00:00:00Z", "2027-01-01T00:00:00",
        "2027-01-01 08:30:00+02:00", "2027-01-01T08:30+02:00", "2027-01-01T08:30", "2027-01-01",
        "2027-06-01T09:00:00.5+02:00", "2027-06-01T09:00:00.123456-05:30",
        "2027-06-01T09:00:00.1234567+00:00", "2027-06-01T09:00:00.000000Z", "2024-02-29T12:00:00+00:00",
        "2027-01-01t08:30:00+00:00", "2027-01-01X08:30:00", "2027-01-01ñ08:30:00", "20270101",
        "20270101T083000", "2027-01-01T083000", "2027-W01-5", "2027W015", "2027-001",
        "2027-01-01T24:00:00", "2027-01-01T08:30:00+0200", "2027-01-01T08:30:00+02",
        "2027-01-01T08:30:00+02:00:30", "2027-01-01T08:30:00+02:00:30.5", "2027-01-01T08",
        "2027-01-01T08:30:00,5+00:00", "tomorrow", "", "2027-13-01", "2027-02-30",
        "2027-01-01T25:00:00", "2027-01-01T08:60:00", "2027-01-01T08:30:60", " 2027-01-01",
        "2027-01-01 ", "2027-1-1", "27-01-01", "2027-01-01T08:30:00+24:00", "2027-01-01T08:30:00ZZ",
    ]:
        try:
            result, error = _instant(text).isoformat(), None
        except InvalidInput as exc:
            result, error = None, str(exc)
        deliberate = result is not None and _EXTENDED_ISO.fullmatch(text) is None
        cases.append(
            {"kind": "parse", "input": text, "result": result, "error": error, "deliberate": deliberate}
        )
    return cases


def nesting_depth(document: object) -> int:
    deepest, stack = 0, [(document, 1)]
    while stack:
        node, depth = stack.pop()
        if isinstance(node, (list, dict)):
            deepest = max(deepest, depth)
            children = node.values() if isinstance(node, dict) else node
            stack.extend((child, depth + 1) for child in children)
    return deepest


def has_lone_surrogate(document: object) -> bool:
    stack = [document]
    while stack:
        node = stack.pop()
        if isinstance(node, str):
            if any("\ud800" <= ch <= "\udfff" for ch in node):
                return True
        elif isinstance(node, dict):
            stack.extend(node.keys())
            stack.extend(node.values())
        elif isinstance(node, list):
            stack.extend(node)
    return False


def standard_json(text: str) -> bool:
    def reject(name: str) -> None:
        raise ValueError(name)

    def finite_float(literal: str) -> float:
        number = float(literal)
        if math.isinf(number):
            raise ValueError("out of range")
        return number

    def representable_int(literal: str) -> int:
        number = int(literal)
        float(number)
        return number

    try:
        document = json.loads(text, parse_constant=reject, parse_float=finite_float, parse_int=representable_int)
    except (ValueError, OverflowError):
        return False
    return nesting_depth(document) <= JSON_DEPTH_LIMIT and not has_lone_surrogate(document)


def standard_pem(value: str) -> str | None:
    text = value.strip(ASCII_SPACE)
    position, blocks = 0, 0
    while position < len(text):
        match = _PEM_STRICT.match(text, position)
        if match is None:
            return "the value is not a valid PEM"
        body = re.sub(r"\s", "", match["body"], flags=re.ASCII)
        try:
            decoded = base64.b64decode(body, validate=True)
        except (binascii.Error, ValueError):
            return "a PEM block does not contain valid base64"
        if not decoded:
            return "a PEM block is empty"
        blocks += 1
        position = match.end()
        while position < len(text) and text[position] in ASCII_SPACE:
            position += 1
    return None if blocks else "the value is not a valid PEM"


def standard_error(value_type: str, value: str) -> str | None:
    data = value.encode()
    if not data:
        return "the value must not be empty"
    if len(data) > 64 * 1024:
        return "the value is larger than 64 KiB"
    if value_type == "json":
        return None if standard_json(value) else "the value is not valid JSON"
    if value_type == "pem":
        return standard_pem(value)
    return None


def pem_block(label: str, payload: bytes, newline: str = "\n") -> str:
    body = base64.b64encode(payload).decode()
    lines = [body[i:i + 64] for i in range(0, len(body), 64)]
    return newline.join([f"-----BEGIN {label}-----", *lines, f"-----END {label}-----"]) + newline


def value_cases() -> list[dict]:
    cert = pem_block("CERTIFICATE", fixed(120, 5))
    key = pem_block("EC PRIVATE KEY", fixed(48, 77))
    begin, end = "-----BEGIN CERTIFICATE-----\n", "-----END CERTIFICATE-----\n"
    inputs = [
        ("string", "hello"), ("string", "ñandú 🎉"), ("string", ""), ("string", "   "),
        ("json", '{"a": [1, 2]}'), ("json", "[]"), ("json", "{}"), ("json", "1"), ("json", "-0"),
        ("json", "1.5e300"), ("json", '"texto ñ"'), ("json", "true"), ("json", "null"),
        ("json", ' {"a":1} '), ("json", '{"a":1,"a":2}'), ("json", "12345678901234567890123"),
        ("json", '"\\u00e9"'), ("json", '"\\ud83d\\ude00"'),
        ("json", "[" * 127 + "]" * 127), ("json", "[" * 128 + "]" * 128),
        ("json", "[" * 32768 + "]" * 32768),
        ("json", "NaN"), ("json", "Infinity"), ("json", "-Infinity"), ("json", "1e400"), ("json", "-1e400"),
        ("json", "1" + "0" * 400), ("json", '"\\ud800"'), ("json", '"\\udc00x"'), ("json", '{"\\ud800": 1}'),
        ("json", "{not json"), ("json", ""), ("json", "{'a': 1}"), ("json", "[1,]"), ("json", "01"),
        ("json", ".5"), ("json", "+1"), ("json", '{"a":1} x'), ("json", " {}"), ("json", "﻿{}"),
        ("json", '"a\tb"'),
        ("pem", cert), ("pem", cert + key), ("pem", cert.replace("\n", "\r\n")), ("pem", "\n\n" + cert + "\n\n"),
        ("pem", "\t" + cert.replace("\n", "\n\t")), ("pem", cert.replace("CERTIFICATE", "certificate")),
        ("pem", begin + "MIIB\n-----END PRIVATE KEY-----\n"), ("pem", begin + "\n\n" + end),
        ("pem", begin + " \n" + end), ("pem", begin + "QQ==\n" + end), ("pem", begin + "QQ\n" + end),
        ("pem", begin + "QQ==QQ==\n" + end), ("pem", begin + "QR==\n" + end), ("pem", begin + "QUJD\n" + end),
        ("pem", begin + "QUJD REVG\n" + end), ("pem", begin + "QUJD\x1cREVG\n" + end),
        ("pem", begin + "QUJD REVG\n" + end), ("pem", begin + "QUJD\x0bREVG\n" + end),
        ("pem", "not a pem"), ("pem", "junk\n" + cert), ("pem", cert + "junk\n"),
    ]
    cases = []
    for value_type, value in inputs:
        try:
            ValueType(value_type).validate(value)
            python = None
        except InvalidInput as exc:
            python = str(exc)
        standard = standard_error(value_type, value)
        if python is not None and standard is None:
            raise SystemExit(f"FR-018 would accept what Python rejects: {value_type} {value[:40]!r}")
        deliberate = python is None and standard is not None
        cases.append({
            "type": value_type, "value": value, "python_error": python, "deliberate": deliberate,
            "expected_error": standard if deliberate else python,
        })
    return cases


def printable_ranges() -> list[list[int]]:
    ranges: list[list[int]] = []
    for code in range(0x110000):
        if chr(code).isprintable():
            if ranges and ranges[-1][1] == code - 1:
                ranges[-1][1] = code
            else:
                ranges.append([code, code])
    return ranges


def write_table(ranges: list[list[int]]) -> None:
    lines = [
        f"// Generated by {GENERATOR} from Python {sys.version.split()[0]} "
        f"(Unicode {unicodedata.unidata_version}).",
        "// Do not edit: run the generator again.",
        "",
        "#[rustfmt::skip]",
        "pub(crate) const PRINTABLE: &[(u32, u32)] = &[",
        *(f"    (0x{start:04X}, 0x{end:04X})," for start, end in ranges),
        "];",
        "",
    ]
    TABLE.write_text("\n".join(lines), encoding="utf-8")


def write_schema(directory: Path) -> None:
    with tempfile.TemporaryDirectory() as tmp:
        connection = store._connect(Path(tmp) / "hecate.db")
        rows = connection.execute("SELECT type, name, tbl_name, sql FROM sqlite_master ORDER BY rowid").fetchall()
        synchronous = connection.execute("PRAGMA synchronous").fetchone()[0]
        secure_delete = connection.execute("PRAGMA secure_delete").fetchone()[0]
        connection.close()
    lines = [
        f"-- Generated by {GENERATOR} from {SOURCE} (Python {sys.version.split()[0]}). Do not edit.",
        f"-- pragma synchronous = {synchronous}",
        f"-- pragma secure_delete = {secure_delete}",
        "",
    ]
    for kind, name, table, sql in rows:
        lines.append(f"-- object: {kind} {name} {table}")
        lines.append("-- no sql" if sql is None else sql + ";")
        lines.append("")
    (directory / "schema.sql").write_text("\n".join(lines), encoding="utf-8")


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: generate.py <vectors directory>")
    directory = Path(sys.argv[1])
    directory.mkdir(parents=True, exist_ok=True)
    write(directory, "kdf.json", kdf_cases())
    write(directory, "root.json", root_cases())
    write(directory, "aad.json", aad_cases())
    write(directory, "envelope.json", envelope_cases())
    write(directory, "fingerprint.json", fingerprint_cases())
    write(directory, "events.json", events_cases())
    write(directory, "instants.json", instants_cases())
    write(directory, "values.json", value_cases())
    ranges = printable_ranges()
    write(directory, "printable.json", ranges)
    write_table(ranges)
    write_schema(directory)


if __name__ == "__main__":
    main()
