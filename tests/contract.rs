//! External contract tests: README, Cargo metadata, and documented API behavior.
use sqlmap_rs::types::{DataResponse, LogResponse};
use sqlmap_rs::{SqlmapError, SqlmapOptions};
use std::fs;

fn cargo_toml_version() -> String {
    let manifest = fs::read_to_string("Cargo.toml").expect("read Cargo.toml");
    for line in manifest.lines() {
        if let Some(rest) = line.strip_prefix("version = ") {
            return rest.trim_matches('"').to_string();
        }
    }
    panic!("version not found in Cargo.toml");
}

#[test]
fn contract_rust_version_is_1_85() {
    let manifest = fs::read_to_string("Cargo.toml").expect("read Cargo.toml");
    assert!(
        manifest.contains("rust-version = \"1.85\""),
        "Cargo.toml rust-version must be 1.85"
    );
}

#[test]
fn contract_readme_setup_ab_github_only() {
    let readme = fs::read_to_string("README.md").expect("read README");
    assert!(
        readme.contains("GitHub repo only"),
        "README must state setup.sh/environment.yml are GitHub-only"
    );
    assert!(
        readme.contains("environment.yml"),
        "README must still reference environment.yml for GitHub users"
    );
    assert!(
        readme.contains("setup.sh"),
        "README must still reference setup.sh for GitHub users"
    );
}

#[test]
fn contract_readme_version_matches_cargo_toml() {
    let version = cargo_toml_version();
    assert_eq!(version, "0.3.3", "Cargo.toml version pin");
    let readme = fs::read_to_string("README.md").expect("read README");
    assert!(
        readme.contains(&format!("sqlmap-rs = \"{version}\"")),
        "README install pin must match Cargo.toml version {version}"
    );
}

#[test]
fn contract_repository_is_santhreal_sqlmap_rs() {
    let manifest = fs::read_to_string("Cargo.toml").expect("read Cargo.toml");
    assert!(
        manifest.contains("repository = \"https://github.com/santhreal/sqlmap-rs\""),
        "Cargo.toml repository must point at santhreal/sqlmap-rs"
    );
}

#[test]
fn contract_tech_serializes_as_technique_not_tech() {
    let opts = SqlmapOptions::builder().tech("BEUSTQ").build();
    let json = serde_json::to_string(&opts).expect("serialize");
    assert!(
        json.contains("\"technique\":\"BEUSTQ\""),
        "tech field must serialize as technique key"
    );
    assert!(
        !json.contains("\"tech\""),
        "Rust field name tech must not appear in JSON"
    );
}

#[test]
fn contract_binary_not_found_display_mentions_sqlmapapi_only() {
    let err = SqlmapError::BinaryNotFound("sqlmapapi".into());
    let display = format!("{err}");
    assert!(
        display.contains("sqlmapapi"),
        "BinaryNotFound must mention sqlmapapi: {display}"
    );
    assert!(
        !display.contains("python3"),
        "BinaryNotFound must not mention python3: {display}"
    );
}

#[test]
fn contract_data_response_success_false_deserializes() {
    let raw = r#"{"success": false, "message": "task not found", "data": null}"#;
    let resp: DataResponse = serde_json::from_str(raw).expect("deserialize");
    assert!(!resp.success);
    assert_eq!(resp.message.as_deref(), Some("task not found"));
}

#[test]
fn contract_log_response_success_false_deserializes() {
    let raw = r#"{"success": false, "message": "log unavailable", "log": null}"#;
    let resp: LogResponse = serde_json::from_str(raw).expect("deserialize");
    assert!(!resp.success);
    assert_eq!(resp.message.as_deref(), Some("log unavailable"));
}

#[test]
fn contract_malformed_response_display_is_stable() {
    let err = SqlmapError::MalformedResponse;
    assert_eq!(format!("{err}"), "malformed JSON response structure");
}

#[test]
fn contract_invalid_task_display_includes_task_id() {
    let err = SqlmapError::InvalidTask("".into());
    let display = format!("{err}");
    assert!(
        display.contains("invalid task ID"),
        "InvalidTask display must name the variant: {display}"
    );
}

#[test]
fn contract_categories_include_api_bindings() {
    let manifest = fs::read_to_string("Cargo.toml").expect("read Cargo.toml");
    assert!(
        manifest.contains("categories = [\"api-bindings\""),
        "Cargo.toml categories must include api-bindings"
    );
}

#[test]
fn contract_full_scan_example_requires_cli_target() {
    let example = fs::read_to_string("examples/full_scan.rs").expect("read example");
    assert!(
        !example.contains("testphp.vulnweb.com"),
        "example must not ship a default vulnweb target"
    );
    assert!(
        example.contains("std::env::args().nth(1)"),
        "example must require a CLI target URL argument"
    );
}
