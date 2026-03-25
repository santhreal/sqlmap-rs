//! Core type definitions for SQLMap REST API (`sqlmapapi`) payloads.

use serde::{Deserialize, Serialize};

/// Response when creating a new task.
#[derive(Debug, Clone, Deserialize)]
pub struct NewTaskResponse {
    /// True if task creation succeeded
    pub success: bool,
    /// The unique execution ID assigned to this task
    pub taskid: Option<String>,
    /// Error or info message
    pub message: Option<String>,
}

/// Generic success response.
#[derive(Debug, Clone, Deserialize)]
pub struct BasicResponse {
    /// True if operation succeeded
    pub success: bool,
    /// Detailed message
    pub message: Option<String>,
}

/// Response containing current execution status.
#[derive(Debug, Clone, Deserialize)]
pub struct StatusResponse {
    /// True if request succeeded
    pub success: bool,
    /// Current engine status ("running", "terminated", etc.)
    pub status: Option<String>,
    /// Underlying process exit code
    pub returncode: Option<i32>,
}

/// A chunk of extracted data reported by SQLMap engine.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SqlmapDataChunk {
    /// 0 (log), 1 (vulnerabilities), etc.
    pub r#type: i32,
    /// The actual JSON payload chunk
    pub value: serde_json::Value,
}

/// Final payload block returning all gathered data for a task.
#[derive(Debug, Clone, Deserialize)]
pub struct DataResponse {
    /// True if fetch succeeded
    pub success: bool,
    /// The aggregated data chunks representing SQL injected results
    pub data: Option<Vec<SqlmapDataChunk>>,
    /// Unused array of structured errors
    pub error: Option<Vec<String>>,
}

/// A parsed finding representing a confirmed injection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlmapFinding {
    /// Original parameter attacked
    pub parameter: String,
    /// Classification of injection
    pub vulnerability_type: String,
    /// Raw payload executed
    pub payload: String,
    /// Arbitrary engine output block
    pub details: serde_json::Value,
}

/// Configuration payload mapped directly to SQLMap CLI arguments.
#[derive(Debug, Clone, Serialize, Default)]
pub struct SqlmapOptions {
    /// The target URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    /// Target directly on specific parameter, e.g. "id"
    #[serde(rename = "testParameter", skip_serializing_if = "Option::is_none")]
    pub test_parameter: Option<String>,

    /// Specific database management system. e.g. "MySQL"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dbms: Option<String>,

    /// HTTP Cookie header value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cookie: Option<String>,

    /// Specific payload technqiues to test (B = Boolean blind, T = Time blind, E = Error, U = UNION query, S = Stacked queries).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tech: Option<String>,

    /// Output extraction verbosity level (1-6). 
    /// For the API, we usually keep this low since data extraction comes via REST endpoint `data`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verbose: Option<i32>,

    /// Number of concurrent workers (default is 1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threads: Option<i32>,
    
    /// Do not ask for user input. Default is true for bot orchestration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch: Option<bool>,

    /// HTTP headers to manually pass into the request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<String>,

    /// Payload risk (1-3)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk: Option<i32>,

    /// Level of tests to perform (1-5, default 1)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<i32>,
    
    /// Use a proxy?
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy: Option<String>,
}
