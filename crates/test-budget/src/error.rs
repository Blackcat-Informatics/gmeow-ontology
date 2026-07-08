// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Diagnostic kinds for the per-test duration budget gate.
//!
//! Both failure surfaces are HARD fails (no-optionality): a malformed budget
//! env var or a JUnit `<testcase>` missing its `time` attribute must trip the
//! gate rather than degrade. Each defect is a
//! [`DiagKind`](gmeow_errors::DiagKind) minted by
//! [`define_diag_kind!`](gmeow_errors::define_diag_kind), carrying a stable
//! registered code and grade.

use gmeow_errors::{Code, FindingCategory, Grade, Severity, Standpoint, define_diag_kind};

define_diag_kind! {
    /// The `GMEOW_TEST_BUDGET_SECS` override is present but malformed (non-UTF-8,
    /// unparsable, non-finite, or non-positive). `reason` carries the specific
    /// defect; the budget policy has no silent fallback for a set-but-invalid var.
    pub struct InvalidBudgetVar { reason: String }
    code = "gmeow-test-budget.env.invalid-budget-var";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", reason;
}

define_diag_kind! {
    /// A JUnit `<testcase>` element carries no parseable `time` attribute. A
    /// silent drop would weaken the gate, so a missing timing is a hard fail.
    pub struct MalformedTestcase { classname: String, name: String }
    code = "gmeow-test-budget.parse.malformed-testcase";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "testcase {}::{} has no parseable `time` attribute — JUnit report is malformed or a new testcase shape is missing timing data", classname, name;
}

/// The complete test-budget diagnostic-code catalog, in registration order.
/// (Consumed by the collision test; the running binary reaches its kinds
/// directly, so the catalog itself is test-facing.)
#[allow(dead_code)]
pub const TEST_BUDGET_DIAG_CODES: &[&str] = &[InvalidBudgetVar::CODE, MalformedTestcase::CODE];

/// Eagerly intern every test-budget diagnostic code (idempotent).
pub fn register_all() -> Vec<Code> {
    vec![InvalidBudgetVar::register(), MalformedTestcase::register()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_errors::intern_code;
    use std::collections::HashSet;

    #[test]
    fn every_test_budget_code_interns_with_no_collision() {
        let handles = register_all();
        assert_eq!(
            handles.len(),
            TEST_BUDGET_DIAG_CODES.len(),
            "register_all() and TEST_BUDGET_DIAG_CODES must enumerate the same kinds"
        );
        for code in TEST_BUDGET_DIAG_CODES {
            assert!(
                intern_code(code).is_ok(),
                "test-budget code `{code}` did not intern after register_all()"
            );
        }
        let distinct_strings: HashSet<&&str> = TEST_BUDGET_DIAG_CODES.iter().collect();
        assert_eq!(
            distinct_strings.len(),
            TEST_BUDGET_DIAG_CODES.len(),
            "duplicate test-budget diagnostic code string detected"
        );
        let distinct_handles: HashSet<Code> = handles.iter().copied().collect();
        assert_eq!(distinct_handles.len(), handles.len());
    }
}
