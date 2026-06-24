// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_organization.py (#867)
//!
//! Each test loads a fixture file, converts it to N-Triples, and validates
//! against the whole shapes corpus.
//!
//! Retained in Python (not migrated):
//!   - `test_contested_membership_coexists`: mixes SHACL check with `g.objects()`
//!     graph content assertions not expressible in Rust without a query API.
//!   - `test_contested_succession_coexists`: same fixture, same graph-content pattern.
//!   - `test_withdrawn_recognition_suppressed_not_deleted`: checks `(triple) in g`
//!     graph membership after SHACL — requires graph query.
//!   - `test_post_seat_independent_of_holder`: `result.ok` + `g.objects()` + set check.
//!   - `test_post_successive_holders`: `result.ok` + `g.subjects()` set check.
//!   - `test_site_location`: `result.ok` + `g.objects()` + `in g` membership checks.
//!   - `test_change_event_entailments`: `result.ok` + `g.objects()` set checks.
//!   - `test_wellformed_legal_identifier_passes`: requires `g.remove()` graph mutation
//!     before validation — not expressible as a pure fixture test.
//!   - `test_no_preferred_or_primary_org_term`: pure TBox sweep over `_graph()`.
//!   - `test_change_event_type_values_exist`: cross-slice `_graph()` check; docstring
//!     marks RETAIN ("narrowing to scopeModule would miss cross-slice violations").

mod conformance_support;
use conformance_support::*;

// ── Tests migrated from tests/test_organization.py ───────────────────────────

/// `test_membership_fills_post_org_mismatch_warns` — a Membership filling a
/// Post in a different org triggers a SHACL Warning; validation still passes
/// (result.ok stays true) but the warning message contains the expected text.
#[test]
fn membership_fills_post_org_mismatch_warns() {
    let nt = fixture_as_nt("coverage", "organization-posts");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "org-mismatch fixture must pass SHACL (warnings only, not violations); violations: {:?}",
        violations(&report)
    );
    let all_messages: Vec<String> = warnings(&report)
        .into_iter()
        .chain(violations(&report))
        .collect();
    let combined = all_messages.join("\n");
    assert!(
        combined.contains("fills a Post whose organization differs"),
        "expected 'fills a Post whose organization differs' in SHACL messages; got: {combined:?}"
    );
}

/// `test_legal_identifier_requires_scheme` — an Identifier node without
/// identifierScheme triggers a SHACL Violation; validation must fail.
#[test]
fn legal_identifier_requires_scheme() {
    let nt = fixture_as_nt("coverage", "organization-legal-identity");
    let report = validate(&nt);
    assert!(
        !ok(&report),
        "malformed legal-identity fixture must fail SHACL validation"
    );
    let msgs = violations(&report);
    let combined = msgs.join("\n");
    assert!(
        combined.contains("must declare a gmeow:identifierScheme"),
        "expected 'must declare a gmeow:identifierScheme' in violation messages; got: {combined:?}"
    );
}
