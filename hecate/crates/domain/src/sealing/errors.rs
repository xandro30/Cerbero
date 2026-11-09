use crate::sealing::state::SealState;

/// The errors Hécate reports to its callers. Messages never carry secrets.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HecateError {
    #[error("{0}")]
    InvalidInput(String),
    #[error("{0}")]
    AuthenticationFailed(String),
    #[error("too many failed attempts; retry in {retry_after_s}s")]
    RateLimited { retry_after_s: u64 },
    #[error("{0}")]
    NotFound(String),
    /// The operation is not allowed now: Hécate is not in the right seal state, or the secret
    /// is not in a state that allows it (`current_version` marks a stale write).
    #[error("{message}")]
    StateConflict {
        state: Option<SealState>,
        message: String,
        current_version: Option<u64>,
    },
}

impl HecateError {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::InvalidInput(message.into())
    }

    pub fn auth(message: impl Into<String>) -> Self {
        Self::AuthenticationFailed(message.into())
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }

    pub fn state_conflict(
        state: Option<SealState>,
        message: Option<String>,
        current_version: Option<u64>,
    ) -> Self {
        let message = message.unwrap_or_else(|| match state {
            Some(state) => format!("operation not allowed while {state}"),
            None => "operation not allowed".to_string(),
        });
        Self::StateConflict {
            state,
            message,
            current_version,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput(_) => "validation",
            Self::AuthenticationFailed(_) => "auth",
            Self::RateLimited { .. } => "rate_limited",
            Self::NotFound(_) => "not_found",
            Self::StateConflict { .. } => "conflict",
        }
    }

    pub fn retryable(&self) -> bool {
        matches!(self, Self::RateLimited { .. })
    }
}
