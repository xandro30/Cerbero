import os
from collections.abc import Mapping
from dataclasses import dataclass
from pathlib import Path

from hecate.application.sealing.settings import KDF_MODERATE, KDF_SENSITIVE, KdfLimits

_PROFILES = {"sensitive": KDF_SENSITIVE, "moderate": KDF_MODERATE}


class ConfigError(Exception):
    pass


@dataclass(frozen=True)
class HecateConfig:
    data_dir: Path
    socket_path: Path
    kdf: KdfLimits = KDF_SENSITIVE

    @classmethod
    def for_dir(cls, data_dir: Path, kdf: KdfLimits = KDF_SENSITIVE) -> "HecateConfig":
        return cls(data_dir=Path(data_dir), socket_path=Path(data_dir) / "control.sock", kdf=kdf)

    @classmethod
    def from_env(cls, environ: Mapping[str, str] | None = None) -> "HecateConfig":
        env = os.environ if environ is None else environ
        data_dir = env.get("HECATE_DATA_DIR")
        if not data_dir:
            raise ConfigError("HECATE_DATA_DIR is required")
        profile = env.get("HECATE_KDF_PROFILE", "sensitive").lower()
        if profile not in _PROFILES:
            raise ConfigError(f"HECATE_KDF_PROFILE must be one of {sorted(_PROFILES)}")
        socket_path = Path(env.get("HECATE_SOCKET_PATH") or Path(data_dir) / "control.sock")
        return cls(data_dir=Path(data_dir), socket_path=socket_path, kdf=_PROFILES[profile])

    @property
    def db_path(self) -> Path:
        return self.data_dir / "hecate.db"

    def prepare(self) -> None:
        """Create the data directory, private to the Hécate user."""
        self.data_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
        os.chmod(self.data_dir, 0o700)
