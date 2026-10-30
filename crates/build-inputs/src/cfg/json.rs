// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native JSON owners; hydration preserves the admitted fields without normalization.

use super::CfgContext;
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record},
};

::purrdf_lex::json_record! { impl ToJson for CfgContext {
    "flags" => flags,
    "values" => values,
    "known" => known,
}}

impl FromJson for CfgContext {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct CfgContext")?;
        let result = Self {
            flags: fields.required("flags")?,
            values: fields.required("values")?,
            known: fields.required("known")?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}
