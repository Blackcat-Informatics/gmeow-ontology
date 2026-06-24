// SPDX-License-Identifier: AGPL-3.0-only
//! Conformance twins migrated from tests/test_norms.py (#867)
//!
//! Migrated: the two `run_shacl`-based fixture tests.
//!
//! Retained in Python (not migrated):
//!   - `test_graft_axioms_live_extension_side_only`: cross-slice file-load check
//!     that iterates subjects dynamically; no `run_shacl` call.
//!   - `test_graft_preserves_core_trio_classhood`: uses `_graph()` /
//!     `load_merged_graph` + `(triple) in g` membership assertions.
//!   - `test_competency_deontic_modalities_query`: external `.rq` file +
//!     SPARQL SELECT result-set check.
//!   - `test_competency_authority_order_query`: external `.rq` file +
//!     SPARQL SELECT result-set check.
//!
//! Both fixture tests use `validate` (fixture-only, no merged ontology), which
//! mirrors Python's `run_shacl` which validates the fixture N-Triples directly
//! against the shapes corpus without merging the base ontology.
mod conformance_support;
use conformance_support::*;

/// `test_wellformed_norms_fixture_conforms` — a fully well-formed norms fixture
/// (two-tier normative system, scoped tenure, conditional norm with ConditionGroup,
/// parameter binding, vantage-indexed evaluation, compliance assessment, and a
/// grafted Permission) must pass SHACL.
#[test]
fn wellformed_norms_fixture_conforms() {
    let nt = fixture_as_nt("shapes", "norms-wellformed");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "well-formed norms fixture must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_malformed_norms_fixture_is_flagged` — eight deliberate violations in
/// the malformed fixture must all be caught:
///   1. anonymous ought (no issuer)
///   2. self-override (`narcissusNorm overrides narcissusNorm`)
///   3. one-member ConditionGroup (needs >= 2 members + operator)
///   4. parameter binding both value and entity (XOR)
///   5. PrecedenceTenure higher = lower, and missing scope
///   6. grafted Permission claiming obligation force
///   7. norm claiming two deontic forces at once
///   8. ConditionEvaluation with no verdict
#[test]
fn malformed_norms_fixture_is_flagged() {
    let nt = fixture_as_nt("shapes", "norms-malformed");
    let report = validate(&nt);
    assert!(
        !ok(&report),
        "malformed norms fixture must fail SHACL; got no violations"
    );
    let errs = violations(&report);
    let joined = errs.join("\n");
    assert!(
        errs.iter()
            .any(|v| v.contains("no ought, only ought-according-to")),
        "expected 'no ought, only ought-according-to' in violations;\ngot: {joined}"
    );
    assert!(
        errs.iter().any(|v| v.contains("never overrides itself")),
        "expected 'never overrides itself' in violations;\ngot: {joined}"
    );
    assert!(
        errs.iter()
            .any(|v| v.contains("at least two gmeow:groupMember")),
        "expected 'at least two gmeow:groupMember' in violations;\ngot: {joined}"
    );
    assert!(
        errs.iter()
            .any(|v| v.contains("exactly one gmeow:groupOperator")),
        "expected 'exactly one gmeow:groupOperator' in violations;\ngot: {joined}"
    );
    assert!(
        errs.iter()
            .any(|v| v.contains("binds exactly one of gmeow:parameterValue")),
        "expected 'binds exactly one of gmeow:parameterValue' in violations;\ngot: {joined}"
    );
    assert!(
        errs.iter()
            .any(|v| v.contains("higher and lower norms must be distinct")),
        "expected 'higher and lower norms must be distinct' in violations;\ngot: {joined}"
    );
    assert!(
        errs.iter().any(|v| v.contains("must be scoped to exactly one gmeow:precedenceScope")),
        "expected 'must be scoped to exactly one gmeow:precedenceScope' in violations;\ngot: {joined}"
    );
    assert!(
        errs.iter()
            .any(|v| v.contains("must be gmeow:deonticPermission")),
        "expected 'must be gmeow:deonticPermission' in violations;\ngot: {joined}"
    );
    assert!(
        errs.iter()
            .any(|v| v.contains("exactly one gmeow:evaluationVerdict")),
        "expected 'exactly one gmeow:evaluationVerdict' in violations;\ngot: {joined}"
    );
    assert!(
        errs.iter()
            .any(|v| v.contains("at most one gmeow:deonticModality")),
        "expected 'at most one gmeow:deonticModality' in violations;\ngot: {joined}"
    );
}
