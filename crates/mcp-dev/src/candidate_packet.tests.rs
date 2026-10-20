// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn packet_submission_schema_binds_theory_and_provenance() {
    let schema = descriptor();
    assert_eq!(
        schema["inputSchema"]["required"],
        json!(["formula", "standpoint", "slice_dir"])
    );
    for key in ["kb", "kb_format", "for_slice", "for_packet"] {
        assert!(schema["inputSchema"]["properties"].get(key).is_none());
    }
    for key in ["dry_run", "max_steps", "max_answers", "math_conjecture"] {
        assert_eq!(
            schema["inputSchema"]["properties"][key],
            gmeow_mcp::candidate_submission_descriptor()["inputSchema"]["properties"][key]
        );
    }
    assert_eq!(schema["annotations"]["readOnlyHint"], false);
}

#[test]
fn packet_submission_rejects_overrides_and_invalid_selectors() {
    let base =
        json!({"slice_dir":"slices/core/ai", "formula":"candidate", "standpoint":"urn:standpoint"});
    assert_eq!(selectors(&base).unwrap(), (None, None));
    for (key, value) in [
        ("kb", json!("forged")),
        ("for_packet", json!("urn:forged")),
        ("kb_format", json!("text/turtle")),
        ("for_slice", json!("urn:forged")),
        ("axis", json!(false)),
        ("batch", json!(-1)),
        ("batch", json!(u64::MAX)),
    ] {
        let mut invalid = base.clone();
        invalid[key] = value;
        assert!(selectors(&invalid).is_err(), "{invalid}");
    }
    let mut valid = base;
    valid["axis"] = json!("A");
    valid["batch"] = json!(2);
    assert_eq!(selectors(&valid).unwrap(), (Some("A"), Some(2)));
}

#[test]
fn packet_submission_registration_has_one_owned_handler() {
    let surface = gmeow_mcp::extension::Surface::assemble(
        gmeow_mcp::extension::Extension::new(),
        crate::dev_extension(PathBuf::from("unused-checkout")),
    )
    .unwrap();
    assert_eq!(
        surface.tool_names(),
        [
            "submit_packet_candidate",
            "validate",
            "reason",
            "sync",
            "constitution"
        ]
    );
    assert_eq!(surface.resource_descriptors().len(), 2);
}

#[test]
fn packet_submission_cannot_read_outside_its_bound_checkout() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().join("checkout");
    let inside = root.join("slice");
    let outside = fixture.path().join("outside");
    std::fs::create_dir_all(&inside).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    assert_eq!(
        selected_directory(&root, "slice").unwrap(),
        inside.canonicalize().unwrap()
    );
    assert!(selected_directory(&root, "../outside").is_err());
}
