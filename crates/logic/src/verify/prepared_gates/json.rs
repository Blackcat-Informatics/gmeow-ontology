// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Full prepared-law artifact; only persistent query operands can be published.
use super::{MathCompilation, MathConstraint, PreparedReasonedGates};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};
impl PreparedReasonedGates {
    /// Encode compact native prepared-law JSON without a terminal newline.
    ///
    /// # Errors
    /// Refuses arena-local query handles with their rule/operand context and
    /// preserves native storage refusals. No partial artifact is returned.
    pub fn to_json_bytes(&self) -> gmeow_errors::Result<Vec<u8>> {
        let value = self.json_value()?;
        let mut resident = purrdf_lex::allocation::Resident;
        let mut memory = purrdf_lex::allocation::Memory::new(&mut resident);
        purrdf_lex::json::write_with_memory(&value, purrdf_lex::json::Format::COMPACT, &mut memory)
            .map(String::into_bytes)
            .map_err(gmeow_errors::Diag::from)
    }

    /// Hydrate native persistence data without preparing or validating source identities.
    /// Callers retain their explicit source-identity admission after authentication.
    ///
    /// # Errors
    /// Preserves native JSON/member/variant refusals, including arena-only query terms.
    pub fn from_json_bytes(bytes: &[u8]) -> gmeow_errors::Result<Self> {
        purrdf_lex::json::record::from_slice(bytes).map_err(gmeow_errors::Diag::from)
    }

    fn json_value(&self) -> gmeow_errors::Result<Value> {
        Ok(Object::new()
            .with("schema_version", self.schema_version.to_json())
            .with("source_digests", self.source_digests.to_json())
            .with(
                "math_rules",
                Value::Array(
                    self.math_rules
                        .iter()
                        .enumerate()
                        .map(|(index, rule)| {
                            rule.json_value()
                                .map_err(|error| error.with_context(format!("math_rules[{index}]")))
                        })
                        .collect::<gmeow_errors::Result<Vec<_>>>()?,
                ),
            )
            .with(
                "enactment",
                self.enactment
                    .json_value()
                    .map_err(|error| error.with_context("enactment"))?,
            )
            .with("math_compilation", self.math_compilation.to_json())
            .into())
    }
}
impl FromJson for PreparedReasonedGates {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct PreparedReasonedGates")?;
        Ok(Self {
            schema_version: fields.required_with("schema_version", |value| {
                gmeow_errors::json::read_unsigned(value, "u32 prepared-law schema")
            })?,
            source_digests: fields.required("source_digests")?,
            math_rules: fields.required("math_rules")?,
            enactment: fields.required("enactment")?,
            math_compilation: fields.required("math_compilation")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for MathCompilation {
    "diagnostics" => diagnostics,
    "constraints" => constraints,
});
impl FromJson for MathCompilation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct MathCompilation")?;
        Ok(Self {
            diagnostics: fields.required("diagnostics")?,
            constraints: fields.required("constraints")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for MathConstraint {
    "iri" => iri,
    "target" => target,
    "failure_class" => failure_class,
    "projected_sparql" => projected_sparql,
});
impl FromJson for MathConstraint {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct MathConstraint")?;
        Ok(Self {
            iri: fields.required("iri")?,
            target: fields.required("target")?,
            failure_class: fields.optional("failure_class")?,
            projected_sparql: fields.required("projected_sparql")?,
        })
    }
}
