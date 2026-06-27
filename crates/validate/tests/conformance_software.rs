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
use rstest::rstest;

/// Turtle prefix block shared by all software tests.
const PREFIXES: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/software/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
";

#[rstest]
#[case::facet_orthogonality_shacl_rejects_two_facets(
    Case::inline(format!(
        "{PREFIXES}\
ex:x rdf:type gmeow:Project .
ex:x rdf:type gmeow:SoftwareProduct .
"
    ))
        .fails()
        .violations(&["may fill at most one software facet"])
)]
#[case::fixture_parses_and_shacl_passes(Case::repo_path("tests/fixtures/software.ttl"))]
fn software(#[case] case: Case) {
    case.run();
}
