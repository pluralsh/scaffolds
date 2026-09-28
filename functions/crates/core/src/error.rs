use std::fmt::Display;

use crate::Action;

/// Failures that prevent a function from producing a [`crate::Response`].
///
/// Guard failures are not errors: they are reported as [`crate::Outcome::Refused`] so the
/// caller can see why an action was not taken.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid request: {0}")]
    InvalidRequest(String),

    #[error("action {0:?} is not supported by this function")]
    UnsupportedAction(Action),

    #[error("cloud provider error: {0}")]
    Provider(String),
}

impl Error {
    pub fn invalid_request(msg: impl Display) -> Self {
        Self::InvalidRequest(msg.to_string())
    }

    pub fn provider(msg: impl Display) -> Self {
        Self::Provider(msg.to_string())
    }

    /// Stable, machine-readable error category reported to the caller as the error type.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::InvalidRequest(_) => "InvalidRequest",
            Self::UnsupportedAction(_) => "UnsupportedAction",
            Self::Provider(_) => "Provider",
        }
    }
}
