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

impl DataResponse {
    /// Extract structured findings from the raw data chunks.
    ///
    /// Type 1 chunks contain vulnerability data. This parses them into
    /// `SqlmapFinding` structs with parameter, type, payload, and details.
    pub fn findings(&self) -> Vec<SqlmapFinding> {
        let Some(ref chunks) = self.data else { return vec![] };
        let mut findings = Vec::new();

        for chunk in chunks {
            // Type 1 = vulnerability findings
            if chunk.r#type == 1 {
                if let Some(arr) = chunk.value.as_array() {
                    for item in arr {
                        if let Some(obj) = item.as_object() {
                            let parameter = obj.get("parameter")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown")
                                .to_string();
                            let vulnerability_type = obj.get("type")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown")
                                .to_string();
                            let payload = obj.get("payload")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            findings.push(SqlmapFinding {
                                parameter,
                                vulnerability_type,
                                payload,
                                details: item.clone(),
                            });
                        }
                    }
                }
            }
        }

        findings
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_data_response_gives_no_findings() {
        let resp = DataResponse { success: true, data: None, error: None };
        assert!(resp.findings().is_empty());
    }

    #[test]
    fn type_0_chunks_ignored() {
        let resp = DataResponse {
            success: true,
            data: Some(vec![SqlmapDataChunk { r#type: 0, value: serde_json::json!("log message") }]),
            error: None,
        };
        assert!(resp.findings().is_empty());
    }

    #[test]
    fn type_1_chunk_parsed_as_finding() {
        let resp = DataResponse {
            success: true,
            data: Some(vec![SqlmapDataChunk {
                r#type: 1,
                value: serde_json::json!([{
                    "parameter": "id",
                    "type": "boolean-based blind",
                    "payload": "id=1 AND 1=1"
                }]),
            }]),
            error: None,
        };
        let findings = resp.findings();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].parameter, "id");
        assert_eq!(findings[0].vulnerability_type, "boolean-based blind");
    }

    #[test]
    fn options_serialization() {
        let opts = SqlmapOptions {
            url: Some("http://test.com?id=1".into()),
            level: Some(3),
            risk: Some(2),
            batch: Some(true),
            ..Default::default()
        };
        let json = serde_json::to_string(&opts).unwrap();
        assert!(json.contains("http://test.com"));
        assert!(json.contains("\"level\":3"));
        // None fields should be skipped
        assert!(!json.contains("dbms"));
    }
}
