// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The slice-quality crate's diagnostic catalog — its share of the one carrier-borne
//! graded-witness substrate. Every fallible slice-quality operation returns a
//! [`gmeow_errors::Diag`], never a stringly-typed error.

use gmeow_errors::grade::{FindingCategory, Standpoint};
use gmeow_errors::{Grade, Severity, define_diag_kind};

macro_rules! sq_grade {
    () => {
        Grade::new(
            Severity::Error,
            FindingCategory::ModelingDisciplineViolation,
            Standpoint::Binding,
        )
    };
}

define_diag_kind! {
    /// A filesystem / dataset-load failure in the slice-quality reader.
    pub struct Io { detail: String }
    code = "slice-quality.io";
    grade = sq_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// A malformed or incomplete quality rubric.
    pub struct Rubric { detail: String }
    code = "slice-quality.rubric";
    grade = sq_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// A slice-gate resolution failure (unknown declared tier, unresolvable slice).
    pub struct Gate { detail: String }
    code = "slice-quality.gate";
    grade = sq_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// A scoring / report-assembly failure over a slice.
    pub struct Report { detail: String }
    code = "slice-quality.report";
    grade = sq_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// A failure reading the RECORDED quality-assessment corpus back: the projection
    /// is absent, unparseable, structurally incomplete (a grade missing its axis,
    /// score, or tier), or stale with respect to the authored sources it claims to
    /// describe. Every one of these is a hard failure — a consumer of the record must
    /// never fall back to a partial reading or to trusting an unverified record.
    pub struct Record { detail: String }
    code = "slice-quality.record";
    grade = sq_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// A reasoning-pass failure in the slice-quality axis producers.
    pub struct Reason { detail: String }
    code = "slice-quality.reason";
    grade = sq_grade!();
    message = "{}", detail;
}

/// The interned code of every kind this crate mints — the collision test's coverage set.
pub const SLICE_QUALITY_DIAG_CODES: &[fn() -> gmeow_errors::Code] = &[
    Io::register,
    Rubric::register,
    Gate::register,
    Report::register,
    Record::register,
    Reason::register,
];

/// Intern every slice-quality diagnostic code (idempotent).
pub fn register_all() {
    for reg in SLICE_QUALITY_DIAG_CODES {
        let _ = reg();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn every_slice_quality_code_interns_with_no_collision() {
        register_all();
        let codes: BTreeSet<_> = SLICE_QUALITY_DIAG_CODES.iter().map(|reg| reg()).collect();
        assert_eq!(
            codes.len(),
            SLICE_QUALITY_DIAG_CODES.len(),
            "slice-quality diagnostic codes must be collision-free"
        );
    }
}
