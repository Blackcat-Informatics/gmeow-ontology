// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The native logic-conformance harness entry point (#785).
//!
//! `datatest-stable` discovers every `profile.json` sentinel under
//! `conformance/logic/cases/` and emits one nextest case PER FILE (i.e. per case
//! directory), run in parallel. Because the glob is on `profile.json` and the
//! case directory is its parent, discovery is category-agnostic: any future
//! `cases/external/<corpus>/<case>/` group (the #753 scope) is picked up
//! automatically once it adopts the same per-case anatomy.
//!
//! Each case runs the native engine ([`gmeow_conformance::run`]) and diffs the
//! produced artifacts against the committed `expected/` goldens
//! ([`gmeow_conformance::compare::diff_case`]). When `GMEOW_CONFORMANCE_BLESS=1`
//! is set the harness writes the goldens instead of asserting — the
//! golden-regeneration path that replaces `gmeow-dev conformance --update`.

use datatest_stable::Utf8Path;

use gmeow_conformance::discover;

/// Run one conformance case, identified by its `profile.json` sentinel path.
fn run_case_file(profile_json: &Utf8Path) -> datatest_stable::Result<()> {
    let case_dir = gmeow_conformance::paths::case_dir(profile_json.as_std_path());
    // Discovery validation: the directory must be a runnable case (input + profile).
    // Task 4/5 extend this to run the engine and diff against goldens.
    discover::validate_case(&case_dir)?;
    Ok(())
}

datatest_stable::harness! {
    { test = run_case_file, root = "../../conformance/logic/cases", pattern = r".*/profile\.json$" },
}
