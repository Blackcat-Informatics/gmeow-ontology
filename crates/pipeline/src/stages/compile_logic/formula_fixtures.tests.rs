// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_logic_compile::ir::PreservationKind;

#[test]
fn selection_records_absence_without_substituting_another_formula() {
    let formula = Formula::atom(
        Term::Iri("urn:selected".to_owned()),
        vec![
            Term::Iri("urn:A".to_owned()),
            Term::Iri("urn:B".to_owned()),
            Term::Iri("urn:C".to_owned()),
        ],
    )
    .unwrap();
    let program =
        LogicProgram::new(Vec::new(), Vec::new(), Vec::new(), None).with_formulas(vec![formula]);
    let observation = observe(program, Vec::new(), Some("urn:missing"));
    assert_eq!(observation.formulas.len(), 1);
    assert_eq!(observation.selected_formulas, 0);
    assert!(observation.lowering.unwrap().existential_rules.is_empty());
}

#[test]
fn selected_ternary_formula_records_actual_shared_native_witness() {
    let formula = Formula::atom(
        Term::Iri("urn:selected".to_owned()),
        vec![
            Term::Iri("urn:A".to_owned()),
            Term::Iri("urn:B".to_owned()),
            Term::Iri("urn:C".to_owned()),
        ],
    )
    .unwrap();
    let program =
        LogicProgram::new(Vec::new(), Vec::new(), Vec::new(), None).with_formulas(vec![formula]);
    let observation = observe(program, Vec::new(), Some("urn:selected"));
    let bytes = serde_json::to_vec(&observation).unwrap();
    let restored: FormulaObservation = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(restored.selected_formulas, 1);
    let lowered = restored.lowering.unwrap();
    assert!(lowered.rules.is_empty());
    assert_eq!(lowered.existential_rules.len(), 1);
    let head = &lowered.existential_rules[0].head;
    assert_eq!(head.len(), 4);
    assert!(head.iter().all(|atom| atom.subject == head[0].subject));
    assert!(
        lowered
            .preservation
            .polarities
            .contains(&PreservationKind::Exact)
    );
}
