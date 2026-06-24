// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_cognition.py (#867)
//!
//! Migrated tests (SHACL fixture-based, `run_shacl(...)` calls):
//!   - `test_wellformed_knowledge_proficiency_conforms`  →  `wellformed_knowledge_proficiency_conforms`
//!   - `test_malformed_knowledge_proficiency_is_flagged` →  `malformed_knowledge_proficiency_is_flagged`
//!
//! Retained in Python (not migrated):
//!   - `test_mental_moment_is_category_under_intrinsic_mode`: cross-slice subject
//!     (gmeow:MentalMoment lives in slices/core/kernel); `_graph()` TBox check.
//!   - `test_mental_moment_has_exactly_one_gufo_metaclass`: dynamic sweep over an
//!     open metaclass set; cannot be faithfully encoded as a finite Rust test.
//!   - `test_intentional_mode_reparented_under_mental_moment`: cross-slice subject
//!     (gmeow:IntentionalMode lives in slices/core/teleology); `_graph()` TBox check.
//!   - `test_proficiency_vocab_relocated_to_kernel`: cross-slice subjects
//!     (ProficiencyScale/Level/Modality live in slices/core/kernel); `_graph()` check.
//!   - `test_cognition_sssom_*`: `load_mappings()` / MAP-flag ledger checks; no
//!     SHACL validation involved.

mod conformance_support;
use conformance_support::*;

// ── Tests migrated from tests/test_cognition.py ───────────────────────────────

/// `test_wellformed_knowledge_proficiency_conforms` — a well-formed
/// `KnowledgeProficiency` with exactly one subject, one level, one agent, and
/// one scale passes SHACL.
///
/// Mirrors the `cognition-wellformed.ttl` fixture loaded by the Python test.
#[test]
fn wellformed_knowledge_proficiency_conforms() {
    let nt = fixture_as_nt("shapes", "cognition-wellformed");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "well-formed KnowledgeProficiency must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_malformed_knowledge_proficiency_is_flagged` — a malformed
/// `KnowledgeProficiency` (no subject, no level, two scales) is rejected with
/// the expected violation messages.
///
/// Mirrors the `cognition-malformed.ttl` fixture loaded by the Python test.
#[test]
fn malformed_knowledge_proficiency_is_flagged() {
    let nt = fixture_as_nt("shapes", "cognition-malformed");
    let report = validate(&nt);
    assert!(
        !ok(&report),
        "malformed KnowledgeProficiency must fail SHACL"
    );
    let errs = violations(&report);
    assert!(!errs.is_empty(), "expected at least one violation");
    let joined = errs.join("\n");
    assert!(
        joined.contains("must reference exactly one subject"),
        "expected 'must reference exactly one subject' in violations; got:\n{joined}"
    );
    assert!(
        joined.contains("must carry exactly one KnowledgeLevel"),
        "expected 'must carry exactly one KnowledgeLevel' in violations; got:\n{joined}"
    );
    assert!(
        joined.contains("at most one scale"),
        "expected 'at most one scale' in violations; got:\n{joined}"
    );
}
