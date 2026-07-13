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
//! ([`gmeow_conformance::compare::diff_case`]). When `GMEOW_CONFORMANCE_BLESS=1`
//! is set the harness writes the goldens instead of asserting — the
//! golden-regeneration path that replaces `gmeow-dev conformance --update`.

use datatest_stable::Utf8Path;

use gmeow_conformance::{bless, compare, discover, run};

/// The environment variable that switches the harness into golden-regeneration
/// (bless) mode: when set to a truthy value, each case writes its goldens instead
/// of asserting against them.
const BLESS_ENV: &str = "GMEOW_CONFORMANCE_BLESS";

/// Run one conformance case, identified by its `profile.json` sentinel path.
///
/// Default (assert) mode: discover → run the native engine → diff against the
/// committed `expected/` goldens, surfacing every mismatch as one aggregated
/// error. Bless mode (`GMEOW_CONFORMANCE_BLESS=1`): regenerate the reproducible
/// goldens instead of asserting.
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
    // ONLY in the non-required `make -C validations/classic-cross-check validate` lane. A
    // Divergence-lane corpus is the named honest-DlGap quarantine — those cases
    // are UNDECIDED by the native path (a gapped verdict the zero-defer
    // consistency runner refuses), so they are pinned exactly by the dedicated
    // divergence gate (`el_divergence_gate`) instead of this generic harness.
    // Skip both here; Lane-A and endogenous cases always run.
    if matches!(
        gmeow_conformance::external::lane_for_case(&case_dir).map_err(|d| d.to_string())?,
        Some(gmeow_conformance::external::Lane::B)
            | Some(gmeow_conformance::external::Lane::Divergence)
    ) {
        return Ok(());
    }

    let outputs = run::run_case(&case_dir).map_err(|d| d.to_string())?;

    if bless_enabled() {
        bless::write_expected(&case_dir, &outputs).map_err(|d| d.to_string())?;
        return Ok(());
    }

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

/// Whether the bless env var is set to a truthy value (`1`/`true`/`yes`, any case).
fn bless_enabled() -> bool {
    std::env::var(BLESS_ENV)
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

datatest_stable::harness! {
    { test = run_case_file, root = "../../conformance/logic/cases", pattern = r".*/profile\.json$" },
}
