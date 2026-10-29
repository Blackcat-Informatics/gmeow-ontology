// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native JSON admission shared by diagnostic and receipt transports.

use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};

/// Implement native JSON for a tuple newtype holding one unconstrained String.
///
/// GMEOW owns the typed wrapper; PurRDF owns string admission and writing.
/// This adapter deliberately has no validation or alternate construction hook.
#[macro_export]
macro_rules! json_string_newtype {
    ($type:ty) => {
        impl ::purrdf_lex::json::record::ToJson for $type {
            fn to_json(&self) -> ::purrdf_lex::json::Value {
                ::purrdf_lex::json::record::ToJson::to_json(&self.0)
            }
        }
        impl ::purrdf_lex::json::record::FromJson for $type {
            fn from_json(
                value: &::purrdf_lex::json::Value,
            ) -> ::std::result::Result<Self, ::purrdf_lex::json::record::DecodeError> {
                <::std::string::String as ::purrdf_lex::json::record::FromJson>::from_json(value)
                    .map(Self)
            }
        }
    };
}

pub use crate::json_string_newtype;

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

/// Read the named subject, predicate and object fields of a statement record.
/// Each transport retains its own subject/object types and record diagnostic.
///
/// # Errors
/// Refuses missing, repeated or mistyped fields in subject/predicate/object order.
pub fn read_statement_fields<S: FromJson, O: FromJson>(
    value: &Value,
    expected: &str,
) -> Result<(S, String, O), DecodeError> {
    let mut fields = Record::new(value, expected)?;
    Ok((
        fields.required("subject")?,
        fields.required("predicate")?,
        fields.required("object")?,
    ))
}

/// Read one fixed-width commitment as strictly unsigned byte tokens.
/// The containing record supplies its field pointer; native array admission
/// retains the index of any malformed or out-of-range byte.
///
/// # Errors
/// Refuses nonarrays, non-byte tokens and a commitment of any other width.
pub fn read_byte_array<const N: usize>(value: &Value) -> Result<[u8; N], DecodeError> {
    purrdf_lex::json::record::items_with(value, |value| {
        read_unsigned::<u8>(value, "an unsigned byte")
    })?
    .try_into()
    .map_err(|_| DecodeError::invalid_value(value, &format!("exactly {N} unsigned bytes")))
}

/// Read one fixed-width row of verbatim strings using native element admission.
///
/// # Errors
/// Refuses nonarrays, nonstring cells and a row whose width differs from `N`.
pub fn read_string_row<const N: usize>(value: &Value) -> Result<[String; N], DecodeError> {
    Vec::<String>::from_json(value)?
        .try_into()
        .map_err(|_| DecodeError::invalid_value(value, &format!("exactly {N} string cells")))
}

/// Admit the null payload of a successful unit observation.
/// The owning transport supplies its existing diagnostic wording.
///
/// # Errors
/// Refuses every non-null value without normalization.
pub fn read_null(value: &Value, expected: &str) -> Result<(), DecodeError> {
    if matches!(value, Value::Null) {
        Ok(())
    } else {
        Err(DecodeError::invalid_type(value, expected))
    }
}

/// Read a signed integer token followed by a checked target-width conversion.
/// The float-like spelling `-0` is refused.
///
/// # Errors
/// Returns a native value refusal for invalid token syntax or target overflow.
pub fn read_signed<T: TryFrom<i128>>(value: &Value, expected: &str) -> Result<T, DecodeError> {
    value
        .as_number()
        .filter(|number| number.lexeme() != "-0")
        .and_then(|number| number.as_i128())
        .and_then(|number| T::try_from(number).ok())
        .ok_or_else(|| DecodeError::invalid_value(value, expected))
}

/// Borrow the single member of an externally tagged enum object.
/// Separate type/cardinality expectations retain the owning transport's wording.
///
/// # Errors
/// Refuses nonobjects, empty objects and multiple members, including duplicates.
pub fn read_variant<'a>(
    value: &'a Value,
    type_expected: &str,
    cardinality_expected: &str,
) -> Result<(&'a str, &'a Value), DecodeError> {
    let object = value
        .as_object()
        .ok_or_else(|| DecodeError::invalid_type(value, type_expected))?;
    let mut entries = object.iter();
    let Some((name, payload)) = entries.next() else {
        return Err(DecodeError::invalid_value(value, cardinality_expected));
    };
    if entries.next().is_some() {
        return Err(DecodeError::invalid_value(value, cardinality_expected));
    }
    Ok((name, payload))
}

/// Read a unit enum's string or its exact singleton-null-object form.
///
/// # Errors
/// Refuses any other representation; the owning enum admits the returned name.
pub fn read_unit_name<'a>(value: &'a Value, expected: &str) -> Result<&'a str, DecodeError> {
    if let Some(name) = value.as_str() {
        return Ok(name);
    }
    let (name, payload) = read_variant(value, expected, expected)?;
    if !matches!(payload, Value::Null) {
        return Err(DecodeError::invalid_value(value, expected));
    }
    Ok(name)
}

/// Read a producer outcome carrying either its typed value or a recorded diagnostic.
/// The owning transport supplies its existing type and cardinality wording.
///
/// # Errors
/// Refuses malformed variants or payloads, retaining the `Ok`/`Err` field pointer.
pub fn read_recorded_result<T: FromJson>(
    value: &Value,
    type_expected: &str,
    cardinality_expected: &str,
) -> Result<Result<T, crate::RecordedDiag>, DecodeError> {
    let (tag, _) = read_variant(value, type_expected, cardinality_expected)?;
    let mut fields = Record::new(value, type_expected)?;
    match tag {
        "Ok" => fields.required("Ok").map(Ok),
        "Err" => fields.required("Err").map(Err),
        name => Err(DecodeError::unknown_variant(name, &["Ok", "Err"])),
    }
}

/// A complete producer outcome admitted at a native JSON boundary.
/// Readers unwrap the result after hydration; the recorded failure is retained verbatim.
#[derive(Debug)]
pub struct RecordedOutcome<T>(
    /// The complete typed success or owned recorded diagnostic.
    pub Result<T, crate::RecordedDiag>,
);

impl<T: FromJson> FromJson for RecordedOutcome<T> {
    /// Hydrate the singleton outcome through the shared recorded-result reader.
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        read_recorded_result(value, "recorded outcome", "one recorded outcome variant").map(Self)
    }
}

/// Write a producer outcome with the same singleton tags as its recorded-result reader.
/// The failure payload is always the complete owned RecordedDiag, including causes.
#[must_use]
pub fn recorded_result_to_json<T: ToJson>(outcome: &Result<T, crate::RecordedDiag>) -> Value {
    match outcome {
        Ok(value) => Object::new().with("Ok", value.to_json()).into(),
        Err(error) => Object::new().with("Err", error.to_json()).into(),
    }
}

/// Project sorted producer keys and complete typed recorded outcomes without cloning.
#[must_use]
pub fn recorded_map_to_json<T: ToJson>(
    outcomes: &std::collections::BTreeMap<String, Result<T, crate::RecordedDiag>>,
) -> Value {
    outcomes
        .iter()
        .map(|(name, outcome)| (name.as_str(), recorded_result_to_json(outcome)))
        .collect::<Object>()
        .into()
}

struct UnitSuccess;

impl FromJson for UnitSuccess {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        read_null(value, "null unit success").map(|()| Self)
    }
}

/// Read a recorded unit outcome through the same typed variant admission.
///
/// # Errors
/// Refuses malformed variants, non-null successes and incomplete diagnostics.
pub fn read_recorded_unit_result(
    value: &Value,
    type_expected: &str,
    cardinality_expected: &str,
) -> Result<Result<(), crate::RecordedDiag>, DecodeError> {
    read_recorded_result::<UnitSuccess>(value, type_expected, cardinality_expected)
        .map(|outcome| outcome.map(|UnitSuccess| ()))
}

/// Write a null unit success or its complete recorded causal diagnostic.
#[must_use]
pub fn recorded_unit_result_to_json(outcome: &Result<(), crate::RecordedDiag>) -> Value {
    match outcome {
        Ok(()) => Object::new().with("Ok", Value::Null).into(),
        Err(error) => Object::new().with("Err", error.to_json()).into(),
    }
}

#[cfg(test)]
mod tests;
