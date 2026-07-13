// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Substrate-model diagnostic kinds.
//!
//! Parsing a user/tool-supplied [`Severity`] or
//! [`FindingCategory`] wire label is a HARD
//! failure surface: an unrecognized token has no silent default (no-optionality),
//! so a typo cannot degrade to a wrong severity/category. Each defect is a
//! [`DiagKind`](crate::diag::DiagKind) minted by `define_diag_kind!`, carrying a
//! stable registered [`Code`], a [`Grade`], and staying downcastable off the
//! [`Diag`](crate::diag::Diag) source.

use crate::code::Code;
use crate::grade::{Grade, Severity, Standpoint};
use crate::model::FindingCategory;

crate::define_diag_kind! {
    /// An unrecognized diagnostic-severity label. `value` is the offending token
    /// (trimmed); the severity vocabulary has no silent fallback, so an unknown
    /// spelling is a hard fail rather than a degrade to a default severity.
    pub struct UnknownSeverityLabel { value: String }
    code = "errors.model.unknown-severity-label";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "unknown diagnostic severity `{}`; expected error, warning, note, or info", value;
}

crate::define_diag_kind! {
    /// An unrecognized `logic:FindingCategory` wire label. `value` is the
    /// offending token (trimmed); the closed category taxonomy has no silent
    /// fallback, so an unknown spelling is a hard fail.
    pub struct UnknownFindingCategory { value: String }
    code = "errors.model.unknown-finding-category";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "unknown finding category `{}`; expected one of the ten logic:FindingCategory wire values", value;
}

/// The complete `gmeow-errors` model diagnostic-code catalog, in registration
/// order. Every [`DiagKind`](crate::diag::DiagKind) minted here appears once —
/// [`register_all`] seeds them and the collision test proves the code strings are
/// distinct.
pub const ERRORS_DIAG_CODES: &[&str] = &[UnknownSeverityLabel::CODE, UnknownFindingCategory::CODE];

/// Eagerly intern every `gmeow-errors` model diagnostic code, seeding the
/// process-wide code registry. Idempotent (each `register()` is a `LazyLock`).
pub fn register_all() -> Vec<Code> {
    vec![
        UnknownSeverityLabel::register(),
        UnknownFindingCategory::register(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::intern_code;
    use std::collections::HashSet;

    #[test]
    fn every_errors_code_interns_with_no_collision() {
        let handles = register_all();
        assert_eq!(
            handles.len(),
            ERRORS_DIAG_CODES.len(),
            "register_all() and ERRORS_DIAG_CODES must enumerate the same kinds"
        );
        for code in ERRORS_DIAG_CODES {
            assert!(
                intern_code(code).is_ok(),
                "errors code `{code}` did not intern after register_all()"
            );
        }
        let distinct_strings: HashSet<&&str> = ERRORS_DIAG_CODES.iter().collect();
        assert_eq!(
            distinct_strings.len(),
            ERRORS_DIAG_CODES.len(),
            "duplicate errors diagnostic code string detected"
        );
        let distinct_handles: HashSet<Code> = handles.iter().copied().collect();
        assert_eq!(
            distinct_handles.len(),
            handles.len(),
            "two errors diagnostic kinds interned to the same code handle"
        );
    }
}
