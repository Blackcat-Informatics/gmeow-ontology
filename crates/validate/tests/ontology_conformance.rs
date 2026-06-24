// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Whole-ontology native SHACL conformance harness (#867).
//!
//! Migrated from Python `run_shacl` pytest functions:
//!
//! - `tests/test_shapes.py`:
//!   `test_wellformed_relator_fixture_conforms`,
//!   `test_malformed_relator_fixture_is_flagged`,
//!   `test_suppression_warning_does_not_fail_validation`,
//!   `test_orthogonality_data_check_rejects_two_axes`,
//!   `test_wellformed_facet_cardinality_passes`,
//!   `test_internal_language_tag_shape_is_case_insensitive`,
//!   `test_wellformed_reference_frame_passes`,
//!   `test_reference_frame_axis_count_must_match_dimension_count`,
//!   `test_malformed_reference_frame_fails`,
//!   `test_profile_open_value_guard_warns_on_orphan`,
//!   `test_wellformed_proximity_fixture_conforms`,
//!   `test_malformed_proximity_fixture_is_flagged`,
//!   `test_wellformed_expertise_fixture_conforms`,
//!   `test_malformed_expertise_fixture_is_flagged`.
//!
//! - `tests/test_attestation.py`:
//!   `test_contested_attestation_coexists`,
//!   `test_all_fixture_files_load`.
//!
//! - `tests/test_coreference.py`:
//!   `test_authority_link_without_match_strength_warns_only`.
//!
//! Each test calls [`validate`] against the real merged shapes corpus — the
//! same corpus that `make validate` uses — so regressions in shape authoring
//! are caught at Rust compile+test speed, not after Python import.

use std::path::{Path, PathBuf};

use gmeow_shacl::engine::validate_graphs;
use gmeow_shacl::report::{Severity, ValidationReport};
use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::store::Store;

// ── Repo-root resolution ──────────────────────────────────────────────────────

/// Absolute path to the repository root (`crates/validate/../../`).
///
/// `CARGO_MANIFEST_DIR` is the `crates/validate` directory. Walking up two
/// levels yields the repository root that contains `shapes/`, `generated/`, and
/// `slices/`.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..") // crates/
        .join("..") // repo root
        .canonicalize()
        .expect("repo root must be resolvable")
}

// ── Shapes corpus assembly ────────────────────────────────────────────────────

/// DSL-specific shapes files excluded from the domain test corpus.
///
/// Mirrors Python's `dsl_shapes` exclusion set in `gmeow_tools.validate._shapes_turtle`.
const DSL_SHAPE_FILENAMES: &[&str] = &[
    "mapping-dsl-shapes.ttl",
    "statement-dsl-shapes.ttl",
    "test-dsl-shapes.ttl",
    "slice-manifest-shapes.ttl",
];

/// Collect `shapes/*.ttl` paths, sorted, excluding DSL-specific files.
fn collect_shapes_dir(root: &Path) -> Vec<PathBuf> {
    let dir = root.join("shapes");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read shapes/: {e}"))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension().and_then(|s| s.to_str()) == Some("ttl")
                && !DSL_SHAPE_FILENAMES
                    .iter()
                    .any(|x| p.file_name().and_then(|n| n.to_str()) == Some(x))
        })
        .collect();
    paths.sort();
    paths
}

/// Collect `generated/shapes/*.ttl` paths, sorted.
///
/// Hard-fails if the directory is absent or empty — the generated frame shapes
/// are load-bearing for Principle 11 enforcement (same contract as Python).
fn collect_generated_shapes(root: &Path) -> Vec<PathBuf> {
    let dir = root.join("generated").join("shapes");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| {
            panic!(
                "no generated shapes under generated/shapes/ — \
                 run `gmeow regenerate frame-shapes` (P11 enforcement lives there): {e}"
            )
        })
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("ttl"))
        .collect();
    assert!(
        !paths.is_empty(),
        "no generated shapes under generated/shapes/ — \
         run `gmeow regenerate frame-shapes` (P11 enforcement lives there)"
    );
    paths.sort();
    paths
}

/// Collect per-slice `shapes.ttl` files from `slices/`, sorted.
///
/// Mirrors Python's `iter_slice_shape_files()`.
fn collect_slice_shapes(root: &Path) -> Vec<PathBuf> {
    let slices_dir = root.join("slices");
    let mut paths: Vec<PathBuf> = Vec::new();
    collect_slice_shapes_recursive(&slices_dir, &mut paths);
    paths.sort();
    paths
}

fn collect_slice_shapes_recursive(dir: &Path, paths: &mut Vec<PathBuf>) {
    let read = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return,
    };
    for entry in read.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() && !path.is_symlink() {
            let candidate = path.join("shapes.ttl");
            if candidate.is_file() {
                paths.push(candidate);
            }
            collect_slice_shapes_recursive(&path, paths);
        }
    }
}

/// Read a Turtle file as raw UTF-8 text.
fn read_ttl(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

/// Assemble the full SHACL shapes corpus as one concatenated Turtle string.
///
/// Replicates `gmeow_tools.validate._shapes_turtle(SHAPES_FILE)`:
///   1. `shapes/gmeow-shapes.ttl` (the base shapes file, listed first),
///   2. other `shapes/*.ttl` excluding DSL-specific files,
///   3. `generated/shapes/*.ttl` (frame-relativity shapes, Principle 11),
///   4. per-slice `shapes.ttl` files.
///
/// Cached via [`std::sync::OnceLock`] so the disk I/O happens at most once per
/// test process even when many tests run in parallel.
fn whole_shapes_ttl() -> &'static str {
    use std::sync::OnceLock;
    static CACHE: OnceLock<String> = OnceLock::new();
    CACHE.get_or_init(|| {
        let root = repo_root();
        let mut parts: Vec<String> = Vec::new();

        // 1. Base shapes file first.
        parts.push(read_ttl(&root.join("shapes").join("gmeow-shapes.ttl")));

        // 2. Additional domain shapes (excludes gmeow-shapes.ttl — already added —
        //    and DSL files).
        let base_name = "gmeow-shapes.ttl";
        for path in collect_shapes_dir(&root) {
            if path.file_name().and_then(|n| n.to_str()) != Some(base_name) {
                parts.push(read_ttl(&path));
            }
        }

        // 3. Generated shapes.
        for path in collect_generated_shapes(&root) {
            parts.push(read_ttl(&path));
        }

        // 4. Per-slice shapes.
        for path in collect_slice_shapes(&root) {
            parts.push(read_ttl(&path));
        }

        parts.join("\n")
    })
}

// ── Fixture helpers ───────────────────────────────────────────────────────────

/// Parse a fixture `.ttl` file into an in-memory oxigraph store and emit as
/// N-Triples text, which the SHACL engine accepts as data input.
///
/// `subdir` is relative to `tests/fixtures/` (e.g. `"shapes"` or `"coverage"`).
fn fixture_as_nt(subdir: &str, name: &str) -> String {
    let root = repo_root();
    // The pytest test directory is `tests/` at the repo root.
    let path = root
        .join("tests")
        .join("fixtures")
        .join(subdir)
        .join(format!("{name}.ttl"));
    ttl_file_to_nt(&path)
}

/// Read a Turtle file at `path`, load it into an oxigraph store, and emit as
/// N-Triples text.
fn ttl_file_to_nt(path: &Path) -> String {
    let ttl = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    ttl_str_to_nt(&ttl)
}

/// Parse an inline Turtle string into an oxigraph store and emit as N-Triples.
///
/// Uses the lenient parser (same as `gmeow_shacl::engine::validate_graphs`) so
/// private-use `@x-gmeow-*` language tags are accepted.
fn ttl_str_to_nt(ttl: &str) -> String {
    let store = Store::new().expect("in-memory store creation is infallible");
    store
        .load_from_reader(
            RdfParser::from_format(RdfFormat::Turtle).lenient(),
            ttl.as_bytes(),
        )
        .unwrap_or_else(|e| panic!("Turtle parse failed: {e}\nInput:\n{ttl}"));
    let mut buf: Vec<u8> = Vec::new();
    store
        .dump_graph_to_writer(
            oxigraph::model::GraphNameRef::DefaultGraph,
            RdfFormat::NTriples,
            &mut buf,
        )
        .expect("N-Triples serialisation is infallible");
    String::from_utf8(buf).expect("oxigraph N-Triples output is valid UTF-8")
}

// ── Report helpers ────────────────────────────────────────────────────────────

/// Collect human-readable messages for results at `Violation` severity.
///
/// Mirrors Python's `result.errors` from `ValidationResult.errors`.
fn violations(report: &ValidationReport) -> Vec<String> {
    report
        .results
        .iter()
        .filter(|r| r.severity == Severity::Violation)
        .map(|r| r.message.clone().unwrap_or_default())
        .collect()
}

/// Collect human-readable messages for results at `Warning` severity.
///
/// Mirrors Python's `result.warnings` from `ValidationResult.warnings`.
fn warnings(report: &ValidationReport) -> Vec<String> {
    report
        .results
        .iter()
        .filter(|r| r.severity == Severity::Warning)
        .map(|r| r.message.clone().unwrap_or_default())
        .collect()
}

/// Return `true` when there are no `Violation`-severity results.
///
/// Mirrors Python's `result.ok` which is `not result.errors`.  A graph with
/// only `Warning`-severity results is "ok" in the Python sense: `run_shacl`
/// routes `sh:Warning` / `sh:Info` results to `result.warnings` and leaves
/// `result.errors` empty, so `result.ok` is `True`. SHACL's own `conforms`
/// field is `False` whenever any result (including warnings) is present, so
/// we cannot use `report.conforms` for this check.
fn ok(report: &ValidationReport) -> bool {
    violations(report).is_empty()
}

/// Run validation of `data_nt` (N-Triples) against the whole shapes corpus.
fn validate(data_nt: &str) -> ValidationReport {
    validate_graphs(data_nt, whole_shapes_ttl()).expect("validate_graphs must not error")
}

// ── Tests migrated from tests/test_shapes.py ─────────────────────────────────

/// `test_wellformed_relator_fixture_conforms` — a well-formed data graph passes
/// every closed-world shape (AC#1 positive, #39).
#[test]
fn wellformed_relator_fixture_conforms() {
    let nt = fixture_as_nt("shapes", "relator-wellformed");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "relator-wellformed.ttl must pass all shapes; violations: {:?}",
        violations(&report)
    );
}

/// `test_malformed_relator_fixture_is_flagged` — a malformed data graph is
/// rejected, and each shape names its violation (AC#1 negative, #39).
#[test]
fn malformed_relator_fixture_is_flagged() {
    let nt = fixture_as_nt("shapes", "relator-malformed");
    let report = validate(&nt);
    assert!(!report.conforms, "relator-malformed.ttl must fail SHACL");
    let all_msgs = violations(&report).join("\n");
    assert!(
        all_msgs.contains("exactly one gmeow:Gender value"),
        "must flag GenderIdentity cardinality; got: {all_msgs}"
    );
    assert!(
        all_msgs.contains("must use exactly one appellation"),
        "must flag appellation cardinality; got: {all_msgs}"
    );
    assert!(
        all_msgs.contains("may fill at most one identity axis"),
        "must flag identity-axis orthogonality (P9); got: {all_msgs}"
    );
    // Suppression is a Warning, not a Violation.
    let warn_msgs = warnings(&report).join("\n");
    assert!(
        warn_msgs.contains("should set gmeow:displayable false"),
        "suppression warning must appear; got: {warn_msgs}"
    );
}

/// `test_suppression_warning_does_not_fail_validation` — a superseded-but-unsuppressed
/// facet warns but does NOT hard-fail (`result.ok`, Principle 10).
///
/// Python's `result.ok` is `not result.errors`, which is `true` when only
/// `sh:Warning`-severity results are present.  The Rust equivalent is
/// `ok(&report)` (no Violation results); `report.conforms` would be `false`
/// here because SHACL's `conforms` field is `false` whenever any result exists.
#[test]
fn suppression_warning_does_not_fail_validation() {
    let nt = fixture_as_nt("shapes", "suppression-warning-only");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "warning-only graph must pass (no violations); violations: {:?}",
        violations(&report)
    );
    let warn_msgs = warnings(&report).join("\n");
    assert!(
        warn_msgs.contains("should set gmeow:displayable false"),
        "suppression warning must be present; got: {warn_msgs}"
    );
}

/// `test_orthogonality_data_check_rejects_two_axes` — the closed-world dual
/// of HermiT's two-axis inconsistency check: a node typed in two disjoint
/// identity axes is caught by SHACL without a reasoner.
#[test]
fn orthogonality_data_check_rejects_two_axes() {
    let data_ttl = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test/> .
ex:x a gmeow:GenderIdentity .
ex:x a gmeow:SexualOrientation .
";
    let nt = ttl_str_to_nt(data_ttl);
    let report = validate(&nt);
    assert!(!report.conforms, "dual-axis node must fail SHACL");
    let all_msgs = violations(&report).join("\n");
    assert!(
        all_msgs.contains("may fill at most one identity axis"),
        "orthogonality message must appear; got: {all_msgs}"
    );
}

/// `test_wellformed_facet_cardinality_passes` — a lone facet with exactly one
/// value conforms (cardinality-shape control case).
#[test]
fn wellformed_facet_cardinality_passes() {
    let data_ttl = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test/> .
ex:f a gmeow:GenderIdentity .
ex:f gmeow:facetSubject ex:person .
ex:f gmeow:facetVantage ex:person .
ex:f gmeow:genderValue gmeow:genderNonBinary .
";
    let nt = ttl_str_to_nt(data_ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "well-formed GenderIdentity facet must pass; violations: {:?}",
        violations(&report)
    );
}

/// `test_internal_language_tag_shape_is_case_insensitive` — BCP-47 private-use
/// tags are case-insensitive in SHACL too.
#[test]
fn internal_language_tag_shape_is_case_insensitive() {
    // N-Triples directly: a gmeow:fullName literal with an uppercase private-use
    // language tag. The SHACL `sh:languageIn` check must accept this.
    let nt = "<https://example.org/test/name> \
               <https://blackcatinformatics.ca/gmeow/fullName> \
               \"Japanese\"@x-GMEOW-Japanese .\n";
    let report = validate(nt);
    assert!(
        ok(&report),
        "uppercase @x-GMEOW-Japanese tag must be accepted; violations: {:?}",
        violations(&report)
    );
}

/// `test_wellformed_reference_frame_passes` — a reference frame profile with all
/// required properties passes SHACL.
#[test]
fn wellformed_reference_frame_passes() {
    let data_ttl = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test/> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
ex:frame a gmeow:ReferenceFrame .
ex:frame gmeow:frameRealm gmeow:frameRealmTerrestrial .
ex:frame gmeow:hasAxis ex:axisX .
ex:frame gmeow:dimensionCount \"1\"^^xsd:nonNegativeInteger .
ex:frame gmeow:frameKind gmeow:frameKindCartesian .
ex:frame gmeow:requiresHost \"false\"^^<http://www.w3.org/2001/XMLSchema#boolean> .
ex:frame gmeow:determinacyModel gmeow:determinacyCrisp .
gmeow:frameRealmTerrestrial a gmeow:FrameRealm .
ex:axisX a gmeow:Axis .
gmeow:frameKindCartesian a gmeow:FrameKind .
gmeow:determinacyCrisp a gmeow:Determinacy .
";
    let nt = ttl_str_to_nt(data_ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "well-formed ReferenceFrame must pass; violations: {:?}",
        violations(&report)
    );
}

/// `test_reference_frame_axis_count_must_match_dimension_count` — frame profiles
/// reject mismatched axis cardinality and dimension count.
#[test]
fn reference_frame_axis_count_must_match_dimension_count() {
    // One axis but dimensionCount = 3.
    let data_ttl = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test/> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
ex:frame a gmeow:ReferenceFrame .
ex:frame gmeow:frameRealm gmeow:frameRealmTerrestrial .
ex:frame gmeow:hasAxis ex:axisX .
ex:frame gmeow:dimensionCount \"3\"^^xsd:nonNegativeInteger .
ex:frame gmeow:frameKind gmeow:frameKindCartesian .
ex:frame gmeow:requiresHost \"false\"^^<http://www.w3.org/2001/XMLSchema#boolean> .
ex:frame gmeow:determinacyModel gmeow:determinacyCrisp .
gmeow:frameRealmTerrestrial a gmeow:FrameRealm .
ex:axisX a gmeow:Axis .
gmeow:frameKindCartesian a gmeow:FrameKind .
gmeow:determinacyCrisp a gmeow:Determinacy .
";
    let nt = ttl_str_to_nt(data_ttl);
    let report = validate(&nt);
    assert!(!report.conforms, "axis/dimension mismatch must fail SHACL");
    let all_msgs = violations(&report).join("\n");
    assert!(
        all_msgs.contains("dimension count must equal"),
        "dimension-count mismatch message must appear; got: {all_msgs}"
    );
}

/// `test_malformed_reference_frame_fails` — a reference frame profile missing
/// required descriptors fails SHACL validation.
#[test]
fn malformed_reference_frame_fails() {
    // Bare ReferenceFrame with no required properties.
    let nt = "<https://example.org/test/frame> \
               <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
               <https://blackcatinformatics.ca/gmeow/ReferenceFrame> .\n";
    let report = validate(nt);
    assert!(!report.conforms, "bare ReferenceFrame must fail SHACL");
    let all_msgs = violations(&report).join("\n");
    assert!(
        all_msgs.contains("declare its frame realm"),
        "missing frameRealm message must appear; got: {all_msgs}"
    );
    assert!(
        all_msgs.contains("have at least one coordinate axis"),
        "missing hasAxis message must appear; got: {all_msgs}"
    );
}

/// `test_profile_open_value_guard_warns_on_orphan` — a novel open-value
/// individual with no profile descriptor triggers a warning but still conforms.
#[test]
fn profile_open_value_guard_warns_on_orphan() {
    let data_ttl = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test/> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix skos:  <http://www.w3.org/2004/02/skos/core#> .
gmeow:profileReferenceFrame a gmeow:Profile .
gmeow:profileReferenceFrame rdfs:label \"Reference Frame Profile\" .
gmeow:profileReferenceFrame skos:definition \"Closed descriptor schema for reference frames.\" .
gmeow:profileReferenceFrame gmeow:profileDescriptor gmeow:frameRealm .
gmeow:profileReferenceFrame gmeow:profileOpenValue gmeow:FrameRealm .
ex:customRealm a gmeow:FrameRealm .
";
    let nt = ttl_str_to_nt(data_ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "open-value orphan is Warning only — must not have violations; violations: {:?}",
        violations(&report)
    );
    let warn_msgs = warnings(&report).join("\n");
    assert!(
        warn_msgs.contains(
            "Open value individuals must be referenced by at least one profile descriptor"
        ),
        "open-value warning must appear; got: {warn_msgs}"
    );
}

/// `test_wellformed_proximity_fixture_conforms` — a well-formed
/// ProximityMeasurement passes every shape (AC#1 positive, #95).
#[test]
fn wellformed_proximity_fixture_conforms() {
    let nt = fixture_as_nt("shapes", "proximity-wellformed");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "proximity-wellformed.ttl must pass all shapes; violations: {:?}",
        violations(&report)
    );
}

/// `test_malformed_proximity_fixture_is_flagged` — a malformed
/// ProximityMeasurement is rejected by SHACL (#95).
#[test]
fn malformed_proximity_fixture_is_flagged() {
    let nt = fixture_as_nt("shapes", "proximity-malformed");
    let report = validate(&nt);
    assert!(!report.conforms, "proximity-malformed.ttl must fail SHACL");
    let all_msgs = (violations(&report).into_iter())
        .chain(warnings(&report))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        all_msgs.contains("exactly one starting entity (gmeow:observedFeature)"),
        "observedFeature cardinality message must appear; got: {all_msgs}"
    );
    assert!(
        all_msgs.contains("exactly one target entity (gmeow:proximityTo)"),
        "proximityTo cardinality message must appear; got: {all_msgs}"
    );
    assert!(
        all_msgs.contains("exactly one scalar quantity result"),
        "scalar quantity result message must appear; got: {all_msgs}"
    );
}

/// `test_wellformed_expertise_fixture_conforms` — a well-formed SkillProficiency
/// + Credential graph passes expertise shapes (#263).
#[test]
fn wellformed_expertise_fixture_conforms() {
    let nt = fixture_as_nt("shapes", "expertise-wellformed");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "expertise-wellformed.ttl must pass all shapes; violations: {:?}",
        violations(&report)
    );
}

/// `test_malformed_expertise_fixture_is_flagged` — a malformed expertise graph
/// is rejected by the SHACL shapes (#263).
#[test]
fn malformed_expertise_fixture_is_flagged() {
    let nt = fixture_as_nt("shapes", "expertise-malformed");
    let report = validate(&nt);
    assert!(!report.conforms, "expertise-malformed.ttl must fail SHACL");
    let all_msgs = (violations(&report).into_iter())
        .chain(warnings(&report))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        all_msgs.contains("must reference exactly one Skill"),
        "missing Skill reference message must appear; got: {all_msgs}"
    );
    assert!(
        all_msgs.contains("levelScale should match"),
        "levelScale mismatch message must appear; got: {all_msgs}"
    );
    assert!(
        all_msgs.contains("must be an Organization"),
        "Organization constraint message must appear; got: {all_msgs}"
    );
    assert!(
        all_msgs.contains("should reference a gmeow:Attestation"),
        "Attestation reference message must appear; got: {all_msgs}"
    );
}

// ── Tests migrated from tests/test_attestation.py ────────────────────────────

/// `test_contested_attestation_coexists` — a contested attestation: one
/// standpoint affirms, another refutes. Both claims load and SHACL-pass.
///
/// This is an ABox multi-file conformance check, not a TBox cell.
#[test]
fn contested_attestation_coexists() {
    let nt = fixture_as_nt("coverage", "attestation-vc");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "attestation-vc.ttl must pass all shapes; violations: {:?}",
        violations(&report)
    );
}

/// `test_all_fixture_files_load` — every scenario in the attestation coverage
/// fixture set loads and SHACL-passes.
#[test]
fn attestation_all_fixture_files_load() {
    let fixtures = [
        "attestation-software-release",
        "attestation-vc",
        "attestation-email-reuse",
        "attestation-quality-report",
        "attestation-blockchain-claim",
        "attestation-ledger-evidence",
    ];
    for name in fixtures {
        let nt = fixture_as_nt("coverage", name);
        let report = validate(&nt);
        assert!(
            ok(&report),
            "{name}.ttl failed SHACL; violations: {:?}",
            violations(&report)
        );
    }
}

// ── Tests migrated from tests/test_coreference.py ────────────────────────────

/// `test_authority_link_without_match_strength_warns_only` — a bare
/// `gmeow:authorityLink` (no `gmeow:matchStrength`) passes (conforms) but
/// emits a warning recommending the strength annotation.
#[test]
fn authority_link_without_match_strength_warns_only() {
    let nt = "\
<https://example.org/coref/entity> \
<https://blackcatinformatics.ca/gmeow/authorityLink> \
<https://example.org/coref/authority> .\n";
    let report = validate(nt);
    assert!(
        ok(&report),
        "bare authorityLink must still pass (Warning only); violations: {:?}",
        violations(&report)
    );
    let warn_msgs = warnings(&report).join("\n");
    assert!(
        warn_msgs.contains("authority link should also assert"),
        "missing-matchStrength warning must appear; got: {warn_msgs}"
    );
}
