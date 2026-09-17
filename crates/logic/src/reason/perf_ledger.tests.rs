// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// The ledger carries exactly the five still-live incremental boundaries. Built
/// optimization levers are removed rather than left behind as stale declarations.
#[test]
fn ledger_has_five_live_flagged_rows_and_no_stale_declared_levers() {
    let ledger = perf_ledger();
    assert_eq!(ledger.rows.len(), 5, "exactly five deferred rows remain");
    let flagged = ledger
        .rows
        .iter()
        .filter(|r| r.status == PerfStatus::FlaggedNonIncremental)
        .count();
    let p1 = ledger
        .rows
        .iter()
        .filter(|r| r.status == PerfStatus::DeclaredP1)
        .count();
    assert_eq!(flagged, 5, "five flagged-non-incremental boundaries");
    assert_eq!(p1, 0, "no already-built lever remains declared-p1");
    assert!(
        ledger
            .rows
            .iter()
            .all(|r| r.status == PerfStatus::FlaggedNonIncremental),
        "all remaining rows are real non-incremental boundaries"
    );
}

/// The emitted Turtle pins the exact canonical content and proves completed
/// levers are absent from the deferred ledger.
#[test]
fn turtle_golden_pins_five_rows_and_excludes_built_levers() {
    let ttl = perf_ledger().to_turtle();

    // Banner + the two status individuals are explained.
    assert!(
        ttl.contains("native physical engine performance ledger"),
        "the banner names the artifact"
    );
    assert!(
        ttl.contains("gmeow:FlaggedNonIncremental — a canonical hard part"),
        "the banner explains the FlaggedNonIncremental status"
    );
    assert!(
        ttl.contains("gmeow:DeclaredP1 — an advanced lever intentionally out of the current scope"),
        "the banner explains the DeclaredP1 status"
    );

    // The ledger header individual + entry count.
    assert!(ttl.contains("#type> <https://blackcatinformatics.ca/gmeow/PerfLedger>"));
    assert!(ttl.contains(&format!("gmeow/entryCount> \"5\"^^<{XSD_INTEGER}>")));

    // The three FlaggedNonIncremental hard parts (canon wording).
    for construct in [
        "incremental well-founded / stable-model solving",
        "existential-rule chase with termination and incrementality together",
        "paraconsistent / modal facets",
        "rule-program-changing conjecture candidates",
        "bounded retractions and non-positive counterfactual programs",
    ] {
        assert!(
            ttl.contains(construct),
            "the flagged-non-incremental hard part must appear verbatim: {construct}"
        );
    }
    // These three levers are built, so none may be misrepresented as deferred.
    for lever in [
        "provenance semirings",
        "compile-don't-interpret (specialize per content-addressed contract hash)",
        "worst-case-optimal joins",
    ] {
        assert!(
            !ttl.contains(&format!("construct> \"{lever}")),
            "a built lever must not remain a deferred construct: {lever}"
        );
    }

    // Only the status used by a live row is emitted as an object.
    assert!(ttl.contains(
        "gmeow/perfStatus> <https://blackcatinformatics.ca/gmeow/FlaggedNonIncremental>"
    ));
    assert!(!ttl.contains("gmeow/perfStatus> <https://blackcatinformatics.ca/gmeow/DeclaredP1>"));

    // Five entries of type gmeow:PerfLedgerEntry.
    assert_eq!(
        ttl.matches("#type> <https://blackcatinformatics.ca/gmeow/PerfLedgerEntry>")
            .count(),
        5,
        "exactly five PerfLedgerEntry rows are emitted"
    );

    // NO process tokens: no `#NNNN` issue/PR references, no `F3`/`T6` ticket
    // tokens (process flow lives only in the issue tracker, never here).
    for ch in ttl.chars().zip(ttl.chars().skip(1)) {
        assert!(
            !(ch.0 == '#' && ch.1.is_ascii_digit()),
            "no `#NNNN` issue/PR token may appear in the perf-ledger Turtle"
        );
    }
    assert!(
        !ttl.contains("F3") && !ttl.contains("T6"),
        "no F3/T6 ticket tokens may appear in the perf-ledger Turtle"
    );

    // Determinism: the emitter is a pure function of the canonical ledger.
    assert_eq!(
        ttl,
        perf_ledger().to_turtle(),
        "the perf-ledger Turtle must be byte-deterministic"
    );
}

#[test]
fn per_shot_row_never_mislabels_from_scratch_solving_as_incremental() {
    let rerun = nonmonotone_solve_run(NonmonotoneSolver::WellFounded, true, 1, 3);
    assert_eq!(rerun.status, PerfStatus::FlaggedNonIncremental);
    assert_eq!(rerun.solver.as_str(), "well-founded alternating fixpoint");
    assert!(rerun.solver_reran());
    assert_eq!(rerun.edb_changes, 1);
    assert_eq!(rerun.ground_rule_changes, 3);

    let reused = nonmonotone_solve_run(NonmonotoneSolver::StableModel, false, 0, 0);
    assert_eq!(
        reused.disposition,
        SolveDisposition::ReusedUnchangedGroundSlice
    );
    assert!(!reused.solver_reran());
    assert_eq!(reused.solver.as_str(), "stable-model cautious enumeration");
}
