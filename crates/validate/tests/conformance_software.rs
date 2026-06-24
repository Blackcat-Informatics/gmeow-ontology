// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_software.py (#867)
//!
//! Migrated tests (use `run_shacl`):
//!   - `test_facet_orthogonality_shacl_rejects_two_facets`: inline dual-facet
//!     graph must be rejected by SHACL.
//!   - `test_fixture_parses_and_shacl_passes`: the `tests/fixtures/software.ttl`
//!     fixture must pass SHACL.
//!
//! Retained in Python (not migrated):
//!   - `test_no_subclass_bridge_between_facets`: uses `_graph()` + `combinations`
//!     over the live merged ontology; requires dynamic class-graph traversal.
//!   - All `test_fixture_has_*` tests: pure `(triple) in g` membership checks on
//!     the rdflib Graph — no SHACL call, not portable to this harness.
//!   - `test_fixture_ai_contributor_is_first_class`: uses `g.subjects()` iteration.
//!   - `test_fixture_contribution_reifies_role_and_degree`: pure triple membership.
//!   - `test_software_contribution_roles_seeded`: dynamic `_graph()` sweep.
//!   - `test_software_event_types_seeded`: dynamic `_graph()` sweep.
//!   - `test_fixture_repository_has_materialization_depth`: uses `g.objects()`.

mod conformance_support;
use conformance_support::*;

/// Turtle prefix block shared by all software tests.
const PREFIXES: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/software/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
";

/// `test_facet_orthogonality_shacl_rejects_two_facets` — an individual typed in
/// two software facet classes must be rejected by SHACL (no reasoner needed).
/// The shape enforces "may fill at most one software facet".
#[test]
fn facet_orthogonality_shacl_rejects_two_facets() {
    let ttl = format!(
        "{PREFIXES}\
ex:x rdf:type gmeow:Project .
ex:x rdf:type gmeow:SoftwareProduct .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        !ok(&report),
        "dual-facet individual must be rejected by SHACL; got no violations"
    );
    let msgs = violations(&report);
    assert!(
        msgs.iter()
            .any(|m| m.contains("may fill at most one software facet")),
        "expected 'may fill at most one software facet' in violations; got: {msgs:?}"
    );
}

/// `test_fixture_parses_and_shacl_passes` — the canonical software fixture
/// `tests/fixtures/software.ttl` must parse and pass all SHACL shapes.
#[test]
fn fixture_parses_and_shacl_passes() {
    let root = repo_root();
    let path = root.join("tests").join("fixtures").join("software.ttl");
    let nt = ttl_file_to_nt(&path);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "software.ttl fixture must pass SHACL; violations: {:?}",
        violations(&report)
    );
}
