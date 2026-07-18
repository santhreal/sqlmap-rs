//! Property-based tests for findings formatting and parsing invariants.
use proptest::prelude::*;
use serde_json::json;
use sqlmap_rs::types::{format_findings, DataResponse, OutputFormat, SqlmapFinding};

proptest! {
    #[test]
    fn prop_finding_json_roundtrip_fields(
        parameter in "[a-zA-Z0-9_]{1,16}",
        vulnerability_type in "[a-zA-Z0-9 _-]{1,32}",
        payload in "[a-zA-Z0-9' =-]{1,32}",
    ) {
        let original = SqlmapFinding::new(
            &parameter,
            &vulnerability_type,
            &payload,
            json!({"source": "proptest"}),
        );
        let json_out = format_findings(std::slice::from_ref(&original), OutputFormat::Json);
        let parsed: Vec<SqlmapFinding> = serde_json::from_str(&json_out)
            .expect("roundtrip JSON must deserialize");
        prop_assert_eq!(parsed.len(), 1);
        prop_assert_eq!(&parsed[0].parameter, &parameter);
        prop_assert_eq!(&parsed[0].vulnerability_type, &vulnerability_type);
        prop_assert_eq!(&parsed[0].payload, &payload);
    }

    #[test]
    fn prop_finding_json_always_parseable_array(
        findings in prop::collection::vec(
            (
                any::<String>(),
                any::<String>(),
                any::<String>(),
            ),
            0..12,
        )
    ) {
        let findings: Vec<SqlmapFinding> = findings
            .into_iter()
            .map(|(parameter, vulnerability_type, payload)| {
                SqlmapFinding::new(parameter, vulnerability_type, payload, json!({}))
            })
            .collect();
        let json_out = format_findings(&findings, OutputFormat::Json);
        let parsed: Vec<SqlmapFinding> = serde_json::from_str(&json_out)
            .expect("JSON output must always deserialize as a findings array");
        prop_assert_eq!(parsed.len(), findings.len());
        prop_assert!(!json_out.contains("\"error\""));
    }

    #[test]
    fn prop_csv_header_and_single_data_line(
        parameter in "[a-zA-Z0-9_]{1,16}",
        vulnerability_type in "[a-zA-Z0-9 _-]{1,32}",
        payload in "[a-zA-Z0-9]{1,24}",
    ) {
        let finding = SqlmapFinding::new(&parameter, &vulnerability_type, &payload, json!({}));
        let csv = format_findings(&[finding], OutputFormat::Csv);
        let lines: Vec<&str> = csv.lines().collect();
        prop_assert_eq!(lines.len(), 2);
        prop_assert_eq!(lines[0], "parameter,vulnerability_type,payload");
        let expected_prefix = format!("{parameter},");
        prop_assert!(lines[1].starts_with(&expected_prefix));
    }

    #[test]
    fn prop_type_0_chunks_always_empty(value in any::<i64>()) {
        let resp: DataResponse = serde_json::from_value(json!({
            "success": true,
            "data": [{ "type": 0, "value": value }],
            "error": null,
            "message": null
        })).expect("fixture");
        prop_assert!(resp.findings().is_empty());
    }
}
