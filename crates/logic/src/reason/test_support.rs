// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only projections of the selected native reasoning operation.

use super::{InferredAxiom, PreparedReasoningInput, SelectedDomains, dl, reason_all};

/// Execute the selected native operation once and project its closure and verdict.
///
/// # Errors
/// Returns source admission, execution, or retained evidence validation failures.
pub(crate) fn reason_closure(
    input: PreparedReasoningInput,
    domains: &SelectedDomains,
) -> gmeow_errors::Result<(Vec<InferredAxiom>, dl::DlVerdict)> {
    let result = reason_all(input, domains)?;
    let verdict = result.native_verdict()?;
    Ok((result.inferred().to_vec(), verdict))
}
