// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The slice-brief crate's diagnostic catalog — its share of the carrier-borne
//! graded-witness substrate. Every fallible assembly step returns a
//! [`gmeow_errors::Diag`], never a stringly-typed error, and a missing required
//! input is one of these hard failures (never a silent default).

use gmeow_errors::grade::{FindingCategory, Standpoint};
use gmeow_errors::{Grade, Severity, define_diag_kind};

macro_rules! brief_grade {
    () => {
        Grade::new(
            Severity::Error,
            FindingCategory::ModelingDisciplineViolation,
            Standpoint::Binding,
        )
    };
}

define_diag_kind! {
    /// A filesystem / dataset-load failure while reading a slice's Turtle.
    pub struct Io { detail: String }
    code = "slice-brief.io";
    grade = brief_grade!();
    message = "{}", detail;
}

define_diag_kind! {
    /// A malformed slice (no `gmeow:Slice`) or an out-of-range batch request.
    pub struct Partition { detail: String }
    code = "slice-brief.partition";
    grade = brief_grade!();
    message = "{}", detail;
}
