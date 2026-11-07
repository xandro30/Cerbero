from pathlib import Path

import pytest

from hecate.application.sealing.settings import KDF_MODERATE, KDF_SENSITIVE
from hecate.infrastructure.config import ConfigError, HecateConfig


def test_data_dir_is_required():
    with pytest.raises(ConfigError):
        HecateConfig.from_env({})


def test_defaults_are_the_sensitive_profile_and_a_socket_in_the_data_dir():
    config = HecateConfig.from_env({"HECATE_DATA_DIR": "/srv/hecate"})
    assert config.kdf == KDF_SENSITIVE
    assert config.socket_path == Path("/srv/hecate/control.sock")
    assert config.db_path == Path("/srv/hecate/hecate.db")


def test_moderate_profile_and_custom_socket_path_are_accepted():
    config = HecateConfig.from_env(
        {"HECATE_DATA_DIR": "/d", "HECATE_KDF_PROFILE": "Moderate", "HECATE_SOCKET_PATH": "/run/h.sock"}
    )
    assert config.kdf == KDF_MODERATE and config.socket_path == Path("/run/h.sock")


@pytest.mark.parametrize("profile", ["minimal", "", "fast"])
def test_unknown_profiles_are_rejected_so_no_weak_profile_exists_in_production(profile):
    with pytest.raises(ConfigError):
        HecateConfig.from_env({"HECATE_DATA_DIR": "/d", "HECATE_KDF_PROFILE": profile})


def test_prepare_creates_a_private_directory(tmp_path):
    config = HecateConfig.for_dir(tmp_path / "data")
    config.prepare()
    assert (config.data_dir.stat().st_mode & 0o777) == 0o700
