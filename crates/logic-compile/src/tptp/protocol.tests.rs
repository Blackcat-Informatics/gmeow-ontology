// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::collections::BTreeSet;

use super::{FofTask, SzsOutcome, admit_szs_transcript};

fn names() -> BTreeSet<String> {
    BTreeSet::from(["gmeow_deadbeef".to_owned(), "gmeow_deadbeef.p".to_owned()])
}

#[test]
fn admits_repeated_bound_status_across_both_channels() {
    let admitted = admit_szs_transcript(
        "% SZS status Satisfiable for gmeow_deadbeef\n",
        "# SZS status Satisfiable for gmeow_deadbeef.p\n",
        &names(),
        FofTask::AxiomConsistency,
    )
    .expect("matching repeated status");
    assert_eq!(admitted.outcome, SzsOutcome::Consistent);
    assert_eq!(admitted.observations.len(), 2);
}

#[test]
fn status_without_problem_name_remains_bound_to_the_selected_child_invocation() {
    let admitted = admit_szs_transcript(
        "% SZS status Unsatisfiable\n",
        "",
        &names(),
        FofTask::AxiomConsistency,
    )
    .expect("unqualified status");
    assert_eq!(admitted.outcome, SzsOutcome::Inconsistent);
}

#[test]
fn rejects_conflicting_statuses() {
    let error = admit_szs_transcript(
        "% SZS status Satisfiable\n",
        "# SZS status Unsatisfiable\n",
        &names(),
        FofTask::AxiomConsistency,
    )
    .expect_err("conflict");
    assert_eq!(error.code, "CONFLICTING_SZS_STATUS");
}

#[test]
fn rejects_explicit_foreign_problem() {
    let error = admit_szs_transcript(
        "% SZS status Satisfiable for somebody_else\n",
        "",
        &names(),
        FofTask::AxiomConsistency,
    )
    .expect_err("foreign problem");
    assert_eq!(error.code, "FOREIGN_SZS_PROBLEM");
}

#[test]
fn rejects_conjecture_status_for_axiom_consistency() {
    for status in ["Theorem", "CounterSatisfiable"] {
        let error = admit_szs_transcript(
            &format!("% SZS status {status}\n"),
            "",
            &names(),
            FofTask::AxiomConsistency,
        )
        .expect_err("wrong task shape");
        assert_eq!(error.code, "SZS_TASK_SHAPE_MISMATCH");
    }
}

#[test]
fn rejects_missing_malformed_and_unknown_status() {
    let missing =
        admit_szs_transcript("ordinary output\n", "", &names(), FofTask::AxiomConsistency)
            .expect_err("missing");
    assert_eq!(missing.code, "MISSING_SZS_STATUS");

    let malformed = admit_szs_transcript("% SZS status\n", "", &names(), FofTask::AxiomConsistency)
        .expect_err("malformed");
    assert_eq!(malformed.code, "MALFORMED_SZS_STATUS");

    let unknown = admit_szs_transcript(
        "% SZS status MadeUp\n",
        "",
        &names(),
        FofTask::AxiomConsistency,
    )
    .expect_err("unknown");
    assert_eq!(unknown.code, "UNKNOWN_SZS_STATUS");
}
