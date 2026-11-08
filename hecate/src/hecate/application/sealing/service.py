import secrets
from collections.abc import Iterator
from contextlib import contextmanager
from dataclasses import dataclass
from datetime import datetime
from typing import NoReturn

from hecate.application.ports import AccessLog, Clock, CryptoError, CryptoProvider, SealRepository
from hecate.application.sealing.root_holder import RootHolder
from hecate.application.sealing.settings import SealingSettings
from hecate.domain.sealing.access_log import AccessLogEntry, Method, Operation, Result
from hecate.domain.sealing.attempts import AttemptPolicy
from hecate.domain.sealing.errors import (
    AuthenticationFailed,
    HecateError,
    InvalidInput,
    RateLimited,
    StateConflict,
)
from hecate.domain.sealing.passphrase import Passphrase
from hecate.domain.sealing.protection import (
    AEAD_NAME,
    KDF_NAME,
    SHARE_COUNT,
    SHARE_THRESHOLD,
    PassphraseProtection,
    RecoveryProtection,
    SealConfig,
    associated_data,
)
from hecate.domain.sealing.share import RecoveryShare
from hecate.domain.sealing.state import SealState, UnsealMethod

ROOT_SIZE = 32
SALT_SIZE = 16
SET_ID_SIZE = 4

INIT_WARNING = (
    "If you lose both the passphrase and two of the three recovery shares, "
    "the data in Hécate cannot be recovered."
)

_RESULT_FOR_ERROR: dict[type[HecateError], Result] = {
    InvalidInput: Result.INVALID_INPUT,
    StateConflict: Result.REJECTED_CONFLICT,
    RateLimited: Result.REJECTED_RATE_LIMITED,
    AuthenticationFailed: Result.FAILURE,
}


@dataclass
class _PendingInit:
    init_id: str
    root: RootHolder
    config: SealConfig
    shares: dict[int, RecoveryShare]
    expires_at: datetime

    def discard(self) -> None:
        self.root.release()


@dataclass
class _PendingRekey:
    rekey_id: str
    recovery_key: RootHolder
    share_set_id: bytes
    shares: dict[int, RecoveryShare]
    expires_at: datetime

    def discard(self) -> None:
        self.recovery_key.release()


class SealingService:
    """Seal lifecycle of Hécate: the root lives only in memory while unsealed."""

    def __init__(
        self,
        *,
        crypto: CryptoProvider,
        repo: SealRepository,
        access_log: AccessLog,
        clock: Clock,
        settings: SealingSettings,
    ) -> None:
        self._crypto = crypto
        self._repo = repo
        self._access_log = access_log
        self._clock = clock
        self._settings = settings
        # A new process always starts without the root (FR-006).
        self._root: RootHolder | None = None
        self._unsealed_via: UnsealMethod | None = None
        self._recommend_passphrase_change = False
        self._pending_init: _PendingInit | None = None
        self._pending_rekey: _PendingRekey | None = None

    # -- shared helpers -----------------------------------------------------------------------

    def _now(self) -> datetime:
        return self._clock.now()

    def _state(self) -> SealState:
        if self._root is not None:
            return SealState.UNSEALED
        if self._repo.load_config() is None:
            return SealState.UNINITIALIZED
        return SealState.SEALED

    def _log(self, operation: Operation, method: Method, result: Result, detail: str) -> None:
        self._access_log.append(AccessLogEntry(self._now(), operation, method, result, detail))

    @contextmanager
    def _logged(self, operation: Operation, method: Method, detail: str) -> Iterator[None]:
        """Leave a trace in the access log when the operation is refused (never the secrets)."""
        try:
            yield
        except HecateError as exc:
            self._log(operation, method, _RESULT_FOR_ERROR.get(type(exc), Result.FAILURE), detail)
            raise

    def _require_state(self, expected: SealState) -> None:
        state = self._state()
        if state is not expected:
            raise StateConflict(state)

    @staticmethod
    def _parse_shares(texts: object) -> list[RecoveryShare]:
        if not isinstance(texts, list) or not all(isinstance(t, str) for t in texts):
            raise InvalidInput("shares must be a list of texts")
        shares = [RecoveryShare.parse(t) for t in texts]
        if len({s.index for s in shares}) != len(shares):
            raise InvalidInput("the same share was given twice")
        if len(shares) < SHARE_THRESHOLD:
            raise InvalidInput(f"{SHARE_THRESHOLD} shares are needed")
        return shares

    def _passphrase_protection(self, root: bytes, passphrase: str, version: int) -> PassphraseProtection:
        salt = self._crypto.random_bytes(SALT_SIZE)
        limits = self._settings.kdf
        key = self._crypto.derive_key(passphrase, salt, limits.opslimit, limits.memlimit)
        nonce, ciphertext = self._crypto.aead_encrypt(key, root, associated_data("passphrase", version))
        return PassphraseProtection(
            version=version, kdf=KDF_NAME, kdf_opslimit=limits.opslimit, kdf_memlimit=limits.memlimit,
            salt=salt, aead=AEAD_NAME, nonce=nonce, ciphertext=ciphertext,
        )

    def _recovery_protection(
        self, root: bytes, recovery_key: bytes, share_set_id: bytes, version: int
    ) -> RecoveryProtection:
        nonce, ciphertext = self._crypto.aead_encrypt(
            recovery_key, root, associated_data("recovery", version)
        )
        return RecoveryProtection(
            version=version, share_set_id=share_set_id, threshold=SHARE_THRESHOLD,
            share_count=SHARE_COUNT, aead=AEAD_NAME, nonce=nonce, ciphertext=ciphertext,
        )

    def _new_shares(self, recovery_key: bytes, share_set_id: bytes) -> dict[int, RecoveryShare]:
        return {
            index: RecoveryShare(index, share_set_id, fragments)
            for index, fragments in self._crypto.shamir_split(recovery_key)
        }

    # -- status -------------------------------------------------------------------------------

    def status(self) -> dict[str, object]:
        config = self._repo.load_config()
        attempts = self._repo.load_attempts()
        return {
            "state": self._state().value,
            "initialized_at": config.initialized_at.isoformat() if config else None,
            "unsealed_via": self._unsealed_via.value if self._unsealed_via else None,
            "recommend_passphrase_change": self._recommend_passphrase_change,
            "consecutive_failures": attempts.consecutive_failures,
            "retry_after_s": attempts.retry_after(self._now()),
        }

    def shutdown(self) -> None:
        """Release the root when the process stops."""
        if self._root is not None:
            self._root.release()
            self._root = None
        self._drop_pending_init()
        self._drop_pending_rekey()

    def record_invalid_request(self, operation: Operation, method: Method, detail: str) -> None:
        """Leave a row for a recognized operation whose arguments were malformed."""
        self._log(operation, method, Result.INVALID_INPUT, detail)

    # -- initialization (FR-002 .. FR-005, FR-018, FR-019) -------------------------------------

    def _drop_pending_init(self) -> None:
        if self._pending_init is not None:
            self._pending_init.discard()
            self._pending_init = None

    def init_begin(self, passphrase: str) -> dict[str, object]:
        with self._logged(Operation.INIT, Method.LOCAL, "begin"):
            self._require_state(SealState.UNINITIALIZED)
            secret = Passphrase(passphrase)
            self._drop_pending_init()

            root = self._crypto.random_bytes(ROOT_SIZE)
            recovery_key = self._crypto.random_bytes(ROOT_SIZE)
            share_set_id = self._crypto.random_bytes(SET_ID_SIZE)
            now = self._now()
            config = SealConfig(
                config_version=1,
                initialized_at=now,
                updated_at=now,
                passphrase=self._passphrase_protection(root, secret.value, 1),
                recovery=self._recovery_protection(root, recovery_key, share_set_id, 1),
            )
            shares = self._new_shares(recovery_key, share_set_id)
            pending = _PendingInit(
                init_id=secrets.token_urlsafe(16),
                root=RootHolder(root),
                config=config,
                shares=shares,
                expires_at=now + self._settings.pending_ttl,
            )
            self._pending_init = pending
        self._log(Operation.INIT, Method.LOCAL, Result.SUCCESS, "begin")
        return {
            "init_id": pending.init_id,
            "shares": [shares[i].encode() for i in sorted(shares)],
            "expires_at": pending.expires_at.isoformat(),
            "warning": INIT_WARNING,
        }

    def init_confirm(self, init_id: str, share_texts: list[str]) -> dict[str, object]:
        with self._logged(Operation.INIT, Method.LOCAL, "confirm"):
            self._require_state(SealState.UNINITIALIZED)
            pending = self._pending_init
            if pending is None or not secrets.compare_digest(pending.init_id, str(init_id)):
                raise InvalidInput("unknown initialization: start it again")
            if self._now() > pending.expires_at:
                self._drop_pending_init()
                raise InvalidInput("the initialization expired: start it again")
            given = self._parse_shares(share_texts)
            if any(pending.shares.get(s.index) != s for s in given):
                raise InvalidInput("the shares do not match the ones that were shown")
            with self._repo.transaction():
                self._repo.save_config(pending.config)
            self._drop_pending_init()
        self._log(Operation.INIT, Method.LOCAL, Result.SUCCESS, "confirm")
        return {"state": SealState.SEALED.value}

    def init_abort(self, init_id: str) -> dict[str, object]:
        with self._logged(Operation.INIT, Method.LOCAL, "abort"):
            self._require_state(SealState.UNINITIALIZED)
            pending = self._pending_init
            if pending is not None and secrets.compare_digest(pending.init_id, str(init_id)):
                self._drop_pending_init()
        self._log(Operation.INIT, Method.LOCAL, Result.SUCCESS, "abort")
        return {}

    # -- unsealing (FR-006 .. FR-008, FR-016) --------------------------------------------------

    def _check_wait(self) -> None:
        wait = self._repo.load_attempts().retry_after(self._now())
        if wait > 0:
            raise RateLimited(wait)

    def _fail_attempt(self, message: str) -> NoReturn:
        attempts = self._repo.load_attempts().register_failure(self._now())
        with self._repo.transaction():
            self._repo.save_attempts(attempts)
        raise AuthenticationFailed(message)

    def _reset_attempts(self) -> None:
        if self._repo.load_attempts() != AttemptPolicy():
            with self._repo.transaction():
                self._repo.save_attempts(AttemptPolicy())

    def _open(self, root: bytes, via: UnsealMethod) -> dict[str, object]:
        self._root = RootHolder(root)
        self._unsealed_via = via
        self._recommend_passphrase_change = via is UnsealMethod.RECOVERY
        return {
            "state": SealState.UNSEALED.value,
            "unsealed_via": via.value,
            "recommend_passphrase_change": self._recommend_passphrase_change,
        }

    def _root_from_shares(self, config: SealConfig, shares: list[RecoveryShare]) -> bytes:
        recovery = config.recovery
        if any(s.share_set_id != recovery.share_set_id for s in shares):
            self._fail_attempt("a share belongs to another set (an old or foreign one)")
        try:
            recovery_key = self._crypto.shamir_combine([(s.index, s.fragments) for s in shares])
            return self._crypto.aead_decrypt(
                recovery_key, recovery.nonce, recovery.ciphertext, recovery.aad()
            )
        except CryptoError:
            self._fail_attempt("the shares do not recover the root")

    def _root_from_passphrase(self, config: SealConfig, passphrase: str) -> bytes:
        protection = config.passphrase
        key = self._crypto.derive_key(
            passphrase, protection.salt, protection.kdf_opslimit, protection.kdf_memlimit
        )
        try:
            return self._crypto.aead_decrypt(key, protection.nonce, protection.ciphertext, protection.aad())
        except CryptoError:
            self._fail_attempt("incorrect passphrase")

    def unseal_with_passphrase(self, passphrase: str) -> dict[str, object]:
        with self._logged(Operation.UNSEAL, Method.PASSPHRASE, "passphrase"):
            self._require_state(SealState.SEALED)
            if not isinstance(passphrase, str) or not passphrase:
                raise InvalidInput("the passphrase must be a non-empty text")
            self._check_wait()
            root = self._root_from_passphrase(self._repo.load_config(), passphrase)
            self._reset_attempts()
        self._log(Operation.UNSEAL, Method.PASSPHRASE, Result.SUCCESS, "passphrase")
        return self._open(root, UnsealMethod.PASSPHRASE)

    # -- sealing (FR-010) ------------------------------------------------------------------------

    def seal(self) -> dict[str, object]:
        with self._logged(Operation.SEAL, Method.LOCAL, "seal"):
            self._require_state(SealState.UNSEALED)
            assert self._root is not None
            self._root.release()
            self._root = None
            self._unsealed_via = None
            self._recommend_passphrase_change = False
            self._drop_pending_rekey()
        self._log(Operation.SEAL, Method.LOCAL, Result.SUCCESS, "seal")
        return {"state": SealState.SEALED.value}

    def unseal_with_shares(self, share_texts: list[str]) -> dict[str, object]:
        with self._logged(Operation.UNSEAL, Method.RECOVERY, "shares"):
            self._require_state(SealState.SEALED)
            shares = self._parse_shares(share_texts)  # format errors do not spend an attempt (FR-009)
            self._check_wait()
            root = self._root_from_shares(self._repo.load_config(), shares)
            self._reset_attempts()
        self._log(Operation.UNSEAL, Method.RECOVERY, Result.SUCCESS, "shares")
        return self._open(root, UnsealMethod.RECOVERY)

    # -- operations other components rely on (secrets: research R6) --------------------------------

    def root(self) -> bytes:
        """The root, only while unsealed; otherwise `conflict` with the seal state."""
        if self._root is None:
            raise StateConflict(self._state())
        return self._root.get()

    def verify_proof(self, proof: object) -> None:
        """Check a proof (current passphrase or two shares) with the shared attempt counter."""
        self._require_state(SealState.UNSEALED)
        self._root_from_proof(self._repo.load_config(), proof)
        self._reset_attempts()

    # -- changing the protection of the root (FR-011 .. FR-013) -----------------------------------

    @staticmethod
    def proof_method(proof: object) -> Method:
        if isinstance(proof, dict) and "shares" in proof and "passphrase" not in proof:
            return Method.RECOVERY
        if isinstance(proof, dict) and "passphrase" in proof and "shares" not in proof:
            return Method.PASSPHRASE
        return Method.LOCAL

    def _root_from_proof(self, config: SealConfig, proof: object) -> bytes:
        """Check a proof (current passphrase or two shares) before the protection is changed."""
        if not isinstance(proof, dict) or ("passphrase" in proof) == ("shares" in proof):
            raise InvalidInput("the proof must be either 'passphrase' or 'shares'")
        if "passphrase" in proof:
            passphrase = proof["passphrase"]
            if not isinstance(passphrase, str) or not passphrase:
                raise InvalidInput("the passphrase must be a non-empty text")
            self._check_wait()
            return self._root_from_passphrase(config, passphrase)
        shares = self._parse_shares(proof["shares"])
        self._check_wait()
        return self._root_from_shares(config, shares)

    def change_passphrase(self, proof: dict[str, object], new_passphrase: str) -> dict[str, object]:
        method = self.proof_method(proof)
        with self._logged(Operation.PASSPHRASE_CHANGE, method, "change"):
            self._require_state(SealState.UNSEALED)
            new_secret = Passphrase(new_passphrase)
            config = self._repo.load_config()
            root = self._root_from_proof(config, proof)
            version = config.config_version + 1
            updated = SealConfig(
                config_version=version,
                initialized_at=config.initialized_at,
                updated_at=self._now(),
                passphrase=self._passphrase_protection(root, new_secret.value, version),
                recovery=config.recovery,  # untouched: the shares keep working (FR-013)
            )
            with self._repo.transaction():
                self._repo.save_config(updated)
                self._repo.save_attempts(AttemptPolicy())
            self._recommend_passphrase_change = False
        self._log(Operation.PASSPHRASE_CHANGE, method, Result.SUCCESS, "change")
        return {"config_version": version}

    # -- giving out new recovery shares (FR-012, FR-013) -----------------------------------------

    def _drop_pending_rekey(self) -> None:
        if self._pending_rekey is not None:
            self._pending_rekey.discard()
            self._pending_rekey = None

    def rekey_begin(self, proof: dict[str, object]) -> dict[str, object]:
        method = self.proof_method(proof)
        with self._logged(Operation.SHARES_REKEY, method, "begin"):
            self._require_state(SealState.UNSEALED)
            self._root_from_proof(self._repo.load_config(), proof)
            self._reset_attempts()
            self._drop_pending_rekey()
            recovery_key = self._crypto.random_bytes(ROOT_SIZE)
            share_set_id = self._crypto.random_bytes(SET_ID_SIZE)
            shares = self._new_shares(recovery_key, share_set_id)
            pending = _PendingRekey(
                rekey_id=secrets.token_urlsafe(16),
                recovery_key=RootHolder(recovery_key),
                share_set_id=share_set_id,
                shares=shares,
                expires_at=self._now() + self._settings.pending_ttl,
            )
            self._pending_rekey = pending
        self._log(Operation.SHARES_REKEY, method, Result.SUCCESS, "begin")
        return {
            "rekey_id": pending.rekey_id,
            "shares": [shares[i].encode() for i in sorted(shares)],
            "expires_at": pending.expires_at.isoformat(),
        }

    def rekey_confirm(self, rekey_id: str, share_texts: list[str]) -> dict[str, object]:
        with self._logged(Operation.SHARES_REKEY, Method.LOCAL, "confirm"):
            self._require_state(SealState.UNSEALED)
            pending = self._pending_rekey
            if pending is None or not secrets.compare_digest(pending.rekey_id, str(rekey_id)):
                raise InvalidInput("unknown share set: start again")
            if self._now() > pending.expires_at:
                self._drop_pending_rekey()
                raise InvalidInput("the share set expired: start again")
            given = self._parse_shares(share_texts)
            if any(pending.shares.get(s.index) != s for s in given):
                raise InvalidInput("the shares do not match the ones that were shown")
            config = self._repo.load_config()
            version = config.config_version + 1
            updated = SealConfig(
                config_version=version,
                initialized_at=config.initialized_at,
                updated_at=self._now(),
                passphrase=config.passphrase,  # untouched (FR-013)
                recovery=self._recovery_protection(
                    self._root.get(), pending.recovery_key.get(), pending.share_set_id, version
                ),
            )
            with self._repo.transaction():
                self._repo.save_config(updated)
            self._drop_pending_rekey()
        self._log(Operation.SHARES_REKEY, Method.LOCAL, Result.SUCCESS, "confirm")
        return {"config_version": version}

    def rekey_abort(self, rekey_id: str) -> dict[str, object]:
        with self._logged(Operation.SHARES_REKEY, Method.LOCAL, "abort"):
            self._require_state(SealState.UNSEALED)
            pending = self._pending_rekey
            if pending is not None and secrets.compare_digest(pending.rekey_id, str(rekey_id)):
                self._drop_pending_rekey()
        self._log(Operation.SHARES_REKEY, Method.LOCAL, Result.SUCCESS, "abort")
        return {}
