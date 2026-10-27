// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native observation transport; decoding does not repeat lowering or scheduling.

use super::{FormulaLoweringInspection, RuleInspection};
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record, items_with},
};

::purrdf_lex::json_record!(impl ToJson for FormulaLoweringInspection {
    "rules" => rules,
    "existential_rules" => existential_rules,
    "preservation" => preservation,
});
impl FromJson for FormulaLoweringInspection {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct FormulaLoweringInspection")?;
        Ok(Self {
            rules: fields.required("rules")?,
            existential_rules: fields.required("existential_rules")?,
            preservation: fields.required("preservation")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for RuleInspection {
    "numeric" => numeric,
    "rule_iri" => rule_iri,
    "head" => head,
    "body" => body,
    "distinct_pairs" => distinct_pairs,
});
impl FromJson for RuleInspection {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct RuleInspection")?;
        Ok(Self {
            numeric: fields.required("numeric")?,
            rule_iri: fields.required("rule_iri")?,
            head: fields.required("head")?,
            body: fields.required("body")?,
            distinct_pairs: fields.required_with("distinct_pairs", |value| {
                items_with(value, |row| -> Result<_, DecodeError> {
                    let [left, right] = gmeow_errors::json::read_string_row(row)?;
                    Ok((left, right))
                })
            })?,
        })
    }
}
