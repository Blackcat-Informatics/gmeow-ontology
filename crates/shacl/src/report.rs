// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

//! SHACL validation report types and serialization.
//!
//! [`ValidationReport`] is the in-memory representation of a SHACL report
//! graph. `to_ntriples()` emits a canonical N-Triples serialization using
//! oxigraph's own serializer (avoiding hand-rolled literal escaping).
//! `tuples_from_ntriples()` round-trips back to the same tuple set for testing.

use std::collections::BTreeSet;

use oxigraph::io::RdfFormat;
use oxigraph::model::{
    BlankNode, GraphName, Literal, NamedNode, NamedNodeRef, NamedOrBlankNode, Quad, Term,
};
use oxigraph::store::Store;

use crate::model::{rdf, sh, xsd};

// ── Severity ──────────────────────────────────────────────────────────────────

/// SHACL result severity levels, ordered from most to least severe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// `sh:Violation` — the most severe level.
    Violation,
    /// `sh:Warning`.
    Warning,
    /// `sh:Info` — the least severe level.
    Info,
}

impl Severity {
    /// The `sh:` IRI string for this severity level.
    pub fn iri(&self) -> &'static str {
        match self {
            Severity::Violation => sh::VIOLATION.as_str(),
            Severity::Warning => sh::WARNING.as_str(),
            Severity::Info => sh::INFO.as_str(),
        }
    }

    /// Parse a severity from its IRI string, returning `None` if unrecognised.
    pub fn from_iri(s: &str) -> Option<Severity> {
        match s {
            "http://www.w3.org/ns/shacl#Violation" => Some(Severity::Violation),
            "http://www.w3.org/ns/shacl#Warning" => Some(Severity::Warning),
            "http://www.w3.org/ns/shacl#Info" => Some(Severity::Info),
            _ => None,
        }
    }
}

// ── ValidationResult ─────────────────────────────────────────────────────────

/// A single SHACL validation result (`sh:ValidationResult`).
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// The focus node that violated the constraint.
    pub focus_node: Term,
    /// The result path, if the violation is path-scoped.
    pub result_path: Option<Term>,
    /// The offending value at the focus node, if applicable.
    pub value: Option<Term>,
    /// The constraint component that produced this result.
    pub source_constraint_component: NamedNode,
    /// The shape that sourced this result.
    pub source_shape: Term,
    /// The severity of this result.
    pub severity: Severity,
    /// An optional human-readable message.
    pub message: Option<String>,
}

// ── ValidationReport ─────────────────────────────────────────────────────────

/// A SHACL validation report (`sh:ValidationReport`).
#[derive(Debug, Clone)]
pub struct ValidationReport {
    /// Whether the data graph conforms to the shapes graph.
    pub conforms: bool,
    /// Individual violation/warning/info results.
    pub results: Vec<ValidationResult>,
}

/// The tuple type used for deterministic comparison of result sets.
///
/// `(focus, path, value, component, source_shape, severity)`
pub type ResultTuple = (
    String,
    Option<String>,
    Option<String>,
    String,
    String,
    Severity,
);

impl ValidationReport {
    /// Emit the report as canonical N-Triples text using oxigraph's serializer.
    ///
    /// The report is built into an in-memory [`Store`] as quads in the default
    /// graph, then serialised with `RdfFormat::NTriples`. This avoids
    /// hand-rolling literal escaping and ensures oxigraph-canonical output.
    pub fn to_ntriples(&self) -> String {
        let store = Store::new().expect("in-memory store creation is infallible");

        let report_bnode = BlankNode::new_unchecked("report");
        let report_subj = NamedOrBlankNode::BlankNode(report_bnode);

        // _:report rdf:type sh:ValidationReport
        insert_triple(
            &store,
            report_subj.clone(),
            rdf::TYPE,
            Term::NamedNode(sh::VALIDATION_REPORT.into()),
        );

        // _:report sh:conforms "true"^^xsd:boolean (or false)
        let conforms_lit = Literal::new_typed_literal(
            if self.conforms { "true" } else { "false" },
            NamedNode::new_unchecked(xsd::BOOLEAN),
        );
        insert_triple(
            &store,
            report_subj.clone(),
            sh::CONFORMS,
            Term::Literal(conforms_lit),
        );

        for (i, r) in self.results.iter().enumerate() {
            let result_id = format!("r{i}");
            let result_bnode = BlankNode::new_unchecked(result_id);
            let result_subj = NamedOrBlankNode::BlankNode(result_bnode.clone());
            let result_term = Term::BlankNode(result_bnode);

            // _:report sh:result _:r{i}
            insert_triple(&store, report_subj.clone(), sh::RESULT, result_term);

            // _:r{i} rdf:type sh:ValidationResult
            insert_triple(
                &store,
                result_subj.clone(),
                rdf::TYPE,
                Term::NamedNode(sh::VALIDATION_RESULT.into()),
            );

            // sh:focusNode
            insert_triple(
                &store,
                result_subj.clone(),
                sh::FOCUS_NODE,
                r.focus_node.clone(),
            );

            // sh:resultSeverity
            insert_triple(
                &store,
                result_subj.clone(),
                sh::RESULT_SEVERITY,
                Term::NamedNode(NamedNode::new_unchecked(r.severity.iri())),
            );

            // sh:sourceConstraintComponent
            insert_triple(
                &store,
                result_subj.clone(),
                sh::SOURCE_CONSTRAINT_COMPONENT,
                Term::NamedNode(r.source_constraint_component.clone()),
            );

            // sh:sourceShape
            insert_triple(
                &store,
                result_subj.clone(),
                sh::SOURCE_SHAPE,
                r.source_shape.clone(),
            );

            // sh:resultPath (optional)
            if let Some(path) = &r.result_path {
                insert_triple(&store, result_subj.clone(), sh::RESULT_PATH, path.clone());
            }

            // sh:value (optional)
            if let Some(value) = &r.value {
                insert_triple(&store, result_subj.clone(), sh::VALUE, value.clone());
            }

            // sh:resultMessage (optional plain string literal)
            if let Some(msg) = &r.message {
                insert_triple(
                    &store,
                    result_subj.clone(),
                    sh::RESULT_MESSAGE,
                    Term::Literal(Literal::new_simple_literal(msg.as_str())),
                );
            }
        }

        let mut buf: Vec<u8> = Vec::new();
        store
            .dump_graph_to_writer(
                oxigraph::model::GraphNameRef::DefaultGraph,
                RdfFormat::NTriples,
                &mut buf,
            )
            .expect("N-Triples serialisation into Vec<u8> is infallible");
        String::from_utf8(buf).expect("oxigraph N-Triples output is valid UTF-8")
    }

    /// Return the result set as a [`BTreeSet`] of [`ResultTuple`]s for
    /// deterministic equality comparison in tests and conformance checks.
    pub fn result_tuples(&self) -> BTreeSet<ResultTuple> {
        self.results
            .iter()
            .map(|r| {
                (
                    r.focus_node.to_string(),
                    r.result_path.as_ref().map(|t| t.to_string()),
                    r.value.as_ref().map(|t| t.to_string()),
                    r.source_constraint_component.to_string(),
                    r.source_shape.to_string(),
                    r.severity,
                )
            })
            .collect()
    }
}

// ── Store helpers ─────────────────────────────────────────────────────────────

/// Insert a triple into the default graph of `store`.
fn insert_triple(
    store: &Store,
    subject: NamedOrBlankNode,
    predicate: NamedNodeRef<'_>,
    object: Term,
) {
    store
        .insert(&Quad::new(
            subject,
            NamedNode::from(predicate),
            object,
            GraphName::DefaultGraph,
        ))
        .expect("in-memory store insert is infallible");
}

// ── Round-trip helpers ────────────────────────────────────────────────────────

/// Extract a `BTreeSet<ResultTuple>` from an N-Triples SHACL report string.
///
/// Loads the N-Triples into an in-memory store and delegates to
/// [`tuples_from_store`].
///
/// # Errors
///
/// Returns an error string if the N-Triples cannot be parsed.
pub fn tuples_from_ntriples(nt: &str) -> Result<BTreeSet<ResultTuple>, String> {
    let store = Store::new().map_err(|e| format!("store creation failed: {e}"))?;
    store
        .load_from_reader(RdfFormat::NTriples, nt.as_bytes())
        .map_err(|e| format!("N-Triples parse error: {e}"))?;
    Ok(tuples_from_store(&store))
}

/// Walk a SHACL report store and extract result tuples.
///
/// Finds all `?r rdf:type sh:ValidationResult` nodes and reads their
/// mandatory and optional predicates, building the same tuple shape as
/// [`ValidationReport::result_tuples`].
pub fn tuples_from_store(store: &Store) -> BTreeSet<ResultTuple> {
    let vr_type = Term::NamedNode(sh::VALIDATION_RESULT.into());

    // Collect all result blank/named nodes.
    let result_nodes: Vec<Term> = store
        .quads_for_pattern(None, Some(rdf::TYPE), Some(vr_type.as_ref()), None)
        .filter_map(|q| q.ok().map(|q| Term::from(q.subject)))
        .collect();

    let mut tuples = BTreeSet::new();

    for result_node in result_nodes {
        let subj_ref = term_as_named_or_blank_ref(&result_node);

        let focus = object_as_string(store, subj_ref, sh::FOCUS_NODE).unwrap_or_default();
        let path = object_as_string(store, subj_ref, sh::RESULT_PATH);
        let value = object_as_string(store, subj_ref, sh::VALUE);
        let component =
            object_as_string(store, subj_ref, sh::SOURCE_CONSTRAINT_COMPONENT).unwrap_or_default();
        let source_shape = object_as_string(store, subj_ref, sh::SOURCE_SHAPE).unwrap_or_default();
        let severity_iri =
            object_as_string(store, subj_ref, sh::RESULT_SEVERITY).unwrap_or_default();

        // Parse severity from the IRI string (strip angle brackets oxigraph adds
        // when you call Term::to_string() on a NamedNode).
        let sev_str = severity_iri.trim_matches(|c| c == '<' || c == '>');
        let severity = Severity::from_iri(sev_str).unwrap_or(Severity::Violation);

        tuples.insert((focus, path, value, component, source_shape, severity));
    }

    tuples
}

/// Extract the `sh:conforms` boolean from a report store, if present.
pub fn conforms_from_store(store: &Store) -> Option<bool> {
    let report_type = Term::NamedNode(sh::VALIDATION_REPORT.into());
    let report_node = store
        .quads_for_pattern(None, Some(rdf::TYPE), Some(report_type.as_ref()), None)
        .find_map(|q| q.ok().map(|q| Term::from(q.subject)))?;

    let subj_ref = term_as_named_or_blank_ref(&report_node);
    let raw = object_as_string(store, subj_ref, sh::CONFORMS)?;
    // oxigraph serialises boolean literals as `"true"^^<xsd:boolean>`
    match raw.as_str() {
        s if s.starts_with("\"true\"") => Some(true),
        s if s.starts_with("\"false\"") => Some(false),
        _ => None,
    }
}

// ── Internal query helpers ────────────────────────────────────────────────────

/// Return the first object of `(subj, pred, ?)` as a `Term::to_string()` string.
fn object_as_string(
    store: &Store,
    subj: oxigraph::model::NamedOrBlankNodeRef<'_>,
    pred: NamedNodeRef<'_>,
) -> Option<String> {
    store
        .quads_for_pattern(Some(subj), Some(pred), None, None)
        .find_map(|q| q.ok().map(|q| q.object.to_string()))
}

/// Borrow a `Term` as a `NamedOrBlankNodeRef` for use in store queries.
///
/// # Panics
///
/// Panics if `term` is a `Literal` or `Triple` — only subject positions are
/// legal for SHACL report nodes.
fn term_as_named_or_blank_ref(term: &Term) -> oxigraph::model::NamedOrBlankNodeRef<'_> {
    match term {
        Term::NamedNode(n) => oxigraph::model::NamedOrBlankNodeRef::NamedNode(n.as_ref()),
        Term::BlankNode(b) => oxigraph::model::NamedOrBlankNodeRef::BlankNode(b.as_ref()),
        other => panic!("expected named or blank node, got {other:?}"),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_result() -> ValidationResult {
        ValidationResult {
            focus_node: Term::NamedNode(NamedNode::new_unchecked("http://example.org/focusA")),
            result_path: Some(Term::NamedNode(NamedNode::new_unchecked(
                "http://example.org/predP",
            ))),
            value: Some(Term::Literal(Literal::new_simple_literal("bad value"))),
            source_constraint_component: NamedNode::new_unchecked(
                "http://www.w3.org/ns/shacl#MinCountConstraintComponent",
            ),
            source_shape: Term::NamedNode(NamedNode::new_unchecked("http://example.org/ShapeA")),
            severity: Severity::Violation,
            message: Some("must have at least one value".to_owned()),
        }
    }

    #[test]
    fn report_round_trip_with_one_result() {
        let report = ValidationReport {
            conforms: false,
            results: vec![make_result()],
        };

        let nt = report.to_ntriples();
        assert!(!nt.is_empty(), "N-Triples output must not be empty");

        let parsed =
            tuples_from_ntriples(&nt).expect("N-Triples from to_ntriples() must parse cleanly");
        let expected = report.result_tuples();

        assert_eq!(
            parsed, expected,
            "round-trip tuples must match original tuples"
        );
    }

    #[test]
    fn empty_conforming_report_round_trips() {
        let report = ValidationReport {
            conforms: true,
            results: vec![],
        };

        let nt = report.to_ntriples();

        // conforms=true must appear in the N-Triples
        assert!(
            nt.contains("true"),
            "N-Triples must contain 'true' for sh:conforms"
        );

        let parsed =
            tuples_from_ntriples(&nt).expect("N-Triples from empty report must parse cleanly");
        assert!(parsed.is_empty(), "empty report must produce zero tuples");

        // Check conforms_from_store directly
        let store = Store::new().unwrap();
        store
            .load_from_reader(RdfFormat::NTriples, nt.as_bytes())
            .unwrap();
        assert_eq!(conforms_from_store(&store), Some(true));
    }

    #[test]
    fn severity_iri_round_trip() {
        for sev in [Severity::Violation, Severity::Warning, Severity::Info] {
            let iri = sev.iri();
            let parsed = Severity::from_iri(iri);
            assert_eq!(
                parsed,
                Some(sev),
                "from_iri(iri()) must round-trip for {sev:?}"
            );
        }
    }

    #[test]
    fn severity_from_iri_unknown_returns_none() {
        assert!(Severity::from_iri("http://example.org/Unknown").is_none());
    }
}
