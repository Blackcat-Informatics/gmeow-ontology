// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Live regression guard for the DSL SHACL phases the `make validate` gate runs.
//! These tests bind the REAL repository `dsl/` corpus and the REAL
//! committed `shapes/*-dsl-shapes.ttl` to the exact resolver the CLI uses, so a
//! future edit that unwires a DSL surface — or lets one go empty — fails here
//! instead of silently reintroducing the "help advertises DSL SHACL that never
//! runs" defect.
//!
//! Three independent, each-failable guards:
//!
//! 1. [`live_dsl_wiring_resolves_all_three_surfaces`] — the resolver yields
//!    non-empty inputs for every `OnValidate` DSL surface, and the three surfaces
//!    partition every `.ttl` under `dsl/` (disjoint + exhaustive), so a new
//!    `dsl/` subtree with no covering surface fails rather than going dark.
//! 2. [`real_dsl_conforms_to_committed_shapes`] — the real corpus actually
//!    conforms; this is the check the live gate now runs, proven green here.
//! 3. [`dsl_shacl_can_fail`] — the failability proof: a deliberately
//!    nonconforming fixture against the REAL committed shapes yields an Error, so
//!    the guard cannot degrade into a presence assertion.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use gmeow_errors::Severity;
use gmeow_validate::dsl_coverage::{PhaseHome, VALIDATE_PHASE_COVERAGE, authored_dsl_shacl_inputs};
use gmeow_validate::dsl_shacl::validate_dsl;
use gmeow_validate::validate_all::collect_ttl_paths;

/// Repository root: `crates/validate/../.. == <repo>` (same pattern as
/// `example_sweep.rs` and the foundation-corpus tests).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root must resolve from crates/validate")
}

fn ttl_set(dir: &str) -> BTreeSet<PathBuf> {
    collect_ttl_paths(dir)
        .unwrap_or_else(|e| panic!("collect_ttl_paths({dir}): {e}"))
        .into_iter()
        .collect()
}

/// The resolver must yield non-empty inputs for every OnValidate DSL surface, and
/// the three surfaces must PARTITION every `.ttl` under `dsl/` — disjoint (no file
/// double-validated) and exhaustive (no `dsl/` file left uncovered). Exhaustivity
/// is the anti-dark-phase teeth: a future `dsl/<newkind>/` tree that no surface
/// covers makes the union smaller than the whole `dsl/` walk and fails here,
/// forcing a new surface (and its registry row + help entry) rather than silent
/// non-coverage.
#[test]
fn live_dsl_wiring_resolves_all_three_surfaces() {
    let root = repo_root();
    let dsl = authored_dsl_shacl_inputs(&root)
        .expect("the committed dsl/ trees and shapes/ files must resolve for the live gate");

    // Every surface has a non-empty directory and non-empty shapes text.
    for (label, dir, shapes) in [
        ("mapping", &dsl.mapping_dir, &dsl.mapping_shapes),
        ("statement", &dsl.statement_dir, &dsl.statement_shapes),
        ("test", &dsl.test_dir, &dsl.test_shapes),
    ] {
        assert!(
            !ttl_set(dir).is_empty(),
            "{label} DSL directory {dir} resolved to zero .ttl files"
        );
        assert!(
            !shapes.trim().is_empty(),
            "{label} DSL shapes resolved empty"
        );
    }

    // Every registry row declared OnValidate that is a DSL surface must be one of
    // the three the resolver wired — the registry and the resolver cannot drift.
    let onvalidate_dsl: Vec<&str> = VALIDATE_PHASE_COVERAGE
        .iter()
        .filter(|p| p.home == PhaseHome::OnValidate && p.phase.ends_with("-dsl-shacl"))
        .map(|p| p.phase)
        .collect();
    assert_eq!(
        onvalidate_dsl,
        ["mapping-dsl-shacl", "statement-dsl-shacl", "test-dsl-shacl"],
        "the resolver wires exactly the registry's OnValidate DSL surfaces; they have drifted"
    );

    // Disjoint + exhaustive over `dsl/`.
    let mapping = ttl_set(&dsl.mapping_dir);
    let statement = ttl_set(&dsl.statement_dir);
    let test = ttl_set(&dsl.test_dir);
    assert!(
        mapping.is_disjoint(&statement)
            && mapping.is_disjoint(&test)
            && statement.is_disjoint(&test),
        "central-DSL surfaces overlap — a file would be validated by two surfaces"
    );
    let covered: BTreeSet<PathBuf> = mapping
        .iter()
        .chain(statement.iter())
        .chain(test.iter())
        .cloned()
        .collect();
    let all_dsl = ttl_set(&root.join("dsl").to_string_lossy());
    let uncovered: Vec<&PathBuf> = all_dsl.difference(&covered).collect();
    assert!(
        uncovered.is_empty(),
        "these .ttl files under dsl/ are covered by NO central-DSL SHACL surface (a dark \
         surface): {uncovered:?}. Add a covering surface to VALIDATE_PHASE_COVERAGE + the \
         resolver + the validate help, or move the files."
    );
}

/// The real `dsl/` corpus conforms to the committed shapes. This is the exact
/// verdict the live `make validate` gate now produces; proving it green here is
/// the PR-gating precondition. `validate_dsl` merges each surface
/// standalone (no TBox), so a red here is either genuine content debt or a shape
/// that (wrongly) assumes TBox-closed types — both fixed on-branch, never bypassed.
#[test]
fn real_dsl_conforms_to_committed_shapes() {
    let root = repo_root();
    let dsl = authored_dsl_shacl_inputs(&root).expect("resolve committed DSL inputs");

    for (label, dir, shapes) in [
        ("mapping", &dsl.mapping_dir, &dsl.mapping_shapes),
        ("statement", &dsl.statement_dir, &dsl.statement_shapes),
        ("test", &dsl.test_dir, &dsl.test_shapes),
    ] {
        let paths = collect_ttl_paths(dir).unwrap_or_else(|e| panic!("collect {dir}: {e}"));
        let findings = validate_dsl(&paths, shapes, label)
            .unwrap_or_else(|e| panic!("validate_dsl({label}): {e}"));
        let errors: Vec<String> = findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .map(|f| f.message.clone())
            .collect();
        assert!(
            errors.is_empty(),
            "real {label} DSL corpus does not conform to shapes/{label}-dsl-shapes.ttl \
             ({} error(s)): {errors:#?}",
            errors.len()
        );
    }
}

/// Failability proof: a deliberately nonconforming fixture validated against the
/// REAL committed statement shapes MUST yield at least one Error. A bare
/// `gmeow:StatementMetadata` node carries none of the properties the shape
/// requires (`gmeow:qSubject`/`gmeow:qPredicate` are `sh:minCount 1`), so it
/// violates without needing any TBox. This is what keeps
/// [`real_dsl_conforms_to_committed_shapes`] honest — it proves that test could
/// fail if the corpus regressed, rather than passing vacuously.
#[test]
fn dsl_shacl_can_fail() {
    let root = repo_root();
    let dsl = authored_dsl_shacl_inputs(&root).expect("resolve committed DSL inputs");

    let tmp = tempfile::tempdir().expect("create temp dir");
    let fixture = tmp.path().join("nonconforming.ttl");
    std::fs::write(
        &fixture,
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         gmeow:DeliberatelyNonconformingStatement a gmeow:StatementMetadata .\n",
    )
    .expect("write fixture");

    let findings = validate_dsl(&[fixture], &dsl.statement_shapes, "statement")
        .expect("validate_dsl over the fixture must not error");
    let errors = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .count();
    assert!(
        errors > 0,
        "a bare gmeow:StatementMetadata node must violate the committed statement DSL shapes; \
         got zero errors — the DSL SHACL guard has lost its teeth. Findings: {findings:#?}"
    );
}
