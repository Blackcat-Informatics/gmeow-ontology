// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Admission diagnostics for the census request, ledger and source inventory.

use std::error::Error;
use std::fmt;
use std::sync::Arc;

use gmeow_errors::diag::Diag;
use gmeow_errors::grade::{Grade, Severity, Standpoint};
use gmeow_errors::model::FindingCategory;

/// The input boundary that refused admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Operation {
    Arguments,
    LedgerParse,
    LedgerSchema,
    ResolveHome,
    Inventory,
}

impl fmt::Display for Operation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Arguments => "arguments",
            Self::LedgerParse => "ledger syntax",
            Self::LedgerSchema => "ledger schema",
            Self::ResolveHome => "home resolution",
            Self::Inventory => "source inventory",
        })
    }
}

gmeow_errors::define_diag_kind! {
    /// An inadmissible census input, retaining its boundary and original cause.
    pub(crate) struct Admission {
        operation: Operation,
        detail: String,
        cause: Option<Arc<dyn Error + Send + Sync>>,
    }
    code = "helper-census.admission";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "{}: {}", operation, detail;
    failure_class = "https://blackcatinformatics.ca/logic/HelperSourceAdmissionRefusal";
    source = kind => kind.cause.as_deref().map(|cause| cause as &(dyn Error + 'static));
}

/// Refuse a request or invariant without inventing an underlying I/O error.
#[track_caller]
pub(crate) fn admission(operation: Operation, detail: impl Into<String>) -> Diag {
    Diag::of_kind(Admission {
        operation,
        detail: detail.into(),
        cause: None,
    })
}

/// Retain the actual parser or regular-expression cause beneath admission.
#[track_caller]
pub(crate) fn caused(operation: Operation, cause: impl Error + Send + Sync + 'static) -> Diag {
    Diag::of_kind(Admission {
        operation,
        detail: cause.to_string(),
        cause: Some(Arc::new(cause)),
    })
}
