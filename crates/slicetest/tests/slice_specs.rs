// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The native slice-test harness entry point (#784).
//!
//! `datatest-stable` discovers every slice-resident test-DSL spec file under
//! `slices/**/tests/` and emits one nextest case PER FILE, run in parallel. Each
//! of the three cell types has its own fixed-filename glob and executor; the
//! executor (in [`gmeow_slicetest::exec`]) parses the file, enumerates its
//! cells, and runs them all, surfacing every failing cell in one aggregated
//! error.
//!
//! `tests/counter-examples/*.ttl` is excluded structurally: it never matches the
//! three fixed spec filenames (it is referenced only via `gmeow:exampleFile`).

use datatest_stable::Utf8Path;

use gmeow_slicetest::exec;

fn run_competency_file(path: &Utf8Path) -> datatest_stable::Result<()> {
    exec::run_competency_file(path.as_std_path())?;
    Ok(())
}

fn run_structural_file(path: &Utf8Path) -> datatest_stable::Result<()> {
    exec::run_structural_file(path.as_std_path())?;
    Ok(())
}

fn run_conformance_file(path: &Utf8Path) -> datatest_stable::Result<()> {
    exec::run_conformance_file(path.as_std_path())?;
    Ok(())
}

datatest_stable::harness! {
    { test = run_competency_file, root = "../../slices", pattern = r".*/tests/competency\.ttl$" },
    { test = run_structural_file, root = "../../slices", pattern = r".*/tests/structural\.ttl$" },
    { test = run_conformance_file, root = "../../slices", pattern = r".*/tests/example-conformance\.ttl$" },
}
