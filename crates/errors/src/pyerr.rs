// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The pure grade → exception-kind decision.
//!
//! [`diag_to_pyerr`](crate::py::diag_to_pyerr) is the ONE `Diag` → `PyErr`
//! contract, but the *class-selection* it performs must be verifiable without a
//! Python runtime — so the decision is factored here as a pure, total function
//! over the [`Grade`] bilattice. `py.rs` maps each [`PyErrKind`] onto a concrete
//! pyo3 exception class. This module carries **no** pyo3 dependency and is always
//! compiled, so its exhaustive coverage test runs on the plain crate.

use crate::grade::{FindingCategory, Grade, Severity};

/// The abstract exception tier a diagnostic maps to — a Python-runtime-free
/// stand-in for the concrete pyo3 exception class chosen in
/// [`diag_to_pyerr`](crate::py::diag_to_pyerr).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PyErrKind {
    /// A real, blocking failure that is a *value/data* problem (→ `PyValueError`).
    Value,
    /// A real, blocking failure in the *runtime* of a computation
    /// (→ `PyRuntimeError`).
    Runtime,
    /// Non-fatal advisory / chatter — a warning that must never present as a hard
    /// error across the FFI boundary (→ a `Warning` subclass).
    Warning,
}

impl PyErrKind {
    /// Whether this kind is a hard (blocking) error class, as opposed to the
    /// non-fatal [`Warning`](PyErrKind::Warning) tier. The gate the exhaustive
    /// test joins on: chatter must never land in the hard-error region.
    pub fn is_hard_error(self) -> bool {
        matches!(self, PyErrKind::Value | PyErrKind::Runtime)
    }
}

/// The single, total, deterministic grade → [`PyErrKind`] decision.
///
/// [`Transient`](FindingCategory::Transient) chatter and every sub-`Error`
/// severity are non-fatal and map to the [`Warning`](PyErrKind::Warning) tier so
/// they can never present as a hard error across the FFI boundary. A real
/// `Error`-severity failure maps to a hard-error class chosen by the finding
/// category: a [`ContradictionWitness`](FindingCategory::ContradictionWitness) is
/// a runtime contradiction ([`Runtime`](PyErrKind::Runtime)); every other category
/// is a value/data problem ([`Value`](PyErrKind::Value)).
pub fn pyerr_kind(grade: &Grade) -> PyErrKind {
    // Transient chatter and every sub-error severity are non-fatal: they must
    // never surface as a hard error, whatever their category.
    if grade.category == FindingCategory::Transient
        || matches!(
            grade.severity,
            Severity::Note | Severity::Info | Severity::Warning
        )
    {
        return PyErrKind::Warning;
    }
    // Error severity, non-transient category: a real blocking failure. The
    // category selects the concrete hard-error class.
    match grade.category {
        FindingCategory::ContradictionWitness => PyErrKind::Runtime,
        _ => PyErrKind::Value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grade::Standpoint;

    #[test]
    fn kind_is_total_and_chatter_is_never_a_hard_error() {
        // Exhaustive over FindingCategory::ALL × Severity::ALL × Standpoint::ALL:
        // every combination yields a kind (total), and the Note/Info/Transient
        // chatter region never maps to a hard-error class.
        for &category in &FindingCategory::ALL {
            for &severity in &Severity::ALL {
                for &standpoint in &Standpoint::ALL {
                    let grade = Grade::new(severity, category, standpoint);
                    let kind = pyerr_kind(&grade);
                    if category == FindingCategory::Transient
                        || matches!(severity, Severity::Note | Severity::Info)
                    {
                        assert!(
                            !kind.is_hard_error(),
                            "chatter must never present as a hard error: {grade:?} -> {kind:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn error_severity_selects_hard_error_classes() {
        // A real Error-severity, non-transient failure is always a hard error; a
        // ContradictionWitness is Runtime, every other category is Value.
        for &category in &FindingCategory::ALL {
            if category == FindingCategory::Transient {
                continue;
            }
            let kind = pyerr_kind(&Grade::new(Severity::Error, category, Standpoint::Binding));
            assert!(
                kind.is_hard_error(),
                "Error/{category:?} must be a hard error"
            );
            if category == FindingCategory::ContradictionWitness {
                assert_eq!(kind, PyErrKind::Runtime);
            } else {
                assert_eq!(kind, PyErrKind::Value);
            }
        }
    }

    #[test]
    fn transient_error_is_still_not_a_hard_error() {
        // The Transient chatter category is non-fatal at EVERY severity — even at
        // Error severity it must not present as a hard error across the FFI.
        for &severity in &Severity::ALL {
            for &standpoint in &Standpoint::ALL {
                let kind = pyerr_kind(&Grade::new(
                    severity,
                    FindingCategory::Transient,
                    standpoint,
                ));
                assert_eq!(kind, PyErrKind::Warning);
            }
        }
    }

    #[test]
    fn warning_severity_never_hard_errors() {
        for &category in &FindingCategory::ALL {
            assert_eq!(
                pyerr_kind(&Grade::new(
                    Severity::Warning,
                    category,
                    Standpoint::Binding
                )),
                PyErrKind::Warning
            );
        }
    }
}
