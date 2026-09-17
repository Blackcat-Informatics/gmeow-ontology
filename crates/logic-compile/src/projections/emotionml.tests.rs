// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn ledger_row_records_the_collapse() {
    let mut loss = LossLedger::new();
    let row = ledger_row(&mut loss);
    assert_records_collapse(&row, &loss).expect("the real row records the collapse");
}

#[test]
fn a_row_missing_the_collapse_record_is_rule9_red() {
    // A row whose target has NO interned drops (an empty loss store) must red: the
    // many-to-one collapse is unrecorded.
    let mut loss = LossLedger::new();
    let row = ledger_row(&mut loss);
    let empty = LossLedger::new();
    assert!(
        assert_records_collapse(&row, &empty).is_err(),
        "rule 9 must red when the many-to-one collapse is unrecorded"
    );
}

#[test]
fn render_emits_both_vocabularies_and_an_envelope() {
    let categories = vec![("anger".to_owned(), format!("{GMEOW_NS}emotionAnger"))];
    let dimensions = vec![
        ("valence".to_owned(), format!("{GMEOW_NS}dimensionValence")),
        ("arousal".to_owned(), format!("{GMEOW_NS}dimensionArousal")),
    ];
    // The COMPUTED schadenfreude worked values (valence 0.7 → 0.85, arousal 0.4 → 0.7,
    // intensity √(79/100) = 0.888819). The real compute + example pin live in the
    // pipeline's `correspondence_lower` test; here they are the fixed expected outputs.
    let worked = WorkedEnvelope {
        intensity: "0.888819".to_owned(),
        dimensions: vec![
            (format!("{GMEOW_NS}dimensionValence"), "0.85".to_owned()),
            (format!("{GMEOW_NS}dimensionArousal"), "0.7".to_owned()),
        ],
    };
    let doc = render_document(&categories, &dimensions, &worked);
    assert!(doc.contains("<vocabulary type=\"category\" id=\"gmeow-emotion-categories\">"));
    assert!(doc.contains("<vocabulary type=\"dimension\" id=\"gmeow-appraisal-dimensions\">"));
    assert!(doc.contains("<item name=\"anger\">"));
    // No fabricated constant: the computed unit-clamp valence + intensity, not "0.5".
    assert!(!doc.contains("value=\"0.5\""));
    assert!(doc.contains("<intensity value=\"0.888819\"/>"));
    assert!(doc.contains("<dimension name=\"valence\" value=\"0.85\"/>"));
    assert!(doc.contains("<dimension name=\"arousal\" value=\"0.7\"/>"));
    assert!(doc.trim_end().ends_with("</emotionml>"));
}
