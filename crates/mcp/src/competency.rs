// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native documentation bindings remain RDF terms until the MCP record exit.

use std::collections::BTreeMap;

use purrdf::{RdfLiteral, RdfTerm, SparqlResult};
use serde_json::{Value, json};

type LiteralKey = (String, String, Option<String>, Option<String>);

#[derive(Default)]
struct Question {
    fields: Option<Value>,
    rationales: BTreeMap<LiteralKey, RdfLiteral>,
}

pub(super) fn records(result: SparqlResult) -> gmeow_errors::Result<Vec<Value>> {
    let SparqlResult::Solutions {
        variables, rows, ..
    } = result
    else {
        return Err(invalid("expected native SELECT solutions"));
    };
    let mut questions: BTreeMap<String, Question> = BTreeMap::new();
    for row in rows {
        if row.len() != variables.len() {
            return Err(invalid("malformed native SELECT row width"));
        }
        let mut fields = BTreeMap::new();
        for (name, value) in variables.iter().zip(row) {
            if fields.contains_key(name.as_str()) {
                return Err(invalid("duplicate native SELECT variable"));
            }
            fields.insert(
                name.as_str(),
                value
                    .map(|term| term.to_rdf_term())
                    .transpose()
                    .map_err(|why| invalid(&format!("malformed native term: {why}")))?,
            );
        }
        let get = |name: &str| fields.get(name).and_then(Option::as_ref);
        let Some(RdfTerm::Iri(iri)) = get("c") else {
            return Err(invalid("competency ?c must be a bound IRI"));
        };
        let query = literal(get("q"), "query text")?;
        if query.datatype_iri() != "http://www.w3.org/2001/XMLSchema#string" {
            return Err(invalid("query text must be xsd:string"));
        }
        let mut object = serde_json::Map::new();
        object.insert("query_text".into(), json!(query.lexical_form));
        if let Some(term) = get("count") {
            let count = literal(Some(term), "expected row count")?;
            if count.datatype_iri() != "http://www.w3.org/2001/XMLSchema#integer" {
                return Err(invalid("expected row count must be xsd:integer"));
            }
            let count = count
                .lexical_form
                .parse::<i64>()
                .map_err(|_| invalid("expected row count must fit a signed 64-bit integer"))?;
            object.insert("expected_row_count".into(), json!(count));
        }
        if let Some(term) = get("exact") {
            let exact = literal(Some(term), "exact rows")?;
            if exact.datatype_iri() != "http://www.w3.org/2001/XMLSchema#boolean" {
                return Err(invalid("exact rows must be xsd:boolean"));
            }
            let exact = match exact.lexical_form.as_str() {
                "true" | "1" => true,
                "false" | "0" => false,
                _ => return Err(invalid("invalid xsd:boolean exact rows")),
            };
            object.insert("exact_rows".into(), json!(exact));
        }
        let question = questions.entry(iri.clone()).or_default();
        let object = Value::Object(object);
        if question
            .fields
            .as_ref()
            .is_some_and(|previous| previous != &object)
        {
            return Err(invalid(&format!(
                "<{iri}> has conflicting query or expectation fields"
            )));
        }
        question.fields = Some(object);
        if let Some(term) = get("rationale") {
            let value = literal(Some(term), "rationale")?;
            let key = (
                value.lexical_form.clone(),
                value.datatype_iri().to_owned(),
                value.language.clone(),
                value
                    .direction
                    .map(|direction| direction.as_str().to_owned()),
            );
            question.rationales.insert(key, value.clone());
        }
    }
    questions
        .into_values()
        .map(|question| {
            let Some(Value::Object(mut object)) = question.fields else {
                return Err(invalid("competency has no query fields"));
            };
            let rationales: Vec<_> = question
                .rationales
                .into_values()
                .map(|literal| {
                    json!({
                        "lexical_form": literal.lexical_form,
                        "datatype": literal.datatype_iri(),
                        "language": literal.language,
                        "direction": literal.direction.map(|direction| direction.as_str()),
                    })
                })
                .collect();
            object.insert("rationales".into(), Value::Array(rationales));
            Ok(Value::Object(object))
        })
        .collect()
}

fn literal<'a>(term: Option<&'a RdfTerm>, field: &str) -> gmeow_errors::Result<&'a RdfLiteral> {
    let Some(RdfTerm::Literal(literal)) = term else {
        return Err(invalid(&format!("{field} must be a bound RDF literal")));
    };
    RdfLiteral::validate_components(
        literal.datatype_iri(),
        literal.language.as_deref(),
        literal.direction,
    )
    .map_err(|why| invalid(&format!("malformed {field}: {why}")))?;
    Ok(literal)
}

fn invalid(message: &str) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Mcp {
        message: format!("competency_questions: {message}"),
    })
}

#[cfg(test)]
mod tests;
