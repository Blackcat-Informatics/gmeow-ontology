// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native migration evidence; unknown fields remain available to other owners.

use super::{ExtractionPlan, FormattedPreservation, Preservation, Replacement};
use gmeow_errors::json::read_unsigned;
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record},
};

fn pair<T>(
    value: &Value,
    expected: &str,
    reader: impl Fn(&Value) -> Result<T, DecodeError>,
) -> Result<[T; 2], DecodeError> {
    purrdf_lex::json::record::items_with(value, reader)?
        .try_into()
        .map_err(|_| DecodeError::invalid_value(value, expected))
}

/// Read a byte range with exact arity and unsigned host-width endpoints.
fn range(value: &Value) -> Result<[usize; 2], DecodeError> {
    pair(value, "an array of exactly two unsigned indices", |value| {
        read_unsigned::<usize>(value, "usize")
    })
}

fn function_bodies(value: &Value) -> Result<Vec<(String, String)>, DecodeError> {
    purrdf_lex::json::record::items_with(value, |value| {
        pair(value, "a function name and digest pair", String::from_json)
            .map(|[name, digest]| (name, digest))
    })
}

::purrdf_lex::json_record! { impl ToJson for Replacement {
    "path" => path,
    "before_sha256" => before_sha256,
    "contents" => contents,
}}

impl FromJson for Replacement {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Replacement")?;
        Ok(Self {
            path: fields.required("path")?,
            before_sha256: fields.optional("before_sha256")?,
            contents: fields.required("contents")?,
        })
    }
}

::purrdf_lex::json_record! { impl ToJson for Preservation {
    "source" => source,
    "destination" => destination,
    "module" => module,
    "original_range" => original_range,
    "extracted_range" => extracted_range,
    "original_body_sha256" => original_body_sha256,
    "extracted_body_sha256" => extracted_body_sha256,
    "function_bodies" => function_bodies,
}}

impl FromJson for Preservation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Preservation")?;
        Ok(Self {
            source: fields.required("source")?,
            destination: fields.required("destination")?,
            module: fields.required("module")?,
            original_range: fields.required_with("original_range", range)?,
            extracted_range: fields.required_with("extracted_range", range)?,
            original_body_sha256: fields.required("original_body_sha256")?,
            extracted_body_sha256: fields.required("extracted_body_sha256")?,
            function_bodies: fields.required_with("function_bodies", function_bodies)?,
        })
    }
}

::purrdf_lex::json_record! { impl ToJson for ExtractionPlan {
    "sources" => sources,
    "replacements" => replacements,
    "preservation" => preservation,
    "blockers" => blockers,
}}

impl FromJson for ExtractionPlan {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ExtractionPlan")?;
        Ok(Self {
            sources: fields.required("sources")?,
            replacements: fields.required("replacements")?,
            preservation: fields.required("preservation")?,
            blockers: fields.required("blockers")?,
        })
    }
}

::purrdf_lex::json_record! { impl ToJson for FormattedPreservation {
    "path" => path,
    "planned_sha256" => planned_sha256,
    "formatted_sha256" => formatted_sha256,
    "canonical_sha256" => canonical_sha256,
    "formatter" => formatter,
    "edition" => edition,
}}

impl FromJson for FormattedPreservation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct FormattedPreservation")?;
        Ok(Self {
            path: fields.required("path")?,
            planned_sha256: fields.required("planned_sha256")?,
            formatted_sha256: fields.required("formatted_sha256")?,
            canonical_sha256: fields.required("canonical_sha256")?,
            formatter: fields.required("formatter")?,
            edition: fields.required("edition")?,
        })
    }
}
