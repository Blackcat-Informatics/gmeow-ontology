// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native JSON admission shared by diagnostic and receipt transports.

use purrdf_lex::json::{Value, record::DecodeError};

/// Read an unsigned integer token followed by a checked target-width conversion.
///
/// Signed zero, negative, fraction and exponent tokens are refused. `expected`
/// preserves the calling field's diagnostic wording; the containing native
/// record or collection supplies its field/index pointer.
///
/// # Errors
/// Returns a native value refusal for invalid token syntax or target overflow.
pub fn read_unsigned<T: TryFrom<u64>>(value: &Value, expected: &str) -> Result<T, DecodeError> {
    value
        .as_u64()
        .and_then(|number| T::try_from(number).ok())
        .ok_or_else(|| DecodeError::invalid_value(value, expected))
}
