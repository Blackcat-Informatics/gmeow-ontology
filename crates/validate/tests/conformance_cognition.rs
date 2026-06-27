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
use rstest::rstest;

// ── Tests migrated from tests/test_cognition.py ───────────────────────────────

#[rstest]
#[case::wellformed_knowledge_proficiency_conforms(Case::file("shapes", "cognition-wellformed"))]
#[case::malformed_knowledge_proficiency_is_flagged(
    Case::file("shapes", "cognition-malformed")
        .fails()
        .violations(&[
            "must reference exactly one subject",
            "must carry exactly one KnowledgeLevel",
            "at most one scale",
        ])
)]
fn cognition(#[case] case: Case) {
    case.run();
}
