// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::cell::Cell;

use super::*;

#[test]
fn reason_verify_orchestration_invokes_the_result_producer_once() {
    let dataset = purrdf::RdfDataset::union(&[]);
    let calls = Cell::new(0usize);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let gates = gmeow_pipeline::fixture::authenticated_reasoned_gates(&root)
        .expect("producer-selected native law preparation");
    let verification = PreparedVerification::new(&[], &gates).expect("empty selected query set");
    let evaluation = evaluate_reason_verify_once(&dataset, &verification, || {
        calls.set(calls.get() + 1);
        reason_all(
            gmeow_logic::reason::prepare_reasoning_input(&dataset)?,
            &gmeow_logic::reasoning_graphs::object_level_domains()?,
        )
    })
    .expect("empty dataset reasons and verifies");

    assert_eq!(
        calls.get(),
        1,
        "the complete closure is produced exactly once"
    );
    assert!(evaluation.result.is_decided_consistent());
    assert!(evaluation.report.ok());
}
