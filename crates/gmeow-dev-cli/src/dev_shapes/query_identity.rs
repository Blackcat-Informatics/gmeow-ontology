// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Conservative identity witnesses for raw SPARQL shape retirement.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_errors::Diag;
use purrdf::RdfDataset;
use purrdf::shapes::shapes::{AnnotatedConstraint, Constraint, Target};
use purrdf::sparql::lexer::{Token, tokenize};
use purrdf::sparql::{Query, SparqlParser, try_pattern_to_select_query};

/// Token boundaries and prologue fields remain separate, so literal contents cannot
/// collide with syntax or another query's framing.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct QueryKey {
    dataset: String,
    base: Option<String>,
    version: Option<String>,
    tokens: Vec<IdentityToken>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum IdentityToken {
    Observed(String),
    Variable(usize),
    Syntax(String),
}

#[cfg(test)]
pub(super) fn sparql_alpha_canonical(select: &str) -> Result<QueryKey, Diag> {
    query_key(select, false)
}

fn query_key(select: &str, preserve_variables: bool) -> Result<QueryKey, Diag> {
    let query = SparqlParser::new()
        .parse_query(select)
        .map_err(Diag::from)?;
    if !matches!(query, Query::Select { .. }) {
        return Err(crate::error::clearance("identity requires a SELECT query"));
    }
    let rendered = try_pattern_to_select_query(query.pattern()).map_err(Diag::from)?;
    let mut observed: BTreeSet<String> = query
        .pattern()
        .projected_names()
        .into_iter()
        .flat_map(|names| names.iter())
        .map(|variable| variable.as_str().to_owned())
        .chain(["this", "path", "value", "message"].map(str::to_owned))
        .collect();
    let tokens = tokenize(&rendered).map_err(Diag::from)?;
    let preserve_variables = preserve_variables
        || tokens
            .iter()
            .take_while(|token| token.token != Token::LBrace)
            .any(|token| token.token == Token::Star);
    // Solution modifiers may wrap the root Project in the native algebra.
    // Preserve the serialized SELECT header's public binding names as well.
    for token in tokens
        .iter()
        .take_while(|token| token.token != Token::LBrace)
    {
        if let Token::Variable(name) = token.token {
            observed.insert(name.to_owned());
        }
    }
    let mut variables = BTreeMap::new();
    let mut next = 0;
    let mut identity = Vec::new();
    for token in tokens {
        match token.token {
            Token::Variable("currentShape" | "shapesGraph") => {
                return Err(crate::error::clearance(
                    "shape-dependent query bindings require a separate equivalence proof",
                ));
            }
            Token::Variable(name) if preserve_variables || observed.contains(name) => {
                identity.push(IdentityToken::Observed(name.to_owned()));
            }
            Token::Variable(name) | Token::BlankNodeLabel(name) => {
                let blank = matches!(token.token, Token::BlankNodeLabel(_));
                let index = variables
                    .entry((blank, name.to_owned()))
                    .or_insert_with(|| {
                        let index = next;
                        next += 1;
                        index
                    });
                identity.push(IdentityToken::Variable(*index));
            }
            Token::Anon => {
                identity.push(IdentityToken::Variable(next));
                next += 1;
            }
            // Native tokens retain decoded literal content and its kind. Debug is
            // used only as an in-process equality key, never a wire representation.
            token => identity.push(IdentityToken::Syntax(format!("{token:?}"))),
        }
    }
    Ok(QueryKey {
        dataset: format!("{:?}", query.dataset()),
        base: query.base_iri().map(|iri| iri.as_str().to_owned()),
        version: query.version().map(|version| version.raw().to_owned()),
        tokens: identity,
    })
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ConstraintKey {
    query: QueryKey,
    severity: String,
    messages: Vec<String>,
    annotations: String,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct BlockKey {
    target: QueryKey,
    constraints: Vec<ConstraintKey>,
    deactivated: bool,
    box_roles: String,
}

/// Compare the native effective constraint contract, not merely its SELECT text.
/// Unsupported target/constraint forms remain residue for another proof path.
pub(super) fn raw_sparql_block_key(
    dataset: &RdfDataset,
    iri: &str,
) -> Result<Option<BlockKey>, Diag> {
    // Preserve the complete native statement layer, named prefix declarations
    // and directional literals. The witness extractor intentionally covers a
    // smaller fragment and must not decide this report-level identity claim.
    let bytes = purrdf::serialize_dataset(dataset, "text/turtle", purrdf::SerializeGraph::Dataset)
        .map_err(Diag::from)?;
    let turtle = String::from_utf8(bytes).map_err(Diag::from)?;
    let shapes = purrdf::shapes::engine::parse_shapes(&turtle, None).map_err(Diag::from)?;
    let Some(shape) = shapes.node_shapes.iter().find(|shape| {
        matches!(&shape.id, purrdf::shapes::term::Term::NamedNode(node) if node.as_str() == iri)
    }) else {
        return Err(crate::error::clearance(format!("missing native shape {iri}")));
    };
    let [
        Target::Sparql {
            select,
            ask: None,
            substitutions,
        },
    ] = shape.targets.as_slice()
    else {
        return Ok(None);
    };
    if !substitutions.is_empty()
        || !shape.property_shapes.is_empty()
        || !shape.rules.is_empty()
        || shape.constraints.is_empty()
    {
        return Ok(None);
    }
    let mut constraints = Vec::new();
    for (index, constraint) in shape.constraints.iter().enumerate() {
        let Constraint::Sparql {
            select,
            messages,
            severity,
            annotations,
        } = constraint
        else {
            return Ok(None);
        };
        let override_ = shape
            .constraint_annotations
            .iter()
            .find(|annotation| annotation.constraint == AnnotatedConstraint::Constraint(index));
        let severity = override_
            .and_then(|annotation| annotation.severity.as_ref())
            .or(severity.as_ref())
            .unwrap_or(&shape.severity);
        let messages = override_
            .map(|annotation| annotation.messages.as_slice())
            .filter(|messages| !messages.is_empty())
            .unwrap_or_else(|| {
                if messages.is_empty() {
                    &shape.messages
                } else {
                    messages
                }
            });
        // Message templates and result annotations can observe otherwise local
        // variable names. Keep those names rather than erasing that dependency.
        let preserve_variables =
            !annotations.is_empty() || messages.iter().any(|message| message.value().contains('{'));
        constraints.push(ConstraintKey {
            query: query_key(select, preserve_variables)?,
            severity: format!("{severity:?}"),
            messages: messages
                .iter()
                .map(|message| purrdf::shapes::term::Term::Literal(message.clone()).to_string())
                .collect(),
            annotations: format!("{annotations:?}"),
        });
    }
    constraints.sort();
    Ok(Some(BlockKey {
        target: query_key(select, false)?,
        constraints,
        deactivated: shape.deactivated,
        box_roles: format!("{:?}", shape.box_roles),
    }))
}
