// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Independently authored outcome bytes; no producer, source or corpus inputs.
use super::*;

#[test]
fn json_recorded_result_writer_success_literal_and_reader_preserve_exact_text() {
    let value: Result<String, crate::RecordedDiag> = Ok("中\n\"\\".into());
    let expected = r#"{"Ok":"中\n\"\\"}"#;
    let json = recorded_result_to_json(&value);
    assert_eq!(purrdf_lex::json::write_compact(&json), expected);
    let restored =
        read_recorded_result::<String>(&json, "authored Result", "one authored Result variant")
            .unwrap();
    assert_eq!(restored, value);
}

#[test]
fn json_recorded_result_writer_complete_refusal_literal_roundtrips_unchanged() {
    let refusal = r#"{"root":{"fingerprint":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"stage":"authored","grade":{"severity":"error","category":"data-shape-violation","standpoint":"binding"},"code":"authored.refusal","observations":[],"frames":[],"antecedents":[],"source_ctx":{"location":{},"focus":null,"term_role":null},"attributions":[],"advice":[],"labels":[],"tags":[],"knowledge":"supported","emitted_at":{"file":"authored.rs","line":1,"column":2}},"causes":[]}"#;
    let recorded: crate::RecordedDiag =
        purrdf_lex::json::record::from_slice(refusal.as_bytes()).unwrap();
    let outcome: Result<String, crate::RecordedDiag> = Err(recorded);
    let expected = format!("{{\"Err\":{refusal}}}");
    let json = recorded_result_to_json(&outcome);
    assert_eq!(purrdf_lex::json::write_compact(&json), expected);
    let restored =
        read_recorded_result::<String>(&json, "authored Result", "one authored Result variant")
            .unwrap();
    assert_eq!(restored, outcome);
}
