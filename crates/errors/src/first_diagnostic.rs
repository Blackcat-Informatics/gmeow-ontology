// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Retain the first typed refusal while an audit continues through its input.

use crate::{Diag, Result};

/// The first recorded diagnostic, with its original grade, context and source.
/// Streaming callbacks can record a refusal without ending the physical audit;
/// the audit owner retrieves it before reporting successful completion.
#[derive(Default)]
#[must_use]
pub struct FirstDiagnostic {
    first: Option<Diag>,
}

impl FirstDiagnostic {
    /// Retain the first failed result; later results cannot replace its cause.
    pub fn record(&mut self, result: Result<()>) {
        if self.first.is_none() {
            self.first = result.err();
        }
    }

    /// Transfer the retained diagnostic to the audit's result boundary.
    #[must_use]
    pub fn take(&mut self) -> Option<Diag> {
        self.first.take()
    }
}
