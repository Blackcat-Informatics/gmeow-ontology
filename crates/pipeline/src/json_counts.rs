// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared host-width admission for observation counts with unsigned token syntax.

use purrdf_lex::json::{Value, record::DecodeError};

/// Read a count without accepting signed zero, decimal, or exponent tokens.
///
/// The pinned native integer `FromJson` uses signed integer admission; count
/// transports instead require `Value::as_u64` followed by the host-width check.
/// `expected` retains the calling record's diagnostic wording, while its native
/// `Record` or collection decoder supplies the field/index pointer.
///
/// # Errors
/// Returns the native value refusal for invalid token syntax or host overflow.
pub(crate) fn read_usize(value: &Value, expected: &str) -> Result<usize, DecodeError> {
    gmeow_errors::json::read_unsigned(value, expected)
}
