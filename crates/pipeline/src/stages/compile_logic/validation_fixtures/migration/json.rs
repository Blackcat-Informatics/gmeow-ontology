// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete finite migration evidence without executing either validator on hydration.

use super::{Agreement, Finding};
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record},
};

::purrdf_lex::json_record!(impl ToJson for Finding {
    "focus" => focus,
    "path" => path,
    "value" => value,
    "component" => component,
    "severity" => severity,
    "messages" => messages,
});
impl FromJson for Finding {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Finding")?;
        Ok(Self {
            focus: fields.required("focus")?,
            path: fields.optional("path")?,
            value: fields.optional("value")?,
            component: fields.required("component")?,
            severity: fields.required("severity")?,
            messages: fields.required("messages")?,
        })
    }
}
::purrdf_lex::json_record!(impl ToJson for Agreement {
    "legacy_shape" => legacy_shape,
    "projected_shape" => projected_shape,
    "checked_focuses" => checked_focuses,
    "expected_violations" => expected_violations,
    "legacy" => legacy,
    "projected" => projected,
});
impl FromJson for Agreement {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Agreement")?;
        Ok(Self {
            legacy_shape: fields.required("legacy_shape")?,
            projected_shape: fields.required("projected_shape")?,
            checked_focuses: fields.required("checked_focuses")?,
            expected_violations: fields.required("expected_violations")?,
            legacy: fields.required("legacy")?,
            projected: fields.required("projected")?,
        })
    }
}
