// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_logic_compile::projections::correspondence::project_correspondence_dataset;

#[test]
fn fidelity_observation_rejects_an_altered_cell() {
    let expected = super::super::synthetic_affine_program();
    let projection = project_correspondence_dataset(&expected).unwrap();
    observe(&expected, &projection).unwrap();
    let mut changed = expected;
    changed.correspondences.clear();
    let mut artifacts = BTreeMap::new();
    record(&changed, &projection, &mut artifacts).unwrap();
    let result: Outcome = purrdf_lex::json::record::from_slice(&artifacts[CHANNEL]).unwrap();
    assert!(result.0.is_err());
}

#[test]
fn native_correspondence_outcome_preserves_null_and_complete_refusals() {
    use purrdf_lex::json::record::{from_slice, to_vec};
    let success: Outcome = from_slice(br#"{"Ok":null}"#).unwrap();
    assert!(success.0.is_ok());
    assert_eq!(to_vec(&success), br#"{"Ok":null}"#);
    let mut ledger = gmeow_errors::DiagLedger::new();
    let premise = ledger.attach(
        gmeow_errors::Diag::from(std::io::Error::other("authored inverse premise"))
            .with_focus("urn:correspondence:premise"),
        gmeow_errors::StageId::new("correspondence-input"),
    );
    let refusal = ledger.record(
        gmeow_errors::Diag::from(std::io::Error::other("inverse differs"))
            .with_focus("urn:correspondence:inverse")
            .with_antecedents([premise]),
        gmeow_errors::StageId::new("correspondence-inverse"),
    );
    let retained: Outcome = from_slice(&to_vec(&Outcome(Err(refusal.clone())))).unwrap();
    assert_eq!(retained.0.unwrap_err(), refusal);
    assert_eq!(refusal.causes.len(), 1);
    for invalid in [
        r#"{"Ok":[]}"#,
        r#"{"Ok":null,"Ok":null}"#,
        r#"{"Ok":null,"Err":null}"#,
        r#"{"Err":{}}"#,
        r#"{"Other":null}"#,
        "{}",
        "null",
        "[null]",
    ] {
        assert!(
            from_slice::<Outcome>(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
}
