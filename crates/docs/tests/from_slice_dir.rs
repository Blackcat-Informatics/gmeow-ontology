// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration coverage for [`DocsModel::from_slice_dir`] — the single-external-slice
//! constructor slice-quality's DocMaturity axis uses to score a FOREIGN slice's
//! documentation coverage from that slice's own files.
//!
//! The fixtures are self-contained single-slice trees (`manifest.ttl` + `module.ttl` +
//! `docs.md`); `from_slice_dir` points `SliceCatalog::discover` straight at one such dir,
//! which stops recursing at that dir's own `manifest.ttl`, yielding a catalog scoped to
//! exactly that one slice.
//!
//! They live with `from_slice_dir` itself, in `gmeow-docs-model`, whose own unit tests read
//! the SAME trees — one fixture set for one constructor. This crate reaches across to them
//! in the direction of its dependency edge (`gmeow-docs` → `gmeow-docs-model`), never the
//! reverse, so the leaf never has to know this crate exists.

use std::path::PathBuf;

use gmeow_docs::rdf::documentation_graph;
use gmeow_docs::{DocsError, DocsModel};

/// The path to a committed single-slice fixture directory, in the crate that owns
/// `from_slice_dir`.
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate is under <repo>/crates")
        .join("docs-model")
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// A model built over ONE external slice dir carries exactly one documented slice,
/// with a bounded coverage fraction and a covered/missing dimension set that
/// matches what the fixture's `module.ttl` + `docs.md` author.
#[test]
fn from_slice_dir_scores_exactly_one_slice() {
    let model = DocsModel::from_slice_dir(&fixture("single-slice"))
        .expect("a well-formed single-slice fixture must build a model");

    let graph = documentation_graph(&model);
    assert_eq!(
        graph.slices.len(),
        1,
        "from_slice_dir must scope the catalog to exactly one slice, not a repo sweep"
    );
    let slice = &graph.slices[0];
    assert_eq!(
        slice.documents,
        "https://blackcatinformatics.ca/gmeow/slices/fixture-single"
    );

    // The bounded fraction is an intrinsic [0,1] measure, never an unbounded ratio.
    assert!(
        (0.0..=1.0).contains(&slice.coverage_fraction),
        "coverage fraction must be in [0,1], got {}",
        slice.coverage_fraction
    );

    // Authored in the fixture: the term carries an rdfs:label and a skos:definition,
    // and docs.md opens with a prose thesis sentence — so these three dimensions are
    // COVERED.
    for covered in ["dimLabel", "dimDefinition", "dimThesisSentence"] {
        assert!(
            slice.covers.contains(covered),
            "the fixture authors {covered}, so it must be covered; covers = {:?}",
            slice.covers
        );
    }

    // Deliberately NOT authored: the fixture ships no realized-state design-set
    // table (and its one term is named in none), so the realized-state dimension is
    // MISSING — a gated miss, matching the fixture.
    assert!(
        !slice.covers.contains("dimRealizedState"),
        "the fixture authors no realized-state table, so dimRealizedState must be missed; \
         covers = {:?}",
        slice.covers
    );
}

/// `term_loss` is `None` on a `from_slice_dir` model BY DESIGN: a foreign slice was
/// never compiled through the pipeline's `stage-mappings`, so it has no dynamic
/// projection-loss ledger rows to attach. This pins that scope boundary as a
/// not-applicable fact — never an "unknown"/"failed" join.
#[test]
fn from_slice_dir_term_loss_is_none_by_design() {
    let model = DocsModel::from_slice_dir(&fixture("single-slice"))
        .expect("a well-formed single-slice fixture must build a model");
    assert!(
        model.term_loss.is_none(),
        "a foreign slice never ran stage-mappings, so term_loss is None by scope, not by failure"
    );
}

/// A manifest with no `a gmeow:Slice` triple is a HARD FAIL, never a vacuous empty
/// model — `from_slice_dir` surfaces the discovery error rather than succeeding on
/// an empty catalog.
#[test]
fn from_slice_dir_malformed_manifest_hard_fails() {
    let err = DocsModel::from_slice_dir(&fixture("malformed-slice"))
        .expect_err("a manifest with no `a gmeow:Slice` triple must hard-fail, not build empty");
    assert!(
        matches!(err, DocsError::Slice(_)),
        "a malformed manifest must surface a slice-catalog error, got {err:?}"
    );
}
