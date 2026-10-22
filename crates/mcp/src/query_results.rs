// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! MCP metadata around the native SPARQL result transport. RDF terms are
//! serialized only by PurRDF, including direction, triple terms and scoped blanks.

use std::collections::BTreeSet;

use gmeow_errors::{Diag, ResultExt};
use purrdf::SparqlResult;
use purrdf_lex::json::{Object, Value, record::read_document};

/// Preserve the MCP form envelope around native SRJ bindings/booleans or the
/// native N-Quads graph. The query API supplies no ResultProvenance carrier;
/// an empty carrier states that fact without fabricating proof or source IDs.
/// Native writer/reader errors and malformed result dimensions fail the exit.
pub(super) fn envelope(result: SparqlResult) -> gmeow_errors::Result<Value> {
    if let SparqlResult::Graph(graph) = result {
        let bytes = purrdf::serialize_dataset(
            graph.as_ref(),
            "application/n-quads",
            purrdf::SerializeGraph::Dataset,
        )
        .with_ctx(|| "serialize MCP CONSTRUCT/DESCRIBE graph result".to_owned())?;
        let nquads = String::from_utf8(bytes)
            .with_ctx(|| "read native MCP graph result as UTF-8".to_owned())?;
        let mut object = Object::new();
        object.insert("form", "graph");
        object.insert("graph_nquads", nquads);
        object.insert("ok", true);
        object.insert("quad_count", graph.quad_count());
        return Ok(Value::Object(object));
    }
    if let SparqlResult::Solutions {
        variables, rows, ..
    } = &result
    {
        let unique: BTreeSet<_> = variables.iter().collect();
        if unique.len() != variables.len() {
            return Err(Diag::of_kind(crate::error::Mcp {
                message: "native SPARQL result has duplicate variable headers".to_owned(),
            }));
        }
        for (index, row) in rows.iter().enumerate() {
            if row.len() != variables.len() {
                return Err(Diag::of_kind(crate::error::Mcp {
                    message: format!(
                        "native SPARQL result row {index} has {} cells for {} variable headers",
                        row.len(),
                        variables.len()
                    ),
                }));
            }
        }
    }
    let outcome =
        purrdf::sparql::to_json(&result, &purrdf::sparql::ResultProvenance::default(), None)
            .with_ctx(|| "serialize native MCP SPARQL results".to_owned())?;
    if outcome.provenance_dropped {
        return Err(Diag::of_kind(crate::error::Mcp {
            message: "native MCP SPARQL results unexpectedly dropped provenance".to_owned(),
        }));
    }
    // Same-native envelope composition: no RDF term decoder or Serde bridge.
    let mut value = read_document(&outcome.bytes)
        .with_ctx(|| "read native MCP SPARQL result envelope".to_owned())?;
    let Some(object) = value.as_object_mut() else {
        return Err(Diag::of_kind(crate::error::Mcp {
            message: "native SPARQL result writer did not return an object".to_owned(),
        }));
    };
    let form = if matches!(result, SparqlResult::Boolean(_)) {
        // MCP's boolean form carries no head, unlike the standalone SRJ document.
        object.remove("head");
        "boolean"
    } else {
        "bindings"
    };
    object.insert("form", form);
    object.insert("ok", true);
    object.sort_keys();
    Ok(value)
}

#[cfg(test)]
mod tests;
