// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Executable acceptance check for the peerage-aware slice-dependency gate: the
//! real authored `slices/` tree must carry ZERO gating (`Severity::Error`)
//! ownership findings. This encodes the "zero violations at flip time" contract
//! — an undeclared cross-slice reference, a stale declaration, a tier-forbidden
//! crossing, a grounding slice depending on a non-grounding slice, or a peered
//! reference off any registered seam each produces an `Error` here and fails
//! this test.
//!
//! It drives the SAME production surface `make validate` folds
//! (`slice_peerage::peerage_aware_ownership_findings` over the on-disk
//! `SliceCatalog` + `OwnershipReport`), but needs no `generated/` tree, so it is
//! a fast standalone regression gate: any future edit that reintroduces an
//! undeclared/stale/forbidden/grounding-downward/off-seam cross-slice edge reds
//! this test directly.
//!
//! The zero-error assertion is paired with NON-VACUITY guards, because "no errors"
//! and "nothing was judged" are the same observation otherwise. The test also
//! asserts the POSITIVE half of the contract directly: the corpus carries
//! seam-covered peered crossings, and at least one undeclared peered edge is
//! suppressed as `Coverage::Covered` rather than merely producing no finding.

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

    let catalog = purrdf::slice::SliceCatalog::discover(&dir, gmeow_ns::gmeow_slice_vocab())
        .expect("discover the real slice catalog");
    let report = purrdf::slice::OwnershipAnalyzer::new(&catalog)
        .analyze()
        .expect("analyze slice ownership over the real corpus");

    // NON-VACUITY. `errors.is_empty()` below is only meaningful if the engine actually
    // had cross-slice references to judge: a catalog that discovered nothing, or a
    // corpus with no peered crossings at all, would satisfy it while proving nothing.
    // These three guards pin that there IS something to look at, and they double as the
    // positive half of the contract — "peered + seam-registered references PASS" is not
    // witnessed by an absence of errors, it is witnessed by `Coverage::Covered`.
    let classification = gmeow_validate::slice_peerage::classify(&report, &catalog)
        .expect("classify the real corpus against the peerage + seam registry");
    assert!(
        !classification.crossings.is_empty(),
        "the real corpus must carry seam-COVERED peered crossings — an empty coverage \
         table means the seam registry sanctioned nothing and this gate is vacuous"
    );
    assert!(
        classification
            .verdicts
            .iter()
            .any(|v| v.coverage == gmeow_validate::slice_peerage::Coverage::Covered),
        "at least one undeclared peered edge must classify as Coverage::Covered — that is \
         the whole point of the seam registry, and a corpus with no Covered edge would \
         pass this gate without ever exercising the suppression path"
    );
    assert!(
        report.edges.len() > 100,
        "the real corpus must yield a substantial computed dependency graph; got {} edge(s) \
         — a near-empty graph means discovery or analysis silently found nothing",
        report.edges.len()
    );

    let findings =
        gmeow_validate::slice_peerage::peerage_aware_ownership_findings(&report, &catalog)
            .expect("peerage-aware ownership projection must not hard-fail (join totality holds)");

    let errors: Vec<&gmeow_errors::Finding> = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .collect();

    assert!(
        errors.is_empty(),
        "the real slices/ tree carries {} gating slice-dependency error(s) — each is an \
         undeclared/stale/tier-forbidden/grounding-downward/off-seam cross-slice edge that must \
         be declared, removed, covered by a registered seam, resolved to a legal tier, or \
         reversed so the grounding slice owns the concept:\n{}",
        errors.len(),
        errors
            .iter()
            .map(|f| format!("  [{}] {}", f.code, f.message))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
