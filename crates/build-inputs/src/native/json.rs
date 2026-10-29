// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native migration evidence; unknown fields remain available to other owners.

use super::NativeSources;
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record},
};

::purrdf_lex::json_record! { impl ToJson for NativeSources {
    "files" => files,
    "excluded" => excluded,
}}

impl FromJson for NativeSources {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct NativeSources")?;
        Ok(Self {
            files: fields.required("files")?,
            excluded: fields.required("excluded")?,
        })
    }
}
