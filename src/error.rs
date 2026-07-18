//! Error definitions for the sqlmapapi binding.
//!
//! Every error variant carries actionable context so callers can
//! diagnose failures without raw subprocess inspection.

use thiserror::Error;

/// Core error type for sqlmap API interactions.
///
/// Covers the full lifecycle from daemon boot through scan completion.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SqlmapError {
    /// Failed to launch or track the `sqlmapapi.py` subprocess.
    #[error("API process error: {0}")]
    ProcessError(#[from] std::io::Error),

    /// The `sqlmapapi` binary could not be found.
    #[error("binary not found: {0} - ensure sqlmapapi is in PATH")]
    BinaryNotFound(String),

    /// HTTP request error when communicating with the REST API.
    #[error("HTTP request failed: {0}")]
    RequestError(#[from] reqwest::Error),

    /// The API responded with `success: false`.
    #[error("sqlmap API returned error: {0}")]
    ApiError(String),

    /// API responded with unexpected or malformed JSON structure.
    #[error("malformed JSON response structure")]
    MalformedResponse,

    /// Polling timeout while waiting for task completion.
    #[error("task execution timed out after {0} seconds")]
    Timeout(u64),

    /// The provided task ID was unrecognized by the server.
    #[error("invalid task ID: {0}")]
    InvalidTask(String),

    /// The daemon failed to bind to the requested port.
    #[error("port {port} is already in use - choose a free port")]
    PortConflict {
        /// The port that was requested.
        port: u16,
    },
}
