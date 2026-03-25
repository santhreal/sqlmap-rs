# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-03-25

### Added
- **Full API coverage**: `stop()`, `kill()`, `fetch_log()`, `list_options()` endpoints.
- **Builder pattern**: `SqlmapOptions::builder()` with fluent API covering 40+ sqlmap options.
- **Multi-format output**: `format_findings()` supporting JSON, CSV, Markdown, and plain text.
- **Port conflict detection**: Prevents silent connection to wrong daemons on startup.
- **Configurable polling**: Custom poll interval and HTTP timeout via `SqlmapEngine::with_config()`.
- **Conda environment**: `environment.yml` for one-command dependency setup.
- **Setup script**: `setup.sh` auto-detects/installs conda + sqlmap with API smoke test.
- **Full example**: `examples/full_scan.rs` demonstrating all capabilities.
- **CI workflow**: `ci.yml` with fmt, clippy, test, doc, and auto-publish pipeline.
- `#[non_exhaustive]` on all public types for forward compatibility.
- `Display` impl on `SqlmapFinding` for human-readable output.
- `SqlmapFinding::new()` constructor for external crate usage.
- `SqlmapEngine::is_available_at()` for custom binary paths.
- `SqlmapEngine::api_url()` accessor.
- `SqlmapTask::task_id()` accessor.
- `LogEntry` and `LogResponse` types for scan log retrieval.
- `OutputFormat` enum for result formatting.
- `PortConflict` error variant with port context.

### Fixed
- **`SqlmapTask::drop()` no longer panics** when no Tokio runtime is active. Uses `Handle::try_current()` instead of bare `tokio::spawn()`.
- **`is_available()` no longer gives false positives** from `python3 -c "import sqlmap"`. Only checks the actual `sqlmapapi` binary.
- **Empty task IDs are rejected** with a proper error instead of silently using empty strings that cause downstream 404s.
- Polling interval reduced from hardcoded 3s to configurable 1s default.

### Changed
- Bumped to 0.2.0 (breaking: `#[non_exhaustive]` on all public types).
- `_process` field renamed to `daemon_process` for clarity.
- Removed unused `async-trait` dependency.
- Added `keywords`, `categories`, `homepage`, `documentation` to `Cargo.toml`.

### Removed
- Redundant `python3 -c "import sqlmap"` fallback in `is_available()`.

## [0.1.1] - 2026-03-24

### Added
- Initial release on crates.io.
- REST API client for sqlmapapi daemon.
- Task creation, configuration, start, poll, and data retrieval.
- RAII task cleanup on drop.
- Basic type definitions for sqlmap responses.
