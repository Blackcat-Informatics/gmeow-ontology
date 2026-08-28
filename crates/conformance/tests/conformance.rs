// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The native logic-conformance harness entry point.
//!
//! `datatest-stable` discovers every `profile.json` sentinel under
//! `conformance/logic/cases/` and emits one nextest case PER FILE (i.e. per case
//! directory), run in parallel. Because the glob is on `profile.json` and the
//! case directory is its parent, discovery is category-agnostic: any future
//! `cases/external/<corpus>/<case>/` group is picked up
//! automatically once it adopts the same per-case anatomy.
//!
//! Each case runs the native engine ([`gmeow_conformance::run`]) and diffs the
//! produced artifacts against the committed `expected/` goldens
//! ([`gmeow_conformance::compare::diff_case`]). The harness is read-only: corpus
//! generation is never reachable from a test process.

use datatest_stable::Utf8Path;

use gmeow_conformance::{compare, discover, run};

/// Run one conformance case, identified by its `profile.json` sentinel path.
///
/// Discover → run the native engine → diff against the committed `expected/`
/// goldens, surfacing every mismatch as one aggregated error.
fn run_case_file(profile_json: &Utf8Path) -> datatest_stable::Result<()> {
    let case_dir = gmeow_conformance::paths::case_dir(profile_json.as_std_path());

    // The `cases/bench/` subtree is the ENGINE-BENCHMARK corpus (relational-core /
    // chasebench mini representatives), a NEW sibling of the
    // OWL-consistency `cases/external/` tree. Its cases carry `program.rules` +
    // `input.nq` + `expected/result.json` (a hand-derived row/answer golden), NOT the
    // `input.logic.ttl` this consistency harness requires. It is loaded and run by the
    // dedicated bench harness (`gmeow_conformance::bench_corpus`), so this generic
    // per-`profile.json` harness MUST skip it — otherwise `validate_case` below would
    // hard-fail on the absent `input.logic.ttl`.
    if gmeow_conformance::paths::case_id(&case_dir).starts_with("bench/") {
        return Ok(());
    }

    discover::validate_case(&case_dir).map_err(|d| d.to_string())?;

    // Lane routing: a Lane-B external corpus is heavy / oracle-backed and runs
    // ONLY in the non-required external-corpus validation lane, never on the required gate. A
    // Divergence-lane corpus is the named honest-DlGap quarantine — those cases
    // are UNDECIDED by the native path (a gapped verdict the zero-defer
    // consistency runner refuses), so they are pinned exactly by the dedicated
    // divergence gate (`el_divergence_gate`) instead of this generic harness. A
    // Decided-lane corpus is the was-divergent-now-DECIDED set — those cases DO
    // decide cleanly, but their dedicated gate (`full_decided_gate`) is the single
    // live-re-run + partition-pin authority, so this generic harness skips them
    // too (mirroring the Divergence routing).
    // Skip all three here; Lane-A and endogenous cases always run.
    if matches!(
        gmeow_conformance::vendored::lane_for_case(&case_dir).map_err(|d| d.to_string())?,
        Some(gmeow_conformance::vendored::Lane::B)
            | Some(gmeow_conformance::vendored::Lane::Divergence)
            | Some(gmeow_conformance::vendored::Lane::Decided)
    ) {
        return Ok(());
    }

    let outputs = run::run_case(&case_dir).map_err(|d| d.to_string())?;

    let diffs = compare::diff_case(&case_dir, &outputs);
    if diffs.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} golden mismatch(es) for {}:\n  • {}",
            diffs.len(),
            outputs.case_id,
            diffs.join("\n  • ")
        )
        .into())
    }
}

datatest_stable::harness! {
    { test = run_case_file, root = "../../conformance/logic/cases", pattern = r".*/profile\.json$" },
}
