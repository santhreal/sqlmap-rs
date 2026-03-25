//! Error definitions for the sqlmapapi binding.

use thiserror::Error;

/// Core error type for sqlmap api interactions.
#[derive(Debug, Error)]
pub enum SqlmapError {
    /// Failed to launch or track the `sqlmapapi.py` subprocess.
    #[error("API process error: {0}")]
    ProcessError(#[from] std::io::Error),

    /// Missing or invalid dependencies (Python or sqlmapapi not found).
    #[error("Binary not found: {0}. Ensure python3 and sqlmap are in PATH.")]
    BinaryNotFound(String),

    /// HTTP Request error when polling the REST API.
    #[error("HTTP Request failed: {0}")]
    RequestError(#[from] reqwest::Error),

    /// The API responded with an error message (success: false).
    #[error("Sqlmap API returned error: {0}")]
    ApiError(String),

    /// API did not respond with expected semantic JSON format.
    #[error("Malformed JSON response structure.")]
    MalformedResponse,
    
    /// Polling timeout while waiting for task to complete.
    #[error("Task execution timed out after {0} seconds.")]
    Timeout(u64),

    /// The provided task ID was unrecognized by the server.
    #[error("Invalid Task ID: {0}")]
    InvalidTask(String),
}
