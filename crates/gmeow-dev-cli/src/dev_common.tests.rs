// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// Resolve nested invocations locally and never replace an unrelated selected directory.
#[test]
fn checkout_discovery_preserves_the_selected_tree_without_a_compiled_fallback() {
    let directory = tempfile::tempdir().expect("isolated selection");
    let selected = directory.path().join("checkout");
    let nested = selected.join("crates/example/src");
    std::fs::create_dir_all(&nested).expect("nested invocation directory");
    std::fs::create_dir(selected.join("slices")).expect("source tree");
    std::fs::write(selected.join("Cargo.toml"), b"[workspace]\n").expect("marker");
    assert_eq!(enclosing_checkout(&nested), selected);
    assert_eq!(enclosing_checkout(&selected), selected);
    let unrelated = directory.path().join("unrelated");
    std::fs::create_dir(&unrelated).expect("unrelated invocation directory");
    assert_eq!(enclosing_checkout(&unrelated), unrelated);
}

/// Preserve timing artifact profiles, native measurements and direct-write failures.
#[test]
fn timing_writer_preserves_complete_authored_metadata_packets() {
    use gmeow_validate::validate_all::{
        ExampleExecution, ExampleValidationMeasurements, ExampleValidationSample, ExampleViews,
    };
    use purrdf_lex::json::record::ToJson as _;
    let directory = tempfile::tempdir().unwrap();
    let example_work = ExampleValidationMeasurements {
        base_projection_us: u128::from(u64::MAX),
        shape_preparation_us: 0,
        examples: vec![
            ExampleValidationSample {
                example: "slice/é中😀.ttl".into(),
                execution: None,
            },
            ExampleValidationSample {
                example: "slice/bound.ttl".into(),
                execution: Some(ExampleExecution {
                    parse_us: 1,
                    projection_us: 2,
                    binding_us: 3,
                    validation_us: 4,
                    views: Some(ExampleViews {
                        retained_payload_bytes: 1,
                        composite_auxiliary_bytes: 2,
                        copied_terms: 3,
                        copied_rows: 4,
                        copied_text_bytes: 5,
                        copied_index_bytes: 6,
                        freezes: 7,
                        composite_materializations: 8,
                        core_auxiliary_bytes: 9,
                        query_auxiliary_bytes: 10,
                        core_materializations: 11,
                        query_materializations: 12,
                    }),
                }),
            },
        ],
    };
    let references = include_str!("dev_common/fixtures/timings-json-v1.txt");
    assert_eq!(references.lines().count(), 7);
    for row in references.lines() {
        let mut fields = row.split_whitespace();
        let name = fields.next().unwrap();
        let digest = fields.next().unwrap();
        let original = purrdf_hash::hex::decode(fields.next().unwrap()).unwrap();
        assert!(fields.next().is_none());
        let mut packet =
            purrdf_lex::json::read_slice(&original, purrdf_lex::json::Limits::DEFAULT).unwrap();
        if name == "validate" {
            // Both published phase arrays must retain the actual typed measurements.
            packet["observations"]["timings"][0]["example_work"] = example_work.to_json();
            packet["timings"][0]["example_work"] = example_work.to_json();
        } else if name == "fanout" {
            // Deliberate insertion order independently exercises root-key sorting.
            packet = purrdf_lex::json::Object::new()
                .with("written", 3_usize)
                .with("skipped", 3_usize)
                .with("produced", 3_usize)
                .with("command", "fanout")
                .into();
        }
        let path = directory.path().join(format!("nested/{name}.json"));
        assert_eq!(write_timings_json(&path, &packet), 0);
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes, original, "{name}");
        assert_eq!(purrdf::ContentDigest::of(&bytes).to_hex(), digest, "{name}");
    }
    let blocked = directory.path().join("not-a-directory");
    std::fs::write(&blocked, b"preserved").unwrap();
    let selected = purrdf_lex::json::Object::new()
        .with("selected", true)
        .into();
    assert_eq!(
        write_timings_json(&blocked.join("timings.json"), &selected),
        1
    );
    assert_eq!(std::fs::read(blocked).unwrap(), b"preserved");
}
