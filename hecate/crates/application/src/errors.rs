use hecate_domain::sealing::errors::HecateError;

use crate::ports::{ConcurrencyConflict, CryptoError};

/// What a use case can fail with: an error for the caller, a stale write the service turns into
/// a conflict, or an internal failure whose cause is only logged by its kind.
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error(transparent)]
    Hecate(#[from] HecateError),
    #[error(transparent)]
    Concurrency(#[from] ConcurrencyConflict),
    #[error("internal error ({kind})")]
    Internal { kind: &'static str },
}

impl ServiceError {
    pub fn internal(kind: &'static str) -> Self {
        Self::Internal { kind }
    }

    /// The kind of an internal failure, as the logs name it.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Hecate(_) => "HecateError",
            Self::Concurrency(_) => "ConcurrencyConflict",
            Self::Internal { kind } => kind,
        }
    }
}

impl From<CryptoError> for ServiceError {
    fn from(_: CryptoError) -> Self {
        Self::internal("CryptoError")
    }
}
