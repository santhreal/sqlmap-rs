//! # Sqlmap-RS
//!
//! An asynchronous, strictly-typed Rust wrapper for the `sqlmapapi` REST Server.
//!
//! Communicates with a **localhost-only** `sqlmapapi` daemon (`127.0.0.1`) over
//! sqlmap's native REST API instead of parsing CLI output.
//!
//! ## Features
//!
//! - **Core API coverage**: start, stop, kill, log, data, option introspection.
//! - **Builder pattern**: Fluent `SqlmapOptions::builder()` with 40+ sqlmap options.
//! - **Multi-format output**: JSON, CSV, Markdown, and plain text.
//! - **RAII cleanup**: Best-effort task and daemon cleanup on drop (requires an active Tokio runtime for task deletion).
//! - **Port conflict detection**: Best-effort TCP probe before spawn; see README Security for TOCTOU limits.
//! - **Configurable polling**: Custom intervals and HTTP timeouts.
//!
//! ## Architecture
//!
//! The library spawns `sqlmapapi.py` on `127.0.0.1` and uses HTTP polling to
//! track scan lifecycles. When the engine drops, the daemon subprocess is killed
//! best-effort. When a task drops, it is deleted from the daemon if a Tokio
//! runtime is available.

#![warn(missing_docs)]

/// REST API Client Orchestration.
pub mod client;
/// Typed Error Variants.
pub mod error;
/// Deserialization payloads, options builder, and output formatting.
pub mod types;

pub use client::{SqlmapEngine, SqlmapTask};
pub use error::SqlmapError;
pub use types::{
    format_findings, DataResponse, LogEntry, LogResponse, OutputFormat, SqlmapDataChunk,
    SqlmapFinding, SqlmapOptions, SqlmapOptionsBuilder,
};

// Holds MSRV pin on idna_adapter (see Cargo.toml).
use idna_adapter as _;
