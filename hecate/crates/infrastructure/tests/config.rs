use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use hecate_application::sealing::settings::{KDF_MODERATE, KDF_SENSITIVE};
use hecate_infrastructure::config::HecateConfig;

fn from(
    pairs: &[(&str, &str)],
) -> Result<HecateConfig, hecate_infrastructure::config::ConfigError> {
    let vars: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    HecateConfig::from_vars(|name| vars.get(name).cloned())
}

// py: unit/test_config.py::test_data_dir_is_required
#[test]
fn data_dir_is_required() {
    let error = from(&[]).unwrap_err();
    assert_eq!(error.to_string(), "HECATE_DATA_DIR is required");
}

// py: unit/test_config.py::test_defaults_are_the_sensitive_profile_and_a_socket_in_the_data_dir
#[test]
fn defaults_are_the_sensitive_profile_and_a_socket_in_the_data_dir() {
    let config = from(&[("HECATE_DATA_DIR", "/srv/hecate")]).unwrap();
    assert_eq!(config.kdf, KDF_SENSITIVE);
    assert_eq!(
        config.socket_path,
        PathBuf::from("/srv/hecate/control.sock")
    );
    assert_eq!(config.db_path(), PathBuf::from("/srv/hecate/hecate.db"));
}

// py: unit/test_config.py::test_moderate_profile_and_custom_socket_path_are_accepted
#[test]
fn moderate_profile_and_custom_socket_path_are_accepted() {
    let config = from(&[
        ("HECATE_DATA_DIR", "/d"),
        ("HECATE_KDF_PROFILE", "Moderate"),
        ("HECATE_SOCKET_PATH", "/run/h.sock"),
    ])
    .unwrap();
    assert_eq!(config.kdf, KDF_MODERATE);
    assert_eq!(config.socket_path, PathBuf::from("/run/h.sock"));
}

// py: unit/test_config.py::test_unknown_profiles_are_rejected_so_no_weak_profile_exists_in_production
#[test]
fn unknown_profiles_are_rejected_so_no_weak_profile_exists_in_production() {
    for profile in ["minimal", "", "fast"] {
        let error =
            from(&[("HECATE_DATA_DIR", "/d"), ("HECATE_KDF_PROFILE", profile)]).unwrap_err();
        assert_eq!(
            error.to_string(),
            "HECATE_KDF_PROFILE must be one of ['moderate', 'sensitive']"
        );
    }
}

// py: unit/test_config.py::test_prepare_creates_a_private_directory
#[test]
fn prepare_creates_a_private_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let config = HecateConfig::for_dir(tmp.path().join("data"), KDF_SENSITIVE);
    config.prepare().unwrap();
    let mode = std::fs::metadata(&config.data_dir)
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o700);
}

#[test]
fn the_api_address_and_the_lifetimes_have_defaults() {
    let config = from(&[("HECATE_DATA_DIR", "/d")]).unwrap();
    assert_eq!(
        config.api_addr,
        "127.0.0.1:8443".parse::<std::net::SocketAddr>().unwrap()
    );
    assert_eq!(config.cert_lifetime_hours, 72);
    assert_eq!(config.enroll_token_minutes, 15);
}

#[test]
fn the_api_address_and_the_lifetimes_are_read_within_their_ranges() {
    for (addr, hours, minutes) in [
        ("0.0.0.0:9443", "1", "1"),
        ("[::1]:8443", "72", "60"),
        ("192.168.1.10:443", "24", "15"),
    ] {
        let config = from(&[
            ("HECATE_DATA_DIR", "/d"),
            ("HECATE_API_ADDR", addr),
            ("HECATE_CERT_LIFETIME_HOURS", hours),
            ("HECATE_ENROLL_TOKEN_MINUTES", minutes),
        ])
        .unwrap();
        assert_eq!(config.api_addr, addr.parse().unwrap());
        assert_eq!(config.cert_lifetime_hours, hours.parse::<u32>().unwrap());
        assert_eq!(config.enroll_token_minutes, minutes.parse::<u32>().unwrap());
    }
}

#[test]
fn a_malformed_or_out_of_range_value_names_its_variable() {
    let api = "HECATE_API_ADDR must be an IP address and a port, such as 127.0.0.1:8443";
    let hours = "HECATE_CERT_LIFETIME_HOURS must be a whole number of hours from 1 to 72";
    let minutes = "HECATE_ENROLL_TOKEN_MINUTES must be a whole number of minutes from 1 to 60";
    for (name, value, message) in [
        ("HECATE_API_ADDR", "localhost:8443", api),
        ("HECATE_API_ADDR", "127.0.0.1", api),
        ("HECATE_API_ADDR", "", api),
        ("HECATE_CERT_LIFETIME_HOURS", "0", hours),
        ("HECATE_CERT_LIFETIME_HOURS", "73", hours),
        ("HECATE_CERT_LIFETIME_HOURS", "1.5", hours),
        ("HECATE_CERT_LIFETIME_HOURS", "", hours),
        ("HECATE_ENROLL_TOKEN_MINUTES", "0", minutes),
        ("HECATE_ENROLL_TOKEN_MINUTES", "61", minutes),
        ("HECATE_ENROLL_TOKEN_MINUTES", "quince", minutes),
    ] {
        let error = from(&[("HECATE_DATA_DIR", "/d"), (name, value)]).unwrap_err();
        assert_eq!(error.to_string(), message, "{name}={value:?}");
    }
}
