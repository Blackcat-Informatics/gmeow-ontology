// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_suppression_conformance.py
//!
//! The all-profiles leak sweep (CONSTITUTION P10): "the leak is prevented" is made
//! a CI-proven property of every projection profile. Each profile's generated
//! CONSTRUCT is rendered over a canary corpus (the merged ontology + the suppression
//! canary fixture + the coarsen fixture), and the suppressed / precise values must
//! never surface in the projection's serialized output — while a displayable CONTROL
//! twin MUST surface in the appellation profiles, proving the absence checks are not
//! vacuous.
//!
//! The Python parametrized over the live `PROFILES` registry and scanned each
//! profile's `serialize(format="turtle")` string. The native twins iterate the 27
//! projection `.rq` files in `generated/queries/` (the profile set), run each
//! CONSTRUCT over the canary source, serialize the projection to N-Triples, and
//! apply the same substring checks.

use crate::conformance_support::*;

/// The projection profiles swept for leaks. The single source of truth is
/// `gmeow_validate::projection_profiles::PROJECTION_PROFILES`, pinned set-equal to
/// the live `gmeow_pipeline::projections::profiles()` registry by an on-gate
/// pipeline test — so a newly registered profile cannot escape this sweep. Each
/// name has a `generated/queries/{name}.rq` CONSTRUCT.
use gmeow_validate::projection_profiles::PROJECTION_PROFILES;

/// The appellation profiles the CONTROL canary MUST surface in — mirrors the Python
/// `_NAME_PROFILES`. The positive control proving the absence checks are non-vacuous.
const NAME_PROFILES: &[&str] = &["foaf", "schema-org", "vcard"];

/// The marked place's precise coordinates in `suppress-gen.ttl`; only the city's
/// coarsened coordinates are publishable — mirrors the Python `_PRECISE_COORDS`.
const PRECISE_COORDS: &[&str] = &["51.500001", "-0.124999"];

/// The merged ontology + the two canary fixtures — the `source` fixture twin.
fn canary_source() -> GraphStore {
    GraphStore::ontology_plus_ttl_files(&[
        repo_root().join("tests/fixtures/coverage/suppression-canary.ttl"),
        repo_root().join("tests/fixtures/coverage/suppress-gen.ttl"),
    ])
}

/// Serialize every profile's projection of the canary corpus to N-Triples once.
fn projections() -> Vec<(&'static str, String)> {
    let source = canary_source();
    PROJECTION_PROFILES
        .iter()
        .map(|name| {
            let query = read_query(&format!("generated/queries/{name}.rq"));
            let out = source.construct(&[], &query);
            (*name, out.to_nt())
        })
        .collect()
}

/// Twin of `test_suppressed_canary_never_leaks` (×27): a `displayable false`
/// value never surfaces in ANY profile's output.
#[gmeow_test_batch_macros::batch_test]
fn suppressed_canary_never_leaks() {
    for (profile, serialized) in projections() {
        assert!(
            !serialized.contains("SUPPRESSED-CANARY"),
            "profile {profile} leaked a displayable-false value"
        );
    }
}

/// Twin of `test_precise_coarsened_values_never_leak` (×27): a `coarsenTo`-marked
/// place's precise coordinates appear in no profile's output.
#[gmeow_test_batch_macros::batch_test]
fn precise_coarsened_values_never_leak() {
    for (profile, serialized) in projections() {
        for precise in PRECISE_COORDS {
            assert!(
                !serialized.contains(precise),
                "profile {profile} leaked a precise value {precise} past gmeow:coarsenTo"
            );
        }
    }
}

/// Twin of `test_control_canary_proves_coverage` (×3): the displayable CONTROL twin
/// DOES project in each appellation profile — the leak tests are not vacuous. This
/// positive control is LOAD-BEARING: if it stops surfacing, the absence checks above
/// would pass trivially.
#[gmeow_test_batch_macros::batch_test]
fn control_canary_proves_coverage() {
    let all = projections();
    for profile in NAME_PROFILES {
        let (_, serialized) = all
            .iter()
            .find(|(name, _)| name == profile)
            .unwrap_or_else(|| panic!("profile {profile} missing from projection set"));
        assert!(
            serialized.contains("CONTROL-CANARY"),
            "profile {profile} no longer projects the control canary — the \
             suppression conformance tests would be vacuous"
        );
    }
}
