// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Conversions from the lexical [`gmeow_sparql_algebra`] term types to the
//! dataset-independent [`TermValue`] lookup/build key.
//!
//! The algebra carries terms lexically (an IRI string, a literal's lexical form +
//! datatype IRI); the IR keys term identity on [`TermValue`]. These helpers bridge
//! the two and apply the one normalization the IR's C0.1 literal-identity contract
//! requires at the lookup boundary: a language tag is lowercased so a query literal
//! matches the dataset's interned (already-lowercased) form.

use gmeow_rdf_core::{RdfTextDirection, TermValue};
use gmeow_sparql_algebra::{BaseDirection, Literal, NamedNode, TermPattern, TriplePattern};

use crate::error::EvalError;

/// Map the algebra's RDF-1.2 base direction to the IR's.
#[inline]
pub(crate) fn map_direction(direction: Option<BaseDirection>) -> Option<RdfTextDirection> {
    direction.map(|d| match d {
        BaseDirection::Ltr => RdfTextDirection::Ltr,
        BaseDirection::Rtl => RdfTextDirection::Rtl,
    })
}

/// An IRI term value.
#[inline]
pub(crate) fn named_node_to_value(node: &NamedNode) -> TermValue {
    TermValue::Iri(node.as_str().to_owned())
}

/// A literal term value, with the language tag lowercased to match the IR's C0.1
/// interned identity (so a query literal resolves to the dataset's stored form).
pub(crate) fn literal_to_value(lit: &Literal) -> TermValue {
    TermValue::Literal {
        lexical_form: lit.value().to_owned(),
        datatype: lit.datatype().as_str().to_owned(),
        language: lit.language().map(str::to_ascii_lowercase),
        direction: map_direction(lit.direction()),
    }
}

/// Convert a **ground** quoted-triple pattern to a [`TermValue::Triple`].
///
/// Returns [`EvalError::Unsupported`] if any component is a variable: matching a
/// quoted triple term whose components *bind* variables (structural triple-term
/// matching) is out of the current S6 BGP scope; only fully-ground quoted triples
/// resolve to a single interned id.
pub(crate) fn ground_triple_pattern_to_value(
    pattern: &TriplePattern,
) -> Result<TermValue, EvalError> {
    let s = ground_term_pattern_to_value(&pattern.subject)?;
    let p = match &pattern.predicate {
        gmeow_sparql_algebra::NamedNodePattern::NamedNode(n) => named_node_to_value(n),
        gmeow_sparql_algebra::NamedNodePattern::Variable(_) => {
            return Err(EvalError::unsupported(
                "variable predicate inside a quoted triple term in a BGP",
            ))
        }
    };
    let o = ground_term_pattern_to_value(&pattern.object)?;
    Ok(TermValue::Triple {
        s: Box::new(s),
        p: Box::new(p),
        o: Box::new(o),
    })
}

/// Convert a **ground** term pattern (no variables) to a [`TermValue`].
///
/// A variable in a quoted-triple component is [`EvalError::Unsupported`] (see
/// [`ground_triple_pattern_to_value`]).
pub(crate) fn ground_term_pattern_to_value(pattern: &TermPattern) -> Result<TermValue, EvalError> {
    match pattern {
        TermPattern::NamedNode(n) => Ok(named_node_to_value(n)),
        TermPattern::BlankNode(b) => Ok(TermValue::Blank {
            label: b.as_str().to_owned(),
            scope: gmeow_rdf_core::BlankScope::DEFAULT,
        }),
        TermPattern::Literal(l) => Ok(literal_to_value(l)),
        TermPattern::Triple(t) => ground_triple_pattern_to_value(t),
        TermPattern::Variable(_) => Err(EvalError::unsupported(
            "variable inside a quoted triple term in a BGP",
        )),
    }
}
