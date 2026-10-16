// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only inspection of prepared native source facts.

pub(super) use purrdf::RdfDataset;

use super::{InputFacts, PreparedReasoningInput, WorldGraphs, prepare_reasoning_input};

/// Read every native statement once, retaining the exact source graph bindings.
pub(in crate::reason) fn input_facts(
    edb: &impl purrdf::DatasetView<ReadError = std::convert::Infallible>,
) -> gmeow_errors::Result<(InputFacts, WorldGraphs)> {
    let PreparedReasoningInput { facts, graphs, .. } = prepare_reasoning_input(edb)?;
    Ok((facts, graphs))
}

/// A private physical closure cannot certify public completeness merely because
/// execution returned a value. Require the actual native consumer's typed refusal.
pub(super) fn assert_obstructed(
    program: &gmeow_logic_compile::ir::LogicProgram,
    closure: super::ProgramClosure,
    expected: crate::reason::refute::NativeObstructionKind,
) -> crate::result::ReasoningResult {
    let prepared = crate::program_analysis::prepare_program(program).unwrap();
    let (result, _, _) = super::super::result_from_closure(&prepared, closure, None).unwrap();
    result.validate_native_closure().unwrap();
    assert_eq!(
        result.completeness,
        crate::result::CompletenessStatus::Incomplete
    );
    assert_eq!(
        result.information,
        crate::result::InformationState::Undetermined
    );
    let execution = result.native_execution().unwrap();
    let obstructions: Vec<_> = execution
        .source_coverage
        .worlds
        .values()
        .flat_map(|world| &world.admissions)
        .flat_map(|admission| &admission.obstructions)
        .chain(
            execution
                .families
                .iter()
                .flat_map(|ledger| &ledger.producer_obstructions)
                .map(|obstruction| &obstruction.reason),
        )
        .chain(
            execution
                .families
                .iter()
                .flat_map(|ledger| &ledger.outcomes)
                .flat_map(|outcome| &outcome.obstructions),
        )
        .chain(
            execution
                .classes
                .iter()
                .flat_map(|class| &class.obstructions),
        )
        .collect();
    assert!(
        obstructions
            .iter()
            .any(|obstruction| obstruction.kind == expected),
        "expected {expected:?}: {obstructions:?}"
    );
    assert!(!result.preservation.unsupported_constructs.is_empty());
    result
}
