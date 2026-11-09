//! Process hardening at start-up (research R12): no core dumps and, on Linux, memory locked so the
//! root cannot reach swap.

use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Linux,
    Other,
}

impl Os {
    pub fn current() -> Self {
        if cfg!(target_os = "linux") {
            Self::Linux
        } else {
            Self::Other
        }
    }
}

pub trait ProcessControls {
    fn disable_core_dumps(&mut self) -> io::Result<()>;
    fn lock_memory(&mut self) -> io::Result<()>;
}

/// The real controls, through the safe functions of nix.
pub struct SystemControls;

impl ProcessControls for SystemControls {
    fn disable_core_dumps(&mut self) -> io::Result<()> {
        nix::sys::resource::setrlimit(nix::sys::resource::Resource::RLIMIT_CORE, 0, 0)
            .map_err(io::Error::from)
    }

    #[cfg(target_os = "linux")]
    fn lock_memory(&mut self) -> io::Result<()> {
        use nix::sys::mman::{MlockAllFlags, mlockall};
        mlockall(MlockAllFlags::MCL_CURRENT | MlockAllFlags::MCL_FUTURE).map_err(io::Error::from)
    }

    #[cfg(not(target_os = "linux"))]
    fn lock_memory(&mut self) -> io::Result<()> {
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }
}

pub fn harden_process() -> Vec<String> {
    harden_process_with(Os::current(), &mut SystemControls)
}

/// Applies the hardening and returns the warnings it logged.
pub fn harden_process_with(os: Os, controls: &mut dyn ProcessControls) -> Vec<String> {
    let mut warnings = Vec::new();
    if controls.disable_core_dumps().is_err() {
        warnings.push("core dumps could not be disabled".to_string());
    }
    match os {
        Os::Other => {
            warnings.push(
                "memory locking is only available on Linux; the root may reach swap".to_string(),
            );
        }
        Os::Linux => {
            if controls.lock_memory().is_err() {
                warnings.push(
                    "mlockall failed (missing permission?); the root may reach swap".to_string(),
                );
            }
        }
    }
    for warning in &warnings {
        tracing::warn!(target: "hecate.process", "{warning}");
    }
    warnings
}
