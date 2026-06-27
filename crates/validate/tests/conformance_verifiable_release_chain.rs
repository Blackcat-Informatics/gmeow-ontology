// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_verifiable_release_chain.py (#867).
//!
//! Only `test_fixture_loads_and_shacl_passes` was migratable: it is the sole
//! `run_shacl` call in the Python file, uses the fixture-only graph (no merged
//! ontology), and has no surrounding dynamic sweep, triple-membership checks,
//! SPARQL queries, or disk/crypto-pipeline coupling.
//!
//! All other tests in the Python file are retained there because they rely on
//! `load_merged_graph()` structural membership checks (`(triple) in g`) or
//! SPARQL competency queries over the combined graph — none of which are
//! `run_shacl` calls and none are portable to SHACL conformance tests.
//!
//! Retained in Python (not migrated):
//!   - `test_build_activity_is_activity`: `_graph()` triple-membership check.
//!   - `test_builder_is_software_agent`: `_graph()` triple-membership check.
//!   - `test_build_properties_exist`: `_graph()` triple-membership check.
//!   - `test_release_doi_property_exists`: `_graph()` triple-membership check.
//!   - `test_slsa_level_is_value_vocabulary`: `_graph()` triple-membership check.
//!   - `test_build_event_type_seeded`: `_graph()` triple-membership check.
//!   - `test_fixture_signed_commit`: `_fixture()` triple-membership check (no SHACL).
//!   - `test_fixture_signed_tag`: `_fixture()` triple-membership check (no SHACL).
//!   - `test_fixture_release_with_doi`: `_fixture()` triple-membership check (no SHACL).
//!   - `test_fixture_build_activity`: `_fixture()` triple-membership check (no SHACL).
//!   - `test_fixture_slsa_attestation`: `_fixture()` triple-membership check (no SHACL).
//!   - `test_fixture_cosign_signature`: `_fixture()` triple-membership check (no SHACL).
//!   - `test_fixture_rekor_entry`: `_fixture()` triple-membership check (no SHACL).
//!   - `test_fixture_swhid_on_commit`: `g.objects()` dynamic iteration (no SHACL).
//!   - `test_query_key_that_signed_commit`: SPARQL SELECT over `_combined()`.
//!   - `test_query_build_that_produced_artifact`: SPARQL SELECT over `_combined()`.
//!   - `test_query_rekor_entry_for_attestation`: SPARQL ASK over `_combined()`.

mod conformance_support;
use conformance_support::*;
use rstest::rstest;

#[rstest]
#[case::fixture_loads_and_shacl_passes(Case::repo_path(
    "tests/fixtures/verifiable-release-chain.ttl"
))]
fn verifiable_release_chain(#[case] case: Case) {
    case.run();
}
