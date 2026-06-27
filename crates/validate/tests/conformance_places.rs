// SPDX-License-Identifier: AGPL-3.0-only
//! Conformance tests for places slice — migrated from tests/test_places.py
//! Dynamic sweep, RCC-8 disjoint, bnode-union-walk, cross-slice tests retained in Python.
//!
//! Migrated: pure SHACL ok-pass checks that call run_shacl(g) with no
//! post-SHACL graph queries, no membership checks, no mutations.
//!
//! Retained in Python (not migrated):
//!   - All tests using _graph() / load_merged_graph (TBox checks, cross-slice)
//!   - All tests with post-SHACL dynamic sweep: g.subjects(), g.objects(), .triples()
//!   - All tests with (triple) in g membership checks
//!   - All tests with g.remove() / g.add() mutations (geocode invalid cases)
//!   - test_biological_standpoint_coordinate_claims_coexist: g.objects() sweep
//!   - test_coordinate_observations_coexist / test_superseded_*: membership checks
//!   - test_land_tenure_instance_structure / test_cadastral_reference_*: g.subjects()

mod conformance_support;
use conformance_support::*;
use rstest::rstest;

#[rstest]
#[case::biological_coverage_passes_shacl(Case::file("coverage", "places-biological"))]
#[case::geocode_shape_valid(Case::file("coverage", "places-geocode"))]
#[case::cadastral_coverage_passes_shacl(Case::file("coverage", "places-cadastral"))]
fn places(#[case] case: Case) {
    case.run();
}
