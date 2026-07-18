# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.1] - 2026-07-17

### Added
- Dict-shaped TECHNIQUES `data` parsing (sorted numeric keys) alongside array form.
- Adversarial tests for dict/list fixture parity and Markdown pipe escaping in all columns.
- Validation for `port == 0` with `spawn_local` and zero `poll_interval`.
- Integration coverage for status `message` propagation and HTTP status checks on start/stop/kill.

### Fixed
- `format_findings` Markdown now escapes `|` in parameter, vulnerability_type, and payload.
- `BinaryNotFound` display mentions `sqlmapapi` only (spawn ENOENT maps to this variant).
- `wait_for_completion` includes API `message` when status JSON has `success: false`.
- `start`, `stop`, and `kill` reject non-success HTTP responses before JSON parse.

### Changed
- MSRV bumped to Rust 1.71.
- README tone softened (em dashes removed); `SqlmapFinding` Display uses ASCII hyphen instead of em dash.

## [0.3.0] - 2026-07-17

### Added
- Santh-standard test suite: `tests/adversarial.rs`, `tests/gap.rs`,
  `tests/property.rs`, `tests/contract.rs`, and `tests/integration.rs`.
- Proptest coverage for finding JSON roundtrip, CSV shape, and type-0 chunk
  invariants.
- Nested TECHNIQUES fixture with two techniques in `tests/test_types.rs`.

### Changed
- Mature release bump after deep review: findings parser hardening and API
  `success: false` rejection from 0.2.2 carried forward.

## [0.2.2] - 2026-07-17

### Fixed
- `fetch_data`, `fetch_log`, and `list_options` now return `SqlmapError::ApiError`
  when the API JSON body has `success: false` (with `message` when present).
- `SqlmapOptions.tech` serializes as sqlmap's `technique` field (BEUSTQ, including `Q`).
- `DataResponse::findings()` skips incomplete type-1 objects (parameter-only or
  type without payload); legacy flat objects with `type` + `payload` still parse.

### Changed
- Crate repository URL: `https://github.com/santhreal/sqlmap-rs`.
- Added `[workspace]` so the crate builds standalone outside the Santh monorepo.
- Published package excludes `setup.sh`, `environment.yml`, and `.github/`.
- README and crate docs: localhost-only daemon, best-effort RAII cleanup; removed
  overclaims (`panic-free`, remote daemon).
- `examples/full_scan` requires a target URL CLI argument (no default vulnweb URL).

## [0.2.1] - 2026-07-17

### Fixed
- Parse real sqlmapapi TECHNIQUES payloads: expand nested `data[]`
  (`title`/`technique` + `payload`) instead of only flat `type` fields.
- `wait_for_completion` no longer treats `"not running"` as success
  (that status means the scan has not finished / not attached yet).
- `PortConflict` message no longer advertises unsupported port `0`.

### Changed
- Crate description no longer claims streaming output.
- Categories: `api-bindings` + `web-programming`.
- Re-export `SqlmapTask` and `format_findings` from the crate root.
- Narrow `tokio` features; stop excluding `setup.sh` / `environment.yml`
  from the published package so README install paths resolve.

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
