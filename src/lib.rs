//! # Sqlmap-RS
//! 
//! An asynchronous, strictly-typed Rust wrapper for the `sqlmapapi` REST Server.
//! 
//! Provides a panic-free API for orchestrating parallel SQL injection scans using the world's most 
//! powerful detection engine without scraping messy command-line outputs.
//! 
//! ## Architecture
//! Instead of executing `sqlmap ...` repeatedly and trying to parse standard output, this library leverages Sqlmap's Native REST API (`sqlmapapi`).
//! It spawns the python daemon under a Tokio background thread, and uses high-performance HTTP polling to track Injection execution lifecycles.

#![warn(missing_docs)]

/// REST API Client Orchestration
pub mod client;
/// Typed Error Variants
pub mod error;
/// Deserialization payloads matching Sqlmap's JSON structures
pub mod types;

pub use client::SqlmapEngine;
pub use error::SqlmapError;
pub use types::{DataResponse, SqlmapDataChunk, SqlmapFinding, SqlmapOptions};
