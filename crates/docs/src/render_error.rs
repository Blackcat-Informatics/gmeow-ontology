// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Typed failures of documentation rendering and its source-link admission.

use gmeow_errors::{FindingCategory, Grade, Severity, Standpoint, define_diag_kind};

define_diag_kind! {
    native_cause;
    /// The fixture producer cannot discover or admit its input model.
    pub struct FixtureModel { detail: String }
    code = "docs.fixture.model-admission";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    native_cause;
    /// The HTML template cannot render the selected page.
    pub struct Template { detail: String }
    code = "docs.render.template";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    native_cause;
    /// The supplied documentation model cannot form an unambiguous source map.
    pub struct SourceMap { detail: String }
    code = "docs.render.source-map";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// An authored heading cannot fit the selected document embedding.
    pub struct HeadingOverflow { detail: String }
    code = "docs.render.heading-overflow";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    /// An internal document or anchor target is absent from the selected render.
    pub struct DanglingLink { detail: String }
    code = "docs.render.dangling-link";
    grade = Grade::new(Severity::Error, FindingCategory::DataShapeViolation, Standpoint::Binding);
    message = "{}", detail;
}

define_diag_kind! {
    native_cause;
    /// A selected syntax provider cannot render its worked example faithfully.
    pub struct Syntax { detail: String }
    code = "docs.render.syntax";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}", detail;
}
