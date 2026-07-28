// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Whole-corpus authoring-integrity gates over the committed repository. Each
//! detector must find ZERO violations on the real corpus (the gate), and the
//! corpus it scans must be genuinely non-empty (non-vacuity — a silently empty
//! scan would make "zero findings" meaningless). The detectors' *detection* logic
//! (that they fire on a bad input) is proven by the synthetic-negative unit tests
//! in `authoring_integrity`'s `#[cfg(test)]` module; here we prove the shipped
//! corpus is clean.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use gmeow_validate::authoring_integrity;
use gmeow_validate::codes;
use gmeow_validate::lint::LintConfig;
use gmeow_validate::validate_all::{ValidateOptions, ValidationRun};

/// The repository root — the `gmeow-validate` crate lives at `crates/validate`.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the validate crate should live under crates/")
        .to_path_buf()
}

#[test]
fn shape_iri_ownership_is_collision_free() {
    let root = repo_root();
    // Non-vacuity: the shape corpus must be genuinely populated.
    let shape_files =
        purrdf::shapes::shape_union::shape_files(&root).expect("enumerate the merged shape corpus");
    assert!(
        !shape_files.is_empty(),
        "merged shape corpus is empty — the collision sweep would be vacuous"
    );

    let findings =
        authoring_integrity::shape_iri_collision_findings(&root).expect("shape collision sweep");
    assert!(
        findings.is_empty(),
        "shape-IRI ownership collisions in the committed corpus:\n{}",
        findings
            .iter()
            .map(|f| f.message.clone())
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn core_rights_module_has_no_norms_extension_leak() {
    let root = repo_root();
    // Non-vacuity: the core rights module must exist and parse to a non-empty graph.
    let path = root.join("slices/core/rights/module.ttl");
    assert!(path.is_file(), "core rights module missing at {path:?}");

    let findings = authoring_integrity::graft_isolation_findings(&root).expect("graft isolation");
    assert!(
        findings.is_empty(),
        "core rights module references norms-extension IRIs:\n{}",
        findings
            .iter()
            .map(|f| f.message.clone())
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn slice_discipline_is_clean_across_the_committed_manifests() {
    let root = repo_root();
    let slices_dir = root.join("slices");
    assert!(slices_dir.is_dir(), "slices/ directory missing");

    let findings =
        authoring_integrity::slice_discipline_findings(&slices_dir).expect("slice discipline");
    assert!(
        findings.is_empty(),
        "slice-discipline defects (duplicate IRI or missing tier) in the committed manifests:\n{}",
        findings
            .iter()
            .map(|f| f.message.clone())
            .collect::<Vec<_>>()
            .join("\n")
    );
}

fn joined(findings: &[gmeow_errors::Finding]) -> String {
    findings
        .iter()
        .map(|f| f.message.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn profile_and_partition_closure_is_clean() {
    let root = repo_root();
    // Non-vacuity: the profile documents must exist and the partition must have
    // genuinely populated core + extension sets (guarded by the detector's
    // full == ontology ∪ extensions check, which is non-trivial only when
    // extensions is non-empty).
    assert!(
        root.join("generated/profiles/full.ttl").is_file(),
        "generated/profiles/full.ttl missing"
    );
    assert!(
        root.join("generated/profiles/claims.ttl").is_file(),
        "generated/profiles/claims.ttl missing"
    );

    let findings = authoring_integrity::profile_closure_findings(&root).expect("profile closure");
    assert!(
        findings.is_empty(),
        "profile/partition closure defects:\n{}",
        joined(&findings)
    );
}

#[test]
fn every_slice_module_is_in_the_catalog() {
    let root = repo_root();
    // Non-vacuity: the catalog must parse to a genuinely populated name set.
    let names = purrdf_free_catalog_names(&root);
    assert!(
        names > 1,
        "catalog parsed {names} <uri> entries — the closure check would be vacuous"
    );

    let findings = authoring_integrity::catalog_closure_findings(&root).expect("catalog closure");
    assert!(
        findings.is_empty(),
        "slice modules absent from catalog-v001.xml:\n{}",
        joined(&findings)
    );
}

/// A local re-parse of the catalog `<uri>` name count for the non-vacuity guard —
/// independent of the detector, so a broken detector cannot make the guard pass.
fn purrdf_free_catalog_names(root: &Path) -> usize {
    let text = std::fs::read_to_string(root.join("catalog-v001.xml")).expect("read catalog");
    let doc = roxmltree::Document::parse(&text).expect("parse catalog");
    doc.descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "uri")
        .count()
}

#[test]
fn every_slice_module_iri_matches_its_location() {
    let root = repo_root();
    let findings = authoring_integrity::module_iri_findings(&root).expect("module iri");
    assert!(
        findings.is_empty(),
        "module owl:Ontology IRIs that do not match their location:\n{}",
        joined(&findings)
    );
}

#[test]
fn coverage_fixtures_use_only_declared_terms() {
    let root = repo_root();
    let declared = authoring_integrity::declared_ontology_terms(&root).expect("declared terms");
    // Non-vacuity on the EXTRACTED set: the declared authority must be populated,
    // not merely the file globs.
    assert!(
        declared.len() > 50,
        "declared-term set is implausibly small ({}) — the authority is vacuous",
        declared.len()
    );
    let findings = authoring_integrity::coverage_fixture_undeclared_findings(&root, &declared)
        .expect("fixture term check");
    assert!(
        findings.is_empty(),
        "coverage fixtures reference undeclared terms:\n{}",
        joined(&findings)
    );
}

#[test]
fn slice_examples_use_only_declared_terms() {
    let root = repo_root();
    let declared = authoring_integrity::declared_ontology_terms(&root).expect("declared terms");
    let findings = authoring_integrity::example_undeclared_term_findings(&root, &declared)
        .expect("example term check");
    assert!(
        findings.is_empty(),
        "slice examples reference undeclared terms:\n{}",
        joined(&findings)
    );
}

/// R9 over the real corpus: every slice mints its vocabulary inside one of the
/// registered term namespaces, so purrdf's ownership analyzer can see every term
/// GMEOW owns. A slice minting elsewhere is invisible to the analyzer — it gets no
/// owner and no dependency edge into it is computable — which is precisely how the
/// `math` slice's entire vocabulary went unseen.
///
/// Guarded for non-vacuity two ways: the vocabulary of the corpus must be visible
/// through the shared vocab, and the `math:` namespace specifically must be owned
/// by it (the namespace whose absence caused the defect).
#[test]
fn every_slice_mints_into_a_registered_term_namespace() {
    let root = repo_root();
    let slices = root.join("slices");

    // Non-vacuity 1: the shared vocab really declares all four namespaces, so a
    // green result below is a real check and not a check against an empty set.
    let vocab = gmeow_ns::gmeow_slice_vocab();
    for ns in gmeow_ns::TERM_NAMESPACES {
        assert!(
            vocab.term_namespaces().contains(ns),
            "{ns} must be an owned term namespace"
        );
    }
    assert!(vocab.owns_term("https://blackcatinformatics.ca/math/Quantity"));

    // Non-vacuity 2: the walk finds real authored files.
    let catalog = purrdf::slice::SliceCatalog::discover(&slices, gmeow_ns::gmeow_slice_vocab())
        .expect("discover the real slice catalog");
    assert!(
        catalog.records().len() > 50,
        "implausibly few slices discovered ({}) — the walk is vacuous",
        catalog.records().len()
    );

    let findings = authoring_integrity::registered_minting_namespace_findings(&slices)
        .expect("registered minting namespaces");
    assert!(
        findings.is_empty(),
        "slices mint vocabulary terms outside the registered term namespaces \
         ({:?}) — such terms are invisible to the ownership analyzer:\n{}",
        gmeow_ns::TERM_NAMESPACES,
        joined(&findings)
    );
}

#[test]
fn slice_source_localizable_literals_are_language_tagged() {
    let root = repo_root();
    let findings =
        authoring_integrity::slice_source_untagged_findings(&root).expect("slice source langtag");
    assert!(
        findings.is_empty(),
        "untagged localizable literals in slice source:\n{}",
        joined(&findings)
    );
}

#[test]
fn nonslice_authored_localizable_literals_are_language_tagged() {
    let root = repo_root();
    let findings = authoring_integrity::nonslice_authored_untagged_findings(&root)
        .expect("non-slice source langtag");
    assert!(
        findings.is_empty(),
        "untagged localizable literals in non-slice authored source:\n{}",
        joined(&findings)
    );
}

/// The live-fold proof: `authoring_integrity_findings` is the SINGLE aggregator
/// `validate_all` folds onto its run ledger (see `validate_all.rs`, the
/// `validate.authoring_integrity` stage). Driving it here over the real corpus and
/// over a planted-bad slice tree proves the fold both (a) runs clean on the
/// production inputs and (b) FIRES on a real loader defect — not a test-only
/// demonstration and not dead code.
#[test]
fn live_aggregator_is_clean_on_the_real_corpus() {
    let root = repo_root();
    let findings = authoring_integrity::authoring_integrity_findings(&root, &root.join("slices"))
        .expect("aggregator runs on the real repo");
    let errors: Vec<&str> = findings
        .iter()
        .filter(|f| f.severity == gmeow_errors::Severity::Error)
        .map(|f| f.message.as_str())
        .collect();
    assert!(
        errors.is_empty(),
        "the folded authoring gate must be clean on the committed corpus:\n{}",
        errors.join("\n")
    );
}

#[test]
fn live_aggregator_fires_on_a_planted_loader_defect() {
    let root = repo_root();
    let tmp = tempfile::tempdir().expect("temp slices dir");
    let slices = tmp.path();

    // A slice manifest with NO gmeow:sliceTier (the missing-tier loader hole) and a
    // second dir redeclaring an existing slice IRI (the duplicate-IRI hole).
    let bad_dir = slices.join("extensions/bad");
    std::fs::create_dir_all(&bad_dir).unwrap();
    std::fs::write(
        bad_dir.join("manifest.ttl"),
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         <https://blackcatinformatics.ca/gmeow/slices/bad> a gmeow:Slice ;\n\
           rdfs:label \"bad\"@x-gmeow-english .\n",
    )
    .unwrap();

    // The aggregator scans project_root=real (clean) and slices_dir=the bad tree.
    let findings = authoring_integrity::authoring_integrity_findings(&root, slices)
        .expect("aggregator runs with a bad slices dir");
    assert!(
        findings
            .iter()
            .any(|f| f.code == "slice-discipline.missing-tier" && f.message.contains("slices/bad")),
        "the folded slice-discipline gate must fire missing-tier on the live path; got: {:?}",
        findings.iter().map(|f| &f.code).collect::<Vec<_>>()
    );
}

#[test]
fn docs_examples_use_only_allowlisted_terms() {
    let root = repo_root();
    // Non-vacuity on the EXTRACTED docs-term set: a broken fence/inline regex that
    // yielded nothing would make "no unallowlisted terms" trivially true.
    let extracted = authoring_integrity::docs_gmeow_terms(&root).expect("docs term extraction");
    assert!(
        !extracted.is_empty(),
        "no gmeow: terms extracted from docs/*.md — the extractor is vacuous"
    );

    let findings = authoring_integrity::docs_undeclared_findings(&root).expect("docs term check");
    assert!(
        findings.is_empty(),
        "docs examples reference unallowlisted GMEOW terms:\n{}",
        joined(&findings)
    );
}

// ── `ValidationRun::run`-level fold proof ────────────────────────────────────
//
// Every test above drives `authoring_integrity::authoring_integrity_findings`
// DIRECTLY, bypassing `crates/validate/src/validate_all.rs` entirely. That
// leaves the LIVE wiring uncovered: the Phase 5b marker guard
// (`project_root.join("slices").is_dir() && project_root.join("shapes").is_dir()`),
// the `slices_path` derivation, the `Severity::Error` filter, and the
// `intern_finding(&mut run_ledger, StageId::new("validate.authoring_integrity"),
// Standpoint::Binding, &finding)` call — none of that runs unless a test drives
// the real entry point, `ValidationRun::run`. A refactor that flipped the guard
// or typo'd a marker path would leave every test above green while silently
// darkening the live `make validate` / `gmeow-dev validate` gate (see
// `crates/gmeow-dev-cli/src/dev_validate.rs`, which sets
// `ValidateOptions { project_root: Some(root), .. }` and leaves `slices_dir:
// None` — exactly the shape driven here).
//
// The tests below build a CHEAP fixture farm instead of copying the whole
// corpus: every top-level entry of the real repo except `slices/` is a single
// symlink (`farm/<entry> -> repo_root()/<entry>`), and `slices/` is mirrored
// with REAL directories + per-FILE symlinks (see
// `mirror_dirs_symlink_files`). Every recursive corpus walker in
// `authoring_integrity.rs` (`all_manifests`, `slice_module_files`, `ttl_recursive`,
// …) rejects a DIRECTORY that is itself a symlink from recursion
// (`path.is_dir() && !path.is_symlink()`), but never applies that check to a
// FILE (the filename/extension branch matches regardless of symlink status) —
// so a file-level mirror is both a faithful, non-vacuous copy of the real
// corpus AND a real, writable directory tree a planted defect can be dropped
// into.

/// Recursively mirror `src` into `dst`: every directory becomes a REAL
/// directory in `dst`, every file becomes an individual symlink to the real
/// file in `src`. See the module doc above for why this specific shape (real
/// dirs, symlinked files) gives full-fidelity discovery under this codebase's
/// `is_dir() && !is_symlink()` recursive walkers while still letting the
/// caller graft extra, non-symlinked content anywhere in the tree.
fn mirror_dirs_symlink_files(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst)
        .unwrap_or_else(|e| panic!("create mirrored directory {}: {e}", dst.display()));
    for entry in std::fs::read_dir(src).unwrap_or_else(|e| panic!("read {}: {e}", src.display())) {
        let entry = entry.expect("read directory entry");
        let path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if path.is_dir() && !path.is_symlink() {
            mirror_dirs_symlink_files(&path, &dst_path);
        } else if path.is_file() {
            std::os::unix::fs::symlink(&path, &dst_path).unwrap_or_else(|e| {
                panic!("symlink {} -> {}: {e}", dst_path.display(), path.display())
            });
        }
    }
}

/// The relative manifest path of the planted missing-tier slice, rooted at
/// `farm/slices` — used both to author the fixture and to assert the finding
/// message names it.
const PLANTED_SLICE_MANIFEST_REL: &str = "zzz-planted/bad/manifest.ttl";
/// The slice IRI declared by the planted manifest — deliberately outside the
/// real `.../gmeow/slices/…` catalog so it cannot collide with a real slice.
const PLANTED_SLICE_IRI: &str = "https://blackcatinformatics.ca/gmeow/slices/zzz-planted-bad";

/// Build the fixture farm described above and graft one planted defect: a
/// `gmeow:Slice` manifest with NO `gmeow:sliceTier` — the missing-tier loader
/// hole `slice-discipline.missing-tier` detects (the same defect shape
/// `live_aggregator_fires_on_a_planted_loader_defect` plants, here reached
/// through the live `project_root`-only marker guard instead of a direct call).
fn build_planted_defect_farm() -> tempfile::TempDir {
    let root = repo_root();
    let farm = tempfile::tempdir().expect("farm tempdir");

    for entry in std::fs::read_dir(&root).expect("read repo root") {
        let entry = entry.expect("read repo root entry");
        let name = entry.file_name();
        if name == "slices" {
            continue;
        }
        std::os::unix::fs::symlink(root.join(&name), farm.path().join(&name))
            .unwrap_or_else(|e| panic!("symlink top-level {name:?}: {e}"));
    }

    mirror_dirs_symlink_files(&root.join("slices"), &farm.path().join("slices"));

    let bad_dir = farm.path().join("slices/zzz-planted/bad");
    std::fs::create_dir_all(&bad_dir).expect("create planted slice dir");
    std::fs::write(
        bad_dir.join("manifest.ttl"),
        format!(
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             <{PLANTED_SLICE_IRI}> a gmeow:Slice ;\n\
               rdfs:label \"zzz planted bad\"@x-gmeow-english .\n"
        ),
    )
    .expect("write planted manifest");

    farm
}

const RUN_NS: &str = "https://blackcatinformatics.ca/gmeow/";

/// The same minimal `ValidationRun::run` harness `crates/validate/tests/validate_all.rs`
/// uses throughout — duplicated locally (as `cache.rs` also does) because each
/// `tests/*.rs` file is its own compiled integration-test binary.
fn run_lint_config() -> LintConfig {
    LintConfig {
        namespace: RUN_NS.to_owned(),
        ontology_iri: RUN_NS.trim_end_matches('/').to_owned(),
        selector_tokens: BTreeSet::new(),
        core_slice_iris: HashSet::new(),
        annotation_predicates: [
            "http://www.w3.org/2000/01/rdf-schema#label",
            "http://www.w3.org/2004/02/skos/core#definition",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
    }
}

/// A SHACL shape corpus that always conforms (no constraints) — the fold under
/// test (`options.project_root`) is entirely independent of `shapes_ttl`/
/// `source_paths` content, so kept minimal on purpose (mirrors the existing
/// `validate_all.rs`/`cache.rs` harness, which never drives `ValidationRun::run`
/// with the whole shape corpus either).
fn run_empty_shapes_ttl() -> String {
    "@prefix sh: <http://www.w3.org/ns/shacl#> .\n".to_owned()
}

/// Write one minimal, well-formed source Turtle file so `ValidationRun::run`'s
/// syntax/sameAs pre-gates pass and the run reaches Phase 5b/5c. Its content is
/// irrelevant to the fold under test.
fn write_run_probe_source(name: &str) -> PathBuf {
    let ttl = format!(
        "@prefix gmeow: <{RUN_NS}> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
         gmeow:AuthoringIntegrityRunProbe a owl:Class ;\n\
           rdfs:label \"probe\"@en ;\n\
           skos:definition \"A probe class for the run-level authoring-integrity test.\"@en ;\n\
           rdfs:isDefinedBy <{RUN_NS}> .\n"
    );
    let path = std::env::temp_dir().join(format!(
        "gmeow_authoring_integrity_{name}_{}.ttl",
        std::process::id()
    ));
    std::fs::write(&path, &ttl).expect("write probe source");
    path
}

/// `ValidationRun::run`-level proof that Phase 5b (`validate.authoring_integrity`)
/// fires on a planted loader defect through the REAL production entry point —
/// not merely when `authoring_integrity_findings` is called directly.
/// `ValidateOptions { project_root: Some(farm), slices_dir: None, .. }` mirrors
/// `dev_validate.rs`'s live shape exactly, so this proves the marker guard, the
/// `slices_path` derivation, the `Severity::Error` filter, and the
/// `StageId::new("validate.authoring_integrity")` intern call all run on the
/// live path.
#[test]
fn run_level_authoring_integrity_fold_fires_on_a_planted_defect() {
    let farm = build_planted_defect_farm();
    let probe = write_run_probe_source("planted");

    let options = ValidateOptions {
        project_root: Some(farm.path().to_path_buf()),
        ..ValidateOptions::default()
    };

    let run = ValidationRun::run(
        &[probe.to_string_lossy().to_string()],
        &run_empty_shapes_ttl(),
        "",
        "",
        &run_lint_config(),
        &options,
    )
    .expect("orchestration must complete over the planted-defect farm");

    std::fs::remove_file(&probe).ok();

    let missing_tier_errors: Vec<&gmeow_errors::Finding> = run
        .report
        .findings
        .iter()
        .filter(|f| {
            f.code == codes::SLICE_DISCIPLINE_MISSING_TIER
                && f.severity == gmeow_errors::Severity::Error
        })
        .collect();
    assert!(
        !missing_tier_errors.is_empty(),
        "ValidationRun::run must fold a slice-discipline.missing-tier Error via \
         Phase 5b when project_root names the planted-defect farm; got findings: {:?}",
        run.report
            .findings
            .iter()
            .map(|f| (&f.code, &f.message))
            .collect::<Vec<_>>()
    );
    assert!(
        missing_tier_errors
            .iter()
            .any(|f| f.message.contains(PLANTED_SLICE_MANIFEST_REL)),
        "the missing-tier finding must name the planted manifest {PLANTED_SLICE_MANIFEST_REL}; \
         got: {:?}",
        missing_tier_errors
            .iter()
            .map(|f| &f.message)
            .collect::<Vec<_>>()
    );
}

/// The companion clean-run proof: `project_root` = the REAL repository root (no
/// planted defect, no farm) must fold ZERO authoring-integrity Errors through
/// `ValidationRun::run` — the guard fires on real markers without a false
/// positive. Also confirms Phase 5c (`validate.ownership` + example-coverage,
/// gated on the SAME `project_root`-only markers) actually ran, via its
/// `slice-ownership-live` / `example-coverage-live` timing phases — the only
/// externally observable signal `ValidationRun`'s public surface exposes for
/// "which phase ran" (`Report`/`Finding` do not carry back the interning
/// `StageId`).
#[test]
fn run_level_authoring_integrity_fold_is_clean_on_the_real_corpus() {
    let root = repo_root();
    let probe = write_run_probe_source("clean");

    let options = ValidateOptions {
        project_root: Some(root),
        timings: true,
        ..ValidateOptions::default()
    };

    let run = ValidationRun::run(
        &[probe.to_string_lossy().to_string()],
        &run_empty_shapes_ttl(),
        "",
        "",
        &run_lint_config(),
        &options,
    )
    .expect("orchestration must complete over the real repository root");

    std::fs::remove_file(&probe).ok();

    // NOTE on why this is NOT a blanket "zero Severity::Error over the whole
    // report" assertion: it was tried and observed NOT robust. `probe` (the
    // lone dataset-driving source this harness loads) is a bare `owl:Class`
    // with an `@en`-tagged `rdfs:label`/`skos:definition` and no
    // `gmeow:graphBoxRole`/stereotype pun — by design, its content is
    // irrelevant to the fold under test (see `write_run_probe_source`'s doc
    // comment). That trips REAL, unrelated `validate.lint.*` and generic
    // `validate.error` (missing-stereotype) findings that have nothing to do
    // with the committed corpus, so a total-zero assertion would be
    // permanently red on an intentionally-minimal fixture, not a signal of a
    // real regression. Evidence: `cargo nextest run -p gmeow-validate --test
    // authoring_integrity run_level_authoring_integrity_fold_is_clean_on_the_real_corpus`
    // with a total-zero assertion FAILED on the clean corpus with exactly
    // those probe-fixture findings and nothing corpus-related.
    //
    // So instead: keep the existing authoring/slice-discipline family-prefix
    // assertion (now also covering `slice-ownership.*`, Phase 5c's OTHER
    // structured-code gate), AND add an explicit, code-independent check for
    // the specific mask the completion-adversary found — the example-coverage
    // gate interns its errors under the GENERIC `validate.error` code with a
    // "no examples/*.ttl" message (see `check_example_coverage` in
    // `validate_all.rs`), which no code-prefix filter can catch. Together
    // these two assertions track every Phase 5b/5c gate-fatal regression this
    // harness can produce, without being defeated by the probe fixture's own,
    // deliberately-incomplete content.
    let authoring_family_errors: Vec<&gmeow_errors::Finding> = run
        .report
        .findings
        .iter()
        .filter(|f| {
            f.severity == gmeow_errors::Severity::Error
                && (f.code.starts_with(codes::AUTHORING_FAMILY)
                    || f.code.starts_with(codes::SLICE_DISCIPLINE_FAMILY)
                    || f.code.starts_with(codes::SLICE_OWNERSHIP_FAMILY))
        })
        .collect();
    assert!(
        authoring_family_errors.is_empty(),
        "the folded authoring-integrity/slice-ownership gates must be clean on the \
         committed corpus via ValidationRun::run:\n{}",
        authoring_family_errors
            .iter()
            .map(|f| format!("{}: {}", f.code, f.message))
            .collect::<Vec<_>>()
            .join("\n")
    );

    // Coverage-tracking assertion: the example-coverage gate (Phase 5c) has NO
    // dedicated code — it folds through the generic-code `intern_phase` path
    // (code `validate.error`) alongside unrelated cheap-phase errors, so it is
    // identified by its message shape, not its code. This is exactly the mask
    // the completion-adversary found: a missing `examples/*.ttl` regression
    // would fail live `make validate` while the family-prefix assertion above
    // stayed green. Falsified below by temporarily removing
    // `slices/profile/agent-runtime/examples/agent-runtime.ttl`.
    let coverage_errors: Vec<&gmeow_errors::Finding> = run
        .report
        .findings
        .iter()
        .filter(|f| {
            f.severity == gmeow_errors::Severity::Error && f.message.contains("no examples/*.ttl")
        })
        .collect();
    assert!(
        coverage_errors.is_empty(),
        "the example-coverage gate (Phase 5c, generic `validate.error` code) must be \
         clean on the committed corpus via ValidationRun::run — every slice must ship \
         at least one examples/*.ttl:\n{}",
        coverage_errors
            .iter()
            .map(|f| format!("{}: {}", f.code, f.message))
            .collect::<Vec<_>>()
            .join("\n")
    );

    // Phase 5c coverage: `validate.ownership` + example-coverage gate on the same
    // `project_root`-only markers as Phase 5b (see validate_all.rs's "Phase 5c"
    // comment). Their `timed(...)` phases are the only externally visible proof
    // they ran through this entry point.
    let phase_names: Vec<&str> = run.timings.iter().map(|t| t.phase.as_str()).collect();
    assert!(
        phase_names.contains(&"slice-ownership-live"),
        "Phase 5c slice-ownership-live must run when project_root names the real repo \
         root and slices_dir is None; got timing phases: {phase_names:?}"
    );
    assert!(
        phase_names.contains(&"example-coverage-live"),
        "Phase 5c example-coverage-live must run when project_root names the real repo \
         root and slices_dir is None; got timing phases: {phase_names:?}"
    );
}
