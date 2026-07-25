// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Executable acceptance check for the peerage-aware slice-dependency gate: the
//! real authored `slices/` tree must carry ZERO gating (`Severity::Error`)
//! ownership findings. This encodes the "zero violations at flip time" contract
//! — an undeclared cross-slice reference, a stale declaration, a tier-forbidden
//! crossing, or a peered reference off any registered seam each produces an
//! `Error` here and fails this test.
//!
//! It drives the SAME production surface `make validate` folds
//! (`slice_peerage::peerage_aware_ownership_findings` over the on-disk
//! `SliceCatalog` + `OwnershipReport`), but needs no `generated/` tree, so it is
//! a fast standalone regression gate: any future edit that reintroduces an
//! undeclared/stale/forbidden/off-seam cross-slice edge reds this test directly.

use std::path::Path;

use gmeow_errors::Severity;

/// The repo `slices/` directory, resolved from this crate's manifest dir.
fn slices_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../slices")
}

#[test]
fn the_peerage_aware_dependency_gate_is_clean_on_the_real_corpus() {
    let dir = slices_dir();
    assert!(
        dir.join("grounding/logic/manifest.ttl").is_file(),
        "expected the real slices/ tree at {}",
        dir.display()
    );

    let catalog = purrdf::slice::SliceCatalog::discover(
        &dir,
        purrdf::SliceVocab::for_namespace("https://blackcatinformatics.ca/gmeow/"),
    )
    .expect("discover the real slice catalog");
    let report = purrdf::slice::OwnershipAnalyzer::new(&catalog)
        .analyze()
        .expect("analyze slice ownership over the real corpus");

    let findings = gmeow_validate::slice_peerage::peerage_aware_ownership_findings(&report, &catalog)
        .expect("peerage-aware ownership projection must not hard-fail (join totality holds)");

    let errors: Vec<&gmeow_errors::Finding> = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .collect();

    assert!(
        errors.is_empty(),
        "the real slices/ tree carries {} gating slice-dependency error(s) — each is an \
         undeclared/stale/forbidden/off-seam cross-slice edge that must be declared, removed, \
         covered by a registered seam, or resolved to a legal tier:\n{}",
        errors.len(),
        errors
            .iter()
            .map(|f| format!("  [{}] {}", f.code, f.message))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
