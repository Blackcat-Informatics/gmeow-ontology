// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! PyO3-free engine for the statement-metadata invariants (#630, Gap B3).
//!
//! These four checks — ported semantics-exact from
//! `src/gmeow_tools/statement_lint.py`'s `annotation_property_soundness`,
//! `base_triple_groundedness`, `base_triple_dl_datatypes`, and
//! `no_preferred_rank` — guard the canonical statement DSL before either downcast
//! is written. The Python version walked a parsed `StatementDsl` (cells) plus the
//! ontology rdflib graph; this version walks the **emitted OWL graph** from the
//! native statements stage unioned with the ontology in one oxigraph [`Store`].
//!
//! # The OWL-axiom mapping
//!
//! `emit_owl` renders each DSL cell as a named `owl:Axiom` node whose IRI is the
//! cell's reifier. So in the OWL graph a cell is reconstructed from each
//! `owl:Axiom` subject `ax`:
//!
//! * the base triple is `(ax owl:annotatedSource, ax owl:annotatedProperty,
//!   ax owl:annotatedTarget)`;
//! * the annotations are every `(ax, prop, value)` whose predicate is NOT one of
//!   `rdf:type`, `owl:annotatedSource`, `owl:annotatedProperty`,
//!   `owl:annotatedTarget`.
//!
//! The Python checks framed every diagnostic with the StatementMetadata cell IRI
//! (`cell.iri`); that IRI is not present in the emitted OWL graph (only the
//! reifier/axiom node is), so the reifier IRI is the natural identifier here. The
//! failing CONDITION and the message TEXT are otherwise reproduced exactly.
//!
//! Engine-core separation: this module imports no pyo3. The [`crate::py`]
//! bindings adapt it to Python.

use oxigraph::model::{Literal, NamedNode, NamedOrBlankNode, Term};
use std::collections::HashSet;

use oxigraph::store::Store;

use crate::model::{owl, rdf};

/// `owl:Axiom` — the reified-statement node type the OWL downcast emits.
const OWL_AXIOM: oxigraph::model::NamedNodeRef<'static> =
    oxigraph::model::NamedNodeRef::new_unchecked("http://www.w3.org/2002/07/owl#Axiom");
/// `owl:annotatedSource`.
const OWL_ANNOTATED_SOURCE: oxigraph::model::NamedNodeRef<'static> =
    oxigraph::model::NamedNodeRef::new_unchecked("http://www.w3.org/2002/07/owl#annotatedSource");
/// `owl:annotatedProperty`.
const OWL_ANNOTATED_PROPERTY: oxigraph::model::NamedNodeRef<'static> =
    oxigraph::model::NamedNodeRef::new_unchecked("http://www.w3.org/2002/07/owl#annotatedProperty");
/// `owl:annotatedTarget`.
const OWL_ANNOTATED_TARGET: oxigraph::model::NamedNodeRef<'static> =
    oxigraph::model::NamedNodeRef::new_unchecked("http://www.w3.org/2002/07/owl#annotatedTarget");
/// `rdf:Property`.
const RDF_PROPERTY: oxigraph::model::NamedNodeRef<'static> =
    oxigraph::model::NamedNodeRef::new_unchecked(
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#Property",
    );

/// The GMEOW vocabulary namespace (`config.NAMESPACE`). Hard-coded to the single
/// canonical value — the statement DSL only ever references GMEOW's own IRIs, and
/// the Python `statement_lint` imported the same `config.NAMESPACE` constant. The
/// `gmeow:confidence` predicate is built from it.
const NAMESPACE: &str = "https://blackcatinformatics.ca/gmeow/";

/// The diagnostic code stamped on every statement-invariant finding (the SARIF
/// rule id). Mirrors the `<tool>.error` convention the legacy bridge uses.
const STATEMENT_CODE: &str = "statement.invariant";

/// The OWL 2 datatype map — the only datatypes legal in a base-triple literal
/// that is a logical data-property assertion. A byte-exact port of
/// `statement_lint._OWL2_DL_DATATYPES`; notably EXCLUDES `xsd:date`, `xsd:time`,
/// `xsd:gYear*`, and `xsd:duration` (using one in the reasoned core is a DL
/// violation).
const OWL2_DL_DATATYPES: &[&str] = &[
    "http://www.w3.org/2000/01/rdf-schema#Literal",
    "http://www.w3.org/1999/02/22-rdf-syntax-ns#PlainLiteral",
    "http://www.w3.org/1999/02/22-rdf-syntax-ns#XMLLiteral",
    "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString",
    "http://www.w3.org/2002/07/owl#real",
    "http://www.w3.org/2002/07/owl#rational",
    "http://www.w3.org/2001/XMLSchema#string",
    "http://www.w3.org/2001/XMLSchema#normalizedString",
    "http://www.w3.org/2001/XMLSchema#token",
    "http://www.w3.org/2001/XMLSchema#language",
    "http://www.w3.org/2001/XMLSchema#Name",
    "http://www.w3.org/2001/XMLSchema#NCName",
    "http://www.w3.org/2001/XMLSchema#NMTOKEN",
    "http://www.w3.org/2001/XMLSchema#decimal",
    "http://www.w3.org/2001/XMLSchema#integer",
    "http://www.w3.org/2001/XMLSchema#nonNegativeInteger",
    "http://www.w3.org/2001/XMLSchema#nonPositiveInteger",
    "http://www.w3.org/2001/XMLSchema#positiveInteger",
    "http://www.w3.org/2001/XMLSchema#negativeInteger",
    "http://www.w3.org/2001/XMLSchema#long",
    "http://www.w3.org/2001/XMLSchema#int",
    "http://www.w3.org/2001/XMLSchema#short",
    "http://www.w3.org/2001/XMLSchema#byte",
    "http://www.w3.org/2001/XMLSchema#unsignedLong",
    "http://www.w3.org/2001/XMLSchema#unsignedInt",
    "http://www.w3.org/2001/XMLSchema#unsignedShort",
    "http://www.w3.org/2001/XMLSchema#unsignedByte",
    "http://www.w3.org/2001/XMLSchema#double",
    "http://www.w3.org/2001/XMLSchema#float",
    "http://www.w3.org/2001/XMLSchema#boolean",
    "http://www.w3.org/2001/XMLSchema#hexBinary",
    "http://www.w3.org/2001/XMLSchema#base64Binary",
    "http://www.w3.org/2001/XMLSchema#anyURI",
    "http://www.w3.org/2001/XMLSchema#dateTime",
    "http://www.w3.org/2001/XMLSchema#dateTimeStamp",
];

/// `rdf:type` values that count as "a declared property" (a port of
/// `statement_lint._PROPERTY_TYPES`).
fn is_property_type(iri: &str) -> bool {
    iri == owl::OBJECT_PROPERTY.as_str()
        || iri == owl::DATATYPE_PROPERTY.as_str()
        || iri == owl::ANNOTATION_PROPERTY.as_str()
        || iri == RDF_PROPERTY.as_str()
}

/// One annotation hung off a reifier: its property IRI and value term.
struct Annotation {
    prop: NamedNode,
    value: Term,
}

/// One reconstructed DSL cell: the reifier/axiom IRI plus the base triple's three
/// terms and the annotations.
struct Cell {
    /// The `owl:Axiom` node IRI (the cell's reifier). Stands in for the Python
    /// `cell.iri` in diagnostics (the StatementMetadata cell IRI is not present
    /// in the emitted OWL graph).
    reifier: String,
    source: Option<Term>,
    property: Option<Term>,
    target: Option<Term>,
    annotations: Vec<Annotation>,
}

/// Whether a term is a GMEOW *vocabulary* term (NAMESPACE + bare local name).
///
/// A byte-exact port of `statement_lint._is_gmeow_vocab_term`: instance/example
/// IRIs live under sub-paths (`…/gmeow/examples/…`, `…/gmeow/reifier/…`) and are
/// NOT vocabulary terms — only a bare local name directly under the namespace
/// (no `/` after the namespace prefix) is checked for declaration.
fn is_gmeow_vocab_term(iri: &str) -> bool {
    iri.starts_with(NAMESPACE) && !iri[NAMESPACE.len()..].contains('/')
}

/// Whether `term` is the subject of any triple in the store (`_is_declared`).
fn is_declared(store: &Store, term: &NamedNode) -> bool {
    store
        .quads_for_pattern(Some(term.as_ref().into()), None, None, None)
        .next()
        .transpose()
        .expect("declaration lookup: in-memory store query failed")
        .is_some()
}

/// All `rdf:type` object IRIs of `subject`.
fn rdf_types(store: &Store, subject: &NamedNode) -> Vec<String> {
    let mut out = Vec::new();
    for quad in store
        .quads_for_pattern(Some(subject.as_ref().into()), Some(rdf::TYPE), None, None)
        .flatten()
    {
        if let Term::NamedNode(n) = quad.object {
            out.push(n.into_string());
        }
    }
    out
}

/// Reconstruct every cell from the `owl:Axiom` nodes in the store.
///
/// The annotated-source/property/target reductions mirror rdflib `graph.value`,
/// which yields one arbitrary object when several are present; oxigraph's
/// quad-pattern iterator is likewise unordered, so the first match is taken. In a
/// well-formed `emit_owl` graph each axiom has exactly one of each.
fn collect_cells(store: &Store) -> Vec<Cell> {
    let mut cells: Vec<Cell> = Vec::new();
    for quad in store
        .quads_for_pattern(None, Some(rdf::TYPE), Some(OWL_AXIOM.into()), None)
        .flatten()
    {
        let ax = match quad.subject {
            NamedOrBlankNode::NamedNode(n) => n,
            NamedOrBlankNode::BlankNode(_) => continue,
        };
        let source = first_object(store, &ax, OWL_ANNOTATED_SOURCE);
        let property = first_object(store, &ax, OWL_ANNOTATED_PROPERTY);
        let target = first_object(store, &ax, OWL_ANNOTATED_TARGET);
        let mut annotations: Vec<Annotation> = Vec::new();
        for ann_quad in store
            .quads_for_pattern(Some(ax.as_ref().into()), None, None, None)
            .flatten()
        {
            let prop = ann_quad.predicate;
            if prop.as_ref() == rdf::TYPE
                || prop.as_ref() == OWL_ANNOTATED_SOURCE
                || prop.as_ref() == OWL_ANNOTATED_PROPERTY
                || prop.as_ref() == OWL_ANNOTATED_TARGET
            {
                continue;
            }
            annotations.push(Annotation {
                prop,
                value: ann_quad.object,
            });
        }
        cells.push(Cell {
            reifier: ax.into_string(),
            source,
            property,
            target,
            annotations,
        });
    }
    cells
}

/// The first object of `(subject, predicate, ?)`, if any.
fn first_object(
    store: &Store,
    subject: &NamedNode,
    predicate: oxigraph::model::NamedNodeRef,
) -> Option<Term> {
    store
        .quads_for_pattern(Some(subject.as_ref().into()), Some(predicate), None, None)
        .flatten()
        .next()
        .map(|q| q.object)
}

/// Run every statement invariant over the emitted-OWL-unioned-with-ontology
/// store, returning a `Finding` per violation (all `Error` severity — these
/// block statement compilation).
///
/// `store` must hold the `emit_owl` output UNIONED with the ontology in the
/// default graph.
pub fn check_statement_invariants(store: &Store) -> Vec<gmeow_diagnostics::Finding> {
    let mut messages: Vec<String> = Vec::new();
    let confidence_iri = format!("{NAMESPACE}confidence");
    let cells = collect_cells(store);

    // The Python aggregator runs the four checks in this fixed order, flattening
    // each cell's problems in DSL order; the messages are surfaced verbatim.
    for cell in &cells {
        annotation_property_soundness(store, cell, &confidence_iri, &mut messages);
    }
    for cell in &cells {
        base_triple_groundedness(store, cell, &mut messages);
    }
    for cell in &cells {
        base_triple_dl_datatypes(cell, &mut messages);
    }
    for cell in &cells {
        no_preferred_rank(cell, &mut messages);
    }

    messages
        .into_iter()
        .map(|message| {
            gmeow_diagnostics::Finding::new(
                gmeow_diagnostics::Severity::Error,
                STATEMENT_CODE,
                message,
            )
            .with_tool("statement")
        })
        .collect()
}

/// The diagnostic code for the RDF-1.2 ↔ OWL round-trip lossless check (#809).
const LOSSLESS_CODE: &str = "statement-compile.lossless-round-trip";

/// Prove the RDF 1.2 lead artifact round-trips to the OWL downcast losslessly.
///
/// `authored` is the OWL graph emitted from the statement DSL; `normalized` is
/// the RDF 1.2 lead artifact normalized back to the OWL normal form (both via the
/// `gmeow-rdf` native codec). The OWL downcast reuses each cell's reifier IRI as a
/// **named** `owl:Axiom` node — there are no blank nodes — so graph isomorphism
/// reduces to ground triple-set equality, and any asymmetry is a lossy round-trip.
///
/// The divergence is computed natively over oxigraph quad sets (RUST-FIRST)
/// instead of rdflib `graph_diff`. An
/// empty result == lossless; otherwise each diverging triple is one error finding,
/// directioned exactly as the Python emitter framed it.
pub fn check_statement_lossless(
    authored: &Store,
    normalized: &Store,
) -> Vec<gmeow_diagnostics::Finding> {
    let owl_triples = triple_set(authored);
    let rdf12_triples = triple_set(normalized);

    let mut findings = Vec::new();
    findings.extend(
        sorted_difference(&owl_triples, &rdf12_triples)
            .map(|t| lossless_finding(format!("OWL form has, RDF 1.2 lost: {t}"))),
    );
    findings.extend(
        sorted_difference(&rdf12_triples, &owl_triples)
            .map(|t| lossless_finding(format!("RDF 1.2 form has, OWL lacks: {t}"))),
    );
    findings
}

/// A single triple, kept as owned terms so set membership/difference avoids the
/// per-quad `String` allocation that formatting every matching triple would cost.
type Triple = (NamedOrBlankNode, NamedNode, Term);

/// Every triple in `store` as a `HashSet` of owned terms. Hashing the terms
/// directly (vs. an N-Triples `String` per quad) skips an allocation for the
/// matching majority; the divergent few are rendered + sorted in
/// [`sorted_difference`] so the diff order stays deterministic (P7).
fn triple_set(store: &Store) -> HashSet<Triple> {
    store
        .iter()
        .map(|quad| {
            let quad = quad.expect("statement lossless: store iteration failed");
            (quad.subject, quad.predicate, quad.object)
        })
        .collect()
}

/// The triples in `lhs` not in `rhs`, rendered `subject predicate object` and
/// **sorted** so the emitted findings are deterministic regardless of `HashSet`
/// iteration order.
fn sorted_difference<'a>(
    lhs: &'a HashSet<Triple>,
    rhs: &'a HashSet<Triple>,
) -> impl Iterator<Item = String> {
    let mut rendered: Vec<String> = lhs
        .difference(rhs)
        .map(|(s, p, o)| format!("{s} {p} {o}"))
        .collect();
    rendered.sort();
    rendered.into_iter()
}

/// Build one `statement-compile.lossless-round-trip` error finding.
fn lossless_finding(message: String) -> gmeow_diagnostics::Finding {
    gmeow_diagnostics::Finding::new(gmeow_diagnostics::Severity::Error, LOSSLESS_CODE, message)
        .with_tool("statement-compile")
}

/// Every annProperty must be an `owl:AnnotationProperty`; confidence ∈ [0, 1]
/// (mirrors `annotation_property_soundness`).
fn annotation_property_soundness(
    store: &Store,
    cell: &Cell,
    confidence_iri: &str,
    out: &mut Vec<String>,
) {
    for ann in &cell.annotations {
        let is_annotation_property = store
            .quads_for_pattern(
                Some(ann.prop.as_ref().into()),
                Some(rdf::TYPE),
                Some(owl::ANNOTATION_PROPERTY.into()),
                None,
            )
            .next()
            .transpose()
            .expect("annotation-property lookup: in-memory store query failed")
            .is_some();
        if !is_annotation_property {
            out.push(format!(
                "{cell}: annotation property {prop} is not an \
                 owl:AnnotationProperty in the ontology — the OWL downcast \
                 would not be OWL 2 DL-clean",
                cell = cell.reifier,
                prop = ann.prop.as_str(),
            ));
        }
        if ann.prop.as_str() == confidence_iri {
            confidence_problem(&cell.reifier, &ann.value, out);
        }
    }
}

/// The `gmeow:confidence` value must be a numeric literal in [0, 1] (mirrors
/// `_confidence_problem`).
fn confidence_problem(cell: &str, value: &Term, out: &mut Vec<String>) {
    let literal = match value {
        Term::Literal(lit) => lit,
        other => {
            out.push(format!(
                "{cell}: gmeow:confidence value must be a literal, got {value}",
                value = term_repr(other),
            ));
            return;
        }
    };
    // rdflib's `float(Literal)` parses the lexical form; an unparsable form is
    // the "not numeric" branch.
    match literal.value().trim().parse::<f64>() {
        Ok(number) if number.is_finite() => {
            if !(0.0..=1.0).contains(&number) {
                out.push(format!(
                    "{cell}: gmeow:confidence {number} is outside [0, 1]",
                    number = format_number(number),
                ));
            }
        }
        _ => {
            out.push(format!(
                "{cell}: gmeow:confidence value {value} is not numeric",
                value = literal_repr(literal),
            ));
        }
    }
}

/// Quoted predicates must be declared; gmeow: subjects/objects must exist
/// (mirrors `base_triple_groundedness`).
fn base_triple_groundedness(store: &Store, cell: &Cell, out: &mut Vec<String>) {
    if let Some(Term::NamedNode(predicate)) = &cell.property {
        let has_property_type = rdf_types(store, predicate)
            .iter()
            .any(|t| is_property_type(t));
        if !has_property_type {
            out.push(format!(
                "{cell}: quoted predicate {predicate} is not a declared GMEOW property",
                cell = cell.reifier,
                predicate = predicate.as_str(),
            ));
        }
    }
    for (role, term) in [("qSubject", &cell.source), ("qObject", &cell.target)] {
        if let Some(Term::NamedNode(node)) = term {
            if is_gmeow_vocab_term(node.as_str()) && !is_declared(store, node) {
                out.push(format!(
                    "{cell}: {role} {term} is a gmeow: vocabulary term \
                     but is not declared in the ontology (typo?)",
                    cell = cell.reifier,
                    term = node.as_str(),
                ));
            }
        }
    }
}

/// A literal base-triple object must use an OWL 2 datatype (mirrors
/// `base_triple_dl_datatypes`).
///
/// rdflib reports `datatype is None` for plain string and language-tagged
/// literals (so the Python check skips them); oxigraph types those as `xsd:string`
/// and `rdf:langString`, both of which are IN [`OWL2_DL_DATATYPES`], so checking
/// "datatype not in the set" is the equivalent of the Python `datatype is not None
/// and datatype not in set`.
fn base_triple_dl_datatypes(cell: &Cell, out: &mut Vec<String>) {
    if let Some(Term::Literal(lit)) = &cell.target {
        let datatype = lit.datatype().as_str().to_owned();
        if !OWL2_DL_DATATYPES.contains(&datatype.as_str()) {
            out.push(format!(
                "{cell}: quoted-object literal datatype {datatype} is \
                 not an OWL 2 datatype — the reasoned OWL downcast would not be \
                 OWL 2 DL (use xsd:dateTime, xsd:string, …)",
                cell = cell.reifier,
            ));
        }
    }
}

/// No cell may carry a preferred/primary selector annotation (Principle 9;
/// mirrors `no_preferred_rank`). The flagged annotation property's local name
/// (after the last `/` or `#`), lowercased, begins with `primary` or `preferred`.
fn no_preferred_rank(cell: &Cell, out: &mut Vec<String>) {
    for ann in &cell.annotations {
        let local = local_name(ann.prop.as_str());
        let lowered = local.to_lowercase();
        if lowered.starts_with("primary") || lowered.starts_with("preferred") {
            out.push(format!(
                "{cell}: annotation property {prop} is a \
                 preferred/primary selector — contested claims are co-equal, \
                 there is no single slot to win (Principle 9)",
                cell = cell.reifier,
                prop = ann.prop.as_str(),
            ));
        }
    }
}

/// The local name of an IRI: the part after the last `/` then after the last `#`
/// (mirrors Python `str(prop).rsplit("/", 1)[-1].rsplit("#", 1)[-1]`).
fn local_name(iri: &str) -> &str {
    let after_slash = iri.rsplit_once('/').map_or(iri, |(_, tail)| tail);
    after_slash
        .rsplit_once('#')
        .map_or(after_slash, |(_, tail)| tail)
}

/// Render a number the way Python's `str(float)` does for the values the
/// confidence check produces: an integral value drops to `N.0`, otherwise the
/// shortest round-tripping form Rust prints (which matches CPython for the simple
/// decimal confidence literals authored in the DSL).
fn format_number(number: f64) -> String {
    if number == number.trunc() && number.is_finite() {
        format!("{number:.1}")
    } else {
        format!("{number}")
    }
}

/// Render an oxigraph [`Literal`] the way rdflib's `repr(Literal)` does, for the
/// `confidence … is not numeric` message (`rdflib.term.Literal('lex')` with the
/// datatype/lang framing). Mirrors the rdflib repr the Python `{value!r}` emitted.
fn literal_repr(lit: &Literal) -> String {
    if let Some(lang) = lit.language() {
        format!(
            "rdflib.term.Literal({value}, lang={lang})",
            value = py_str_repr(lit.value()),
            lang = py_str_repr(lang),
        )
    } else {
        let datatype = lit.datatype();
        if datatype.as_str() == "http://www.w3.org/2001/XMLSchema#string" {
            format!("rdflib.term.Literal({})", py_str_repr(lit.value()))
        } else {
            format!(
                "rdflib.term.Literal({value}, datatype=rdflib.term.URIRef({dt}))",
                value = py_str_repr(lit.value()),
                dt = py_str_repr(datatype.as_str()),
            )
        }
    }
}

/// Render a non-literal annotation value the way Python `{value!r}` does for an
/// `rdflib.URIRef` / blank node (the "value must be a literal" branch).
fn term_repr(term: &Term) -> String {
    match term {
        Term::NamedNode(n) => format!("rdflib.term.URIRef({})", py_str_repr(n.as_str())),
        Term::BlankNode(b) => format!("rdflib.term.BNode({})", py_str_repr(b.as_str())),
        Term::Literal(lit) => literal_repr(lit),
        // An RDF-star triple term cannot be an annotation value in the OWL
        // downcast; render its `Display` defensively for an exhaustive match.
        other => format!("{other}"),
    }
}

/// Mirror CPython's `str.__repr__` for the lexical values the confidence/repr
/// messages frame (single-quote default; switch to double quotes if the string
/// has a single quote but no double quote; escape backslash, the active quote,
/// and `\t \n \r`). The confidence lexical forms are ASCII, so the control-escape
/// branch is exercised only defensively.
fn py_str_repr(s: &str) -> String {
    let has_single = s.contains('\'');
    let has_double = s.contains('"');
    let quote = if has_single && !has_double { '"' } else { '\'' };

    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if c.is_control() => {
                let cp = c as u32;
                if cp <= 0xff {
                    out.push_str(&format!("\\x{cp:02x}"));
                } else if cp <= 0xffff {
                    out.push_str(&format!("\\u{cp:04x}"));
                } else {
                    out.push_str(&format!("\\U{cp:08x}"));
                }
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// Native (`gmeow_rdf::RdfDataset`) twins of the statement invariant + lossless
// checks (EPIC #906). These are ADDITIVE: the oxigraph `Store`-based functions
// above stay for the PyO3 surface and other callers; the native pipeline points
// at these. Every check below reproduces its `Store` counterpart's failing
// CONDITION, message TEXT, severity, and emission ORDER byte-identically — the
// statement diagnostics feed the committed `graph/diagnostics` projection in
// `gmeow.gts`, so any drift would change committed bytes.
//
// The two query layers differ only mechanically: where the `Store` version calls
// `store.quads_for_pattern(s, p, o, None)` and matches on `oxigraph::model::Term`,
// the native version resolves term values to `TermId`s via
// `RdfDataset::term_id_by_value`, scans `quads_for_pattern(..., GraphMatch::Default)`,
// and matches on the borrowed [`gmeow_rdf::TermRef`]. The inputs are stand-alone
// Turtle/N-Triples documents whose data lands in the default graph, so the
// `GraphMatch::Default` filter is the exact native analogue of the single-graph
// `Store`.
//
// Cell collection ORDER: the `Store` version yields `owl:Axiom` subjects in
// oxigraph's quad-iteration order; the native version SORTS the collected cells by
// reifier IRI so the per-check message order is deterministic and reproducible. The
// production path (`compile_statements`) already sorts cells by reifier before
// emitting the OWL, so the diagnostics order is observable; sorting here keeps the
// native output stable and free of dataset-iteration-order coupling.

use gmeow_rdf::{DatasetView, GraphMatch, RdfDataset, TermRef, TermValue};

/// Resolve an IRI value to its dataset-local [`TermId`], if interned.
fn ds_iri_id(ds: &RdfDataset, iri: &str) -> Option<gmeow_rdf::TermId> {
    ds.term_id_by_value(&TermValue::iri(iri))
}

/// An owned, dataset-independent rendering of one resolved object term — the native
/// analogue of the `oxigraph::model::Term` arms the checks discriminate on.
#[derive(Clone, PartialEq, Eq)]
enum NativeObject {
    Iri(String),
    Blank(String),
    Literal {
        lexical: String,
        datatype: String,
        language: Option<String>,
    },
    /// A quoted triple term — cannot be an annotation value in the OWL downcast;
    /// kept for an exhaustive match.
    Triple,
}

/// Materialize a [`TermRef`] (object position) into an owned [`NativeObject`].
fn native_object(ds: &RdfDataset, term: TermRef<'_>) -> NativeObject {
    match term {
        TermRef::Iri(iri) => NativeObject::Iri(iri.to_owned()),
        TermRef::Blank { label, .. } => NativeObject::Blank(label.to_owned()),
        TermRef::Literal {
            lexical,
            datatype,
            language,
            ..
        } => {
            let dt = match ds.resolve(datatype) {
                TermRef::Iri(iri) => iri.to_owned(),
                // A literal datatype is always an IRI; defensively stringify.
                other => format!("{other:?}"),
            };
            NativeObject::Literal {
                lexical: lexical.to_owned(),
                datatype: dt,
                language: language.map(str::to_owned),
            }
        }
        TermRef::Triple { .. } => NativeObject::Triple,
    }
}

/// One reconstructed DSL cell over the native dataset (the native twin of [`Cell`]).
struct NativeCell {
    reifier: String,
    source: Option<NativeObject>,
    property: Option<NativeObject>,
    target: Option<NativeObject>,
    annotations: Vec<NativeAnnotation>,
}

/// One annotation hung off a reifier (native twin of [`Annotation`]).
struct NativeAnnotation {
    prop: String,
    value: NativeObject,
}

/// Whether `iri` is the subject of any default-graph triple (native `_is_declared`).
fn ds_is_declared(ds: &RdfDataset, iri: &str) -> bool {
    match ds_iri_id(ds, iri) {
        Some(id) => ds
            .quads_for_pattern(Some(id), None, None, GraphMatch::Default)
            .next()
            .is_some(),
        None => false,
    }
}

/// All `rdf:type` object IRIs of `subject_iri` (native twin of [`rdf_types`]).
fn ds_rdf_types(ds: &RdfDataset, subject_iri: &str) -> Vec<String> {
    let mut out = Vec::new();
    let (Some(s_id), Some(type_id)) = (
        ds_iri_id(ds, subject_iri),
        ds_iri_id(ds, rdf::TYPE.as_str()),
    ) else {
        return out;
    };
    for q in ds.quads_for_pattern(Some(s_id), Some(type_id), None, GraphMatch::Default) {
        if let TermRef::Iri(iri) = ds.resolve(q.o) {
            out.push(iri.to_owned());
        }
    }
    out
}

/// The first object of `(subject_id, predicate_iri, ?)` in the default graph.
fn ds_first_object(
    ds: &RdfDataset,
    subject_id: gmeow_rdf::TermId,
    predicate_iri: &str,
) -> Option<NativeObject> {
    let p_id = ds_iri_id(ds, predicate_iri)?;
    ds.quads_for_pattern(Some(subject_id), Some(p_id), None, GraphMatch::Default)
        .next()
        .map(|q| native_object(ds, ds.resolve(q.o)))
}

/// Reconstruct every cell from the `owl:Axiom` nodes (native twin of [`collect_cells`]).
///
/// Cells are SORTED by reifier IRI so the per-check message order is deterministic
/// and independent of dataset iteration order (the `Store` version relied on
/// oxigraph's iteration order; sorting makes the native output reproducible).
fn ds_collect_cells(ds: &RdfDataset) -> Vec<NativeCell> {
    let mut cells: Vec<NativeCell> = Vec::new();
    let (Some(type_id), Some(axiom_id)) = (
        ds_iri_id(ds, rdf::TYPE.as_str()),
        ds_iri_id(ds, OWL_AXIOM.as_str()),
    ) else {
        return cells;
    };
    // Reserved predicate IRIs that are part of the base-triple frame, not annotations.
    let reserved = [
        rdf::TYPE.as_str(),
        OWL_ANNOTATED_SOURCE.as_str(),
        OWL_ANNOTATED_PROPERTY.as_str(),
        OWL_ANNOTATED_TARGET.as_str(),
    ];
    for q in ds.quads_for_pattern(None, Some(type_id), Some(axiom_id), GraphMatch::Default) {
        let TermRef::Iri(ax) = ds.resolve(q.s) else {
            // Blank-node axiom subject: skipped, mirroring the `Store` version.
            continue;
        };
        let ax = ax.to_owned();
        let ax_id = ds.resolve(q.s);
        let TermRef::Iri(_) = ax_id else { continue };
        let ax_term_id = q.s;
        let source = ds_first_object(ds, ax_term_id, OWL_ANNOTATED_SOURCE.as_str());
        let property = ds_first_object(ds, ax_term_id, OWL_ANNOTATED_PROPERTY.as_str());
        let target = ds_first_object(ds, ax_term_id, OWL_ANNOTATED_TARGET.as_str());
        // Annotations: every (ax, prop, value) whose predicate is not a reserved one.
        // Sorted by (predicate IRI, value rendering) so the per-cell annotation
        // emission order is deterministic (the `Store` version iterated in oxigraph's
        // order; the production path emits sorted annotations, so order is observable).
        let mut annotations: Vec<NativeAnnotation> = Vec::new();
        for ann in ds.quads_for_pattern(Some(ax_term_id), None, None, GraphMatch::Default) {
            let TermRef::Iri(prop) = ds.resolve(ann.p) else {
                continue;
            };
            if reserved.contains(&prop) {
                continue;
            }
            annotations.push(NativeAnnotation {
                prop: prop.to_owned(),
                value: native_object(ds, ds.resolve(ann.o)),
            });
        }
        annotations.sort_by(|a, b| {
            (a.prop.as_str(), native_object_n3(&a.value))
                .cmp(&(b.prop.as_str(), native_object_n3(&b.value)))
        });
        cells.push(NativeCell {
            reifier: ax,
            source,
            property,
            target,
            annotations,
        });
    }
    cells.sort_by(|a, b| a.reifier.cmp(&b.reifier));
    cells
}

/// An N-Triples-style rendering of a [`NativeObject`] for sort keys (IRIs as
/// `<iri>`, literals as `"lex"`, `"lex"@lang`, or `"lex"^^<dt>`, blanks as `_:b`).
fn native_object_n3(obj: &NativeObject) -> String {
    match obj {
        NativeObject::Iri(iri) => format!("<{iri}>"),
        NativeObject::Blank(label) => format!("_:{label}"),
        NativeObject::Literal {
            lexical,
            datatype,
            language,
        } => match language {
            Some(lang) => format!("\"{lexical}\"@{lang}"),
            None => format!("\"{lexical}\"^^<{datatype}>"),
        },
        NativeObject::Triple => "<<triple>>".to_owned(),
    }
}

/// Native twin of [`check_statement_invariants`]: run every statement invariant over
/// the emitted-OWL-unioned-with-ontology dataset, returning a `Finding` per violation.
///
/// `ds` must hold the `emit_owl` output UNIONED with the ontology in the default
/// graph (the native pipeline builds it via `parse_dataset` + `RdfDataset::union`).
/// Message text, severity, and check order are byte-identical to the `Store` version.
pub fn check_statement_invariants_dataset(ds: &RdfDataset) -> Vec<gmeow_diagnostics::Finding> {
    let mut messages: Vec<String> = Vec::new();
    let confidence_iri = format!("{NAMESPACE}confidence");
    let cells = ds_collect_cells(ds);

    for cell in &cells {
        ds_annotation_property_soundness(ds, cell, &confidence_iri, &mut messages);
    }
    for cell in &cells {
        ds_base_triple_groundedness(ds, cell, &mut messages);
    }
    for cell in &cells {
        ds_base_triple_dl_datatypes(cell, &mut messages);
    }
    for cell in &cells {
        ds_no_preferred_rank(cell, &mut messages);
    }

    messages
        .into_iter()
        .map(|message| {
            gmeow_diagnostics::Finding::new(
                gmeow_diagnostics::Severity::Error,
                STATEMENT_CODE,
                message,
            )
            .with_tool("statement")
        })
        .collect()
}

/// Native twin of [`check_statement_lossless`].
///
/// `authored` is the OWL graph emitted from the statement DSL; `normalized` is the
/// RDF 1.2 lead artifact normalized back to OWL normal form. Both are blank-free
/// named `owl:Axiom` graphs, so graph isomorphism reduces to ground triple-set
/// equality. The triple rendering is identical on both sides so the set-difference
/// is correct; the divergence is sorted so findings are deterministic (P7).
pub fn check_statement_lossless_dataset(
    authored: &RdfDataset,
    normalized: &RdfDataset,
) -> Vec<gmeow_diagnostics::Finding> {
    let owl_triples = ds_triple_set(authored);
    let rdf12_triples = ds_triple_set(normalized);

    let mut findings = Vec::new();
    findings.extend(
        ds_sorted_difference(&owl_triples, &rdf12_triples)
            .map(|t| lossless_finding(format!("OWL form has, RDF 1.2 lost: {t}"))),
    );
    findings.extend(
        ds_sorted_difference(&rdf12_triples, &owl_triples)
            .map(|t| lossless_finding(format!("RDF 1.2 form has, OWL lacks: {t}"))),
    );
    findings
}

/// Every default-graph triple of `ds`, rendered `subject predicate object` in the
/// same N-Triples form both sides use (so the set-difference is well-defined).
fn ds_triple_set(ds: &RdfDataset) -> HashSet<String> {
    ds.quads_for_pattern(None, None, None, GraphMatch::Default)
        .map(|q| {
            format!(
                "{} {} {}",
                ds_term_n3(ds, q.s),
                ds_term_n3(ds, q.p),
                ds_term_n3(ds, q.o)
            )
        })
        .collect()
}

/// The triples in `lhs` not in `rhs`, sorted (deterministic regardless of `HashSet`
/// iteration order).
fn ds_sorted_difference<'a>(
    lhs: &'a HashSet<String>,
    rhs: &'a HashSet<String>,
) -> impl Iterator<Item = String> {
    let mut rendered: Vec<String> = lhs.difference(rhs).cloned().collect();
    rendered.sort();
    rendered.into_iter()
}

/// Render any resolved term to the N-Triples lexical form used by [`ds_triple_set`].
fn ds_term_n3(ds: &RdfDataset, id: gmeow_rdf::TermId) -> String {
    native_object_n3(&native_object(ds, ds.resolve(id)))
}

/// Native twin of [`annotation_property_soundness`].
fn ds_annotation_property_soundness(
    ds: &RdfDataset,
    cell: &NativeCell,
    confidence_iri: &str,
    out: &mut Vec<String>,
) {
    let type_id = ds_iri_id(ds, rdf::TYPE.as_str());
    let ann_prop_class = ds_iri_id(ds, owl::ANNOTATION_PROPERTY.as_str());
    for ann in &cell.annotations {
        let is_annotation_property = match (ds_iri_id(ds, &ann.prop), type_id, ann_prop_class) {
            (Some(p_id), Some(t_id), Some(c_id)) => ds
                .quads_for_pattern(Some(p_id), Some(t_id), Some(c_id), GraphMatch::Default)
                .next()
                .is_some(),
            _ => false,
        };
        if !is_annotation_property {
            out.push(format!(
                "{cell}: annotation property {prop} is not an \
                 owl:AnnotationProperty in the ontology — the OWL downcast \
                 would not be OWL 2 DL-clean",
                cell = cell.reifier,
                prop = ann.prop,
            ));
        }
        if ann.prop == confidence_iri {
            ds_confidence_problem(&cell.reifier, &ann.value, out);
        }
    }
}

/// Native twin of [`confidence_problem`].
fn ds_confidence_problem(cell: &str, value: &NativeObject, out: &mut Vec<String>) {
    let NativeObject::Literal {
        lexical,
        datatype,
        language,
    } = value
    else {
        out.push(format!(
            "{cell}: gmeow:confidence value must be a literal, got {value}",
            value = native_term_repr(value),
        ));
        return;
    };
    match lexical.trim().parse::<f64>() {
        Ok(number) if number.is_finite() => {
            if !(0.0..=1.0).contains(&number) {
                out.push(format!(
                    "{cell}: gmeow:confidence {number} is outside [0, 1]",
                    number = format_number(number),
                ));
            }
        }
        _ => {
            out.push(format!(
                "{cell}: gmeow:confidence value {value} is not numeric",
                value = native_literal_repr(lexical, datatype, language.as_deref()),
            ));
        }
    }
}

/// Native twin of [`base_triple_groundedness`].
fn ds_base_triple_groundedness(ds: &RdfDataset, cell: &NativeCell, out: &mut Vec<String>) {
    if let Some(NativeObject::Iri(predicate)) = &cell.property {
        let has_property_type = ds_rdf_types(ds, predicate)
            .iter()
            .any(|t| is_property_type(t));
        if !has_property_type {
            out.push(format!(
                "{cell}: quoted predicate {predicate} is not a declared GMEOW property",
                cell = cell.reifier,
            ));
        }
    }
    for (role, term) in [("qSubject", &cell.source), ("qObject", &cell.target)] {
        if let Some(NativeObject::Iri(node)) = term {
            if is_gmeow_vocab_term(node) && !ds_is_declared(ds, node) {
                out.push(format!(
                    "{cell}: {role} {term} is a gmeow: vocabulary term \
                     but is not declared in the ontology (typo?)",
                    cell = cell.reifier,
                    term = node,
                ));
            }
        }
    }
}

/// Native twin of [`base_triple_dl_datatypes`].
fn ds_base_triple_dl_datatypes(cell: &NativeCell, out: &mut Vec<String>) {
    if let Some(NativeObject::Literal { datatype, .. }) = &cell.target {
        if !OWL2_DL_DATATYPES.contains(&datatype.as_str()) {
            out.push(format!(
                "{cell}: quoted-object literal datatype {datatype} is \
                 not an OWL 2 datatype — the reasoned OWL downcast would not be \
                 OWL 2 DL (use xsd:dateTime, xsd:string, …)",
                cell = cell.reifier,
            ));
        }
    }
}

/// Native twin of [`no_preferred_rank`].
fn ds_no_preferred_rank(cell: &NativeCell, out: &mut Vec<String>) {
    for ann in &cell.annotations {
        let lowered = local_name(&ann.prop).to_lowercase();
        if lowered.starts_with("primary") || lowered.starts_with("preferred") {
            out.push(format!(
                "{cell}: annotation property {prop} is a \
                 preferred/primary selector — contested claims are co-equal, \
                 there is no single slot to win (Principle 9)",
                cell = cell.reifier,
                prop = ann.prop,
            ));
        }
    }
}

/// Render a [`NativeObject`] literal the way [`literal_repr`] does for an
/// oxigraph `Literal` (the `confidence … is not numeric` message).
fn native_literal_repr(lexical: &str, datatype: &str, language: Option<&str>) -> String {
    if let Some(lang) = language {
        format!(
            "rdflib.term.Literal({value}, lang={lang})",
            value = py_str_repr(lexical),
            lang = py_str_repr(lang),
        )
    } else if datatype == "http://www.w3.org/2001/XMLSchema#string" {
        format!("rdflib.term.Literal({})", py_str_repr(lexical))
    } else {
        format!(
            "rdflib.term.Literal({value}, datatype=rdflib.term.URIRef({dt}))",
            value = py_str_repr(lexical),
            dt = py_str_repr(datatype),
        )
    }
}

/// Render a non-literal [`NativeObject`] the way [`term_repr`] does (the "value must
/// be a literal" branch).
fn native_term_repr(obj: &NativeObject) -> String {
    match obj {
        NativeObject::Iri(iri) => format!("rdflib.term.URIRef({})", py_str_repr(iri)),
        NativeObject::Blank(label) => format!("rdflib.term.BNode({})", py_str_repr(label)),
        NativeObject::Literal {
            lexical,
            datatype,
            language,
        } => native_literal_repr(lexical, datatype, language.as_deref()),
        NativeObject::Triple => "<<triple>>".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_rdf::oxigraph::{store_from_dataset, GraphPolicy};
    use gmeow_rdf::parse_dataset;

    /// Build a store from a Turtle fixture (the native statement OWL + ontology
    /// union the production wrapper assembles).
    fn store_from(ttl: &str) -> Store {
        let dataset = parse_dataset(ttl.as_bytes(), "text/turtle", None).unwrap();
        store_from_dataset(&dataset, GraphPolicy::FlattenToDefaultGraph).unwrap()
    }

    const PREFIXES: &str = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n";

    /// A minimal ontology declaring the predicate, an annotation property, and
    /// the gmeow subject/object terms the clean fixtures reference.
    const ONTO: &str = "gmeow:knows a owl:ObjectProperty .\n\
         gmeow:confidence a owl:AnnotationProperty .\n\
         gmeow:source a owl:AnnotationProperty .\n\
         gmeow:Alice a owl:NamedIndividual .\n\
         gmeow:Bob a owl:NamedIndividual .\n";

    fn messages(ttl: &str) -> Vec<String> {
        check_statement_invariants(&store_from(ttl))
            .into_iter()
            .map(|f| f.message)
            .collect()
    }

    #[test]
    fn lossless_identical_graphs_have_no_findings() {
        let ttl = format!(
            "{PREFIXES}gmeow:Alice gmeow:knows gmeow:Bob .\n\
             <https://blackcatinformatics.ca/gmeow/reifier/x> a owl:Axiom ;\n\
               owl:annotatedSource gmeow:Alice ;\n\
               owl:annotatedProperty gmeow:knows ;\n\
               owl:annotatedTarget gmeow:Bob ;\n\
               gmeow:confidence \"0.9\"^^xsd:decimal .\n"
        );
        let findings = check_statement_lossless(&store_from(&ttl), &store_from(&ttl));
        assert!(findings.is_empty(), "identical graphs are lossless");
    }

    #[test]
    fn lossless_divergence_is_directioned() {
        let owl = format!("{PREFIXES}gmeow:Alice gmeow:knows gmeow:Bob .\n");
        let rdf12 = format!("{PREFIXES}gmeow:Alice gmeow:knows gmeow:Carol .\n");
        let findings = check_statement_lossless(&store_from(&owl), &store_from(&rdf12));

        assert_eq!(findings.len(), 2);
        assert!(findings.iter().all(|f| f.code == LOSSLESS_CODE));
        assert!(findings
            .iter()
            .any(|f| f.message.starts_with("OWL form has, RDF 1.2 lost:")
                && f.message.contains("Bob")));
        assert!(findings
            .iter()
            .any(|f| f.message.starts_with("RDF 1.2 form has, OWL lacks:")
                && f.message.contains("Carol")));
    }

    #[test]
    fn statement_clean_cell_has_no_findings() {
        let msgs = messages(&format!(
            "{PREFIXES}{ONTO}\
             gmeow:Alice gmeow:knows gmeow:Bob .\n\
             <https://blackcatinformatics.ca/gmeow/reifier/x> a owl:Axiom ;\n\
               owl:annotatedSource gmeow:Alice ;\n\
               owl:annotatedProperty gmeow:knows ;\n\
               owl:annotatedTarget gmeow:Bob ;\n\
               gmeow:confidence 0.9 ;\n\
               gmeow:source \"A reliable source.\" .\n"
        ));
        assert!(msgs.is_empty(), "expected clean, got: {msgs:?}");
    }

    #[test]
    fn statement_flags_non_annotation_property() {
        // gmeow:source is NOT declared an owl:AnnotationProperty here.
        let msgs = messages(&format!(
            "{PREFIXES}\
             gmeow:knows a owl:ObjectProperty .\n\
             gmeow:Alice a owl:NamedIndividual .\n\
             gmeow:Bob a owl:NamedIndividual .\n\
             <https://blackcatinformatics.ca/gmeow/reifier/x> a owl:Axiom ;\n\
               owl:annotatedSource gmeow:Alice ;\n\
               owl:annotatedProperty gmeow:knows ;\n\
               owl:annotatedTarget gmeow:Bob ;\n\
               gmeow:source \"A reliable source.\" .\n"
        ));
        assert!(
            msgs.iter()
                .any(|m| m.contains("is not an owl:AnnotationProperty")),
            "got: {msgs:?}"
        );
    }

    #[test]
    fn statement_flags_confidence_out_of_range() {
        let msgs = messages(&format!(
            "{PREFIXES}{ONTO}\
             <https://blackcatinformatics.ca/gmeow/reifier/x> a owl:Axiom ;\n\
               owl:annotatedSource gmeow:Alice ;\n\
               owl:annotatedProperty gmeow:knows ;\n\
               owl:annotatedTarget gmeow:Bob ;\n\
               gmeow:confidence 1.5 .\n"
        ));
        assert!(
            msgs.iter()
                .any(|m| m.contains("gmeow:confidence 1.5 is outside [0, 1]")),
            "got: {msgs:?}"
        );
    }

    #[test]
    fn statement_flags_confidence_not_numeric() {
        let msgs = messages(&format!(
            "{PREFIXES}{ONTO}\
             <https://blackcatinformatics.ca/gmeow/reifier/x> a owl:Axiom ;\n\
               owl:annotatedSource gmeow:Alice ;\n\
               owl:annotatedProperty gmeow:knows ;\n\
               owl:annotatedTarget gmeow:Bob ;\n\
               gmeow:confidence \"high\" .\n"
        ));
        assert!(
            msgs.iter()
                .any(|m| m.contains("is not numeric") && m.contains("rdflib.term.Literal('high')")),
            "got: {msgs:?}"
        );
    }

    #[test]
    fn statement_flags_undeclared_predicate() {
        let msgs = messages(&format!(
            "{PREFIXES}\
             gmeow:Alice a owl:NamedIndividual .\n\
             gmeow:Bob a owl:NamedIndividual .\n\
             <https://blackcatinformatics.ca/gmeow/reifier/x> a owl:Axiom ;\n\
               owl:annotatedSource gmeow:Alice ;\n\
               owl:annotatedProperty gmeow:undeclaredPred ;\n\
               owl:annotatedTarget gmeow:Bob .\n"
        ));
        assert!(
            msgs.iter()
                .any(|m| m.contains("is not a declared GMEOW property")),
            "got: {msgs:?}"
        );
    }

    #[test]
    fn statement_flags_undeclared_gmeow_object_term() {
        // gmeow:Ghost is a bare-local gmeow vocab term that is never a subject.
        let msgs = messages(&format!(
            "{PREFIXES}\
             gmeow:knows a owl:ObjectProperty .\n\
             gmeow:Alice a owl:NamedIndividual .\n\
             <https://blackcatinformatics.ca/gmeow/reifier/x> a owl:Axiom ;\n\
               owl:annotatedSource gmeow:Alice ;\n\
               owl:annotatedProperty gmeow:knows ;\n\
               owl:annotatedTarget gmeow:Ghost .\n"
        ));
        assert!(
            msgs.iter().any(|m| m.contains("qObject")
                && m.contains("is a gmeow: vocabulary term but is not declared")),
            "got: {msgs:?}"
        );
    }

    #[test]
    fn statement_subpath_object_term_is_not_a_vocab_term() {
        // An example/instance IRI under a sub-path is NOT a vocab term, so an
        // undeclared one must NOT be flagged (the _is_gmeow_vocab_term carve-out).
        let msgs = messages(&format!(
            "{PREFIXES}\
             gmeow:knows a owl:ObjectProperty .\n\
             gmeow:Alice a owl:NamedIndividual .\n\
             <https://blackcatinformatics.ca/gmeow/reifier/x> a owl:Axiom ;\n\
               owl:annotatedSource gmeow:Alice ;\n\
               owl:annotatedProperty gmeow:knows ;\n\
               owl:annotatedTarget <https://blackcatinformatics.ca/gmeow/examples/thing> .\n"
        ));
        assert!(
            !msgs.iter().any(|m| m.contains("vocabulary term")),
            "sub-path IRI must not be flagged: {msgs:?}"
        );
    }

    #[test]
    fn statement_flags_non_dl_datatype() {
        let msgs = messages(&format!(
            "{PREFIXES}\
             gmeow:bornOn a owl:DatatypeProperty .\n\
             gmeow:Alice a owl:NamedIndividual .\n\
             <https://blackcatinformatics.ca/gmeow/reifier/x> a owl:Axiom ;\n\
               owl:annotatedSource gmeow:Alice ;\n\
               owl:annotatedProperty gmeow:bornOn ;\n\
               owl:annotatedTarget \"2020-01-01\"^^xsd:date .\n"
        ));
        assert!(
            msgs.iter().any(|m| m
                .contains("literal datatype http://www.w3.org/2001/XMLSchema#date is")
                && m.contains("not an OWL 2 datatype")),
            "got: {msgs:?}"
        );
    }

    #[test]
    fn statement_dl_datatype_date_time_is_clean() {
        let msgs = messages(&format!(
            "{PREFIXES}\
             gmeow:bornAt a owl:DatatypeProperty .\n\
             gmeow:Alice a owl:NamedIndividual .\n\
             <https://blackcatinformatics.ca/gmeow/reifier/x> a owl:Axiom ;\n\
               owl:annotatedSource gmeow:Alice ;\n\
               owl:annotatedProperty gmeow:bornAt ;\n\
               owl:annotatedTarget \"2020-01-01T00:00:00\"^^xsd:dateTime .\n"
        ));
        assert!(
            !msgs.iter().any(|m| m.contains("not an OWL 2 datatype")),
            "xsd:dateTime is OWL 2 DL: {msgs:?}"
        );
    }

    #[test]
    fn statement_flags_preferred_rank_annotation() {
        let msgs = messages(&format!(
            "{PREFIXES}\
             gmeow:knows a owl:ObjectProperty .\n\
             gmeow:preferredRank a owl:AnnotationProperty .\n\
             gmeow:Alice a owl:NamedIndividual .\n\
             gmeow:Bob a owl:NamedIndividual .\n\
             <https://blackcatinformatics.ca/gmeow/reifier/x> a owl:Axiom ;\n\
               owl:annotatedSource gmeow:Alice ;\n\
               owl:annotatedProperty gmeow:knows ;\n\
               owl:annotatedTarget gmeow:Bob ;\n\
               gmeow:preferredRank 1 .\n"
        ));
        assert!(
            msgs.iter()
                .any(|m| m.contains("is a preferred/primary selector")),
            "got: {msgs:?}"
        );
    }

    #[test]
    fn statement_flags_primary_prefixed_annotation() {
        let msgs = messages(&format!(
            "{PREFIXES}\
             gmeow:knows a owl:ObjectProperty .\n\
             gmeow:primarySource a owl:AnnotationProperty .\n\
             gmeow:Alice a owl:NamedIndividual .\n\
             gmeow:Bob a owl:NamedIndividual .\n\
             <https://blackcatinformatics.ca/gmeow/reifier/x> a owl:Axiom ;\n\
               owl:annotatedSource gmeow:Alice ;\n\
               owl:annotatedProperty gmeow:knows ;\n\
               owl:annotatedTarget gmeow:Bob ;\n\
               gmeow:primarySource gmeow:Alice .\n"
        ));
        assert!(
            msgs.iter()
                .any(|m| m.contains("is a preferred/primary selector")),
            "got: {msgs:?}"
        );
    }

    #[test]
    fn local_name_splits_slash_and_hash() {
        assert_eq!(local_name("https://ex/path#frag"), "frag");
        assert_eq!(local_name("https://ex/path/leaf"), "leaf");
        assert_eq!(local_name("bare"), "bare");
    }

    /// Parse a Turtle fixture into a frozen native dataset (no oxigraph round-trip).
    fn dataset_from(ttl: &str) -> std::sync::Arc<gmeow_rdf::RdfDataset> {
        parse_dataset(ttl.as_bytes(), "text/turtle", None).unwrap()
    }

    /// The native invariant twin must produce byte-identical messages to the `Store`
    /// version across the full fixture battery (#906 parity).
    #[test]
    fn native_invariants_parity_with_store() {
        let fixtures = [
            // clean cell
            format!(
                "{PREFIXES}{ONTO}\
                 gmeow:Alice gmeow:knows gmeow:Bob .\n\
                 <https://blackcatinformatics.ca/gmeow/reifier/x> a owl:Axiom ;\n\
                   owl:annotatedSource gmeow:Alice ;\n\
                   owl:annotatedProperty gmeow:knows ;\n\
                   owl:annotatedTarget gmeow:Bob ;\n\
                   gmeow:confidence 0.9 ;\n\
                   gmeow:source \"A reliable source.\" .\n"
            ),
            // non-annotation property + out-of-range + not-numeric + undeclared + non-DL + preferred
            format!(
                "{PREFIXES}\
                 gmeow:knows a owl:ObjectProperty .\n\
                 gmeow:preferredRank a owl:AnnotationProperty .\n\
                 gmeow:bornOn a owl:DatatypeProperty .\n\
                 gmeow:Alice a owl:NamedIndividual .\n\
                 <https://blackcatinformatics.ca/gmeow/reifier/y> a owl:Axiom ;\n\
                   owl:annotatedSource gmeow:Alice ;\n\
                   owl:annotatedProperty gmeow:undeclaredPred ;\n\
                   owl:annotatedTarget gmeow:Ghost ;\n\
                   gmeow:confidence 1.5 ;\n\
                   gmeow:source \"s\" ;\n\
                   gmeow:preferredRank 1 .\n\
                 <https://blackcatinformatics.ca/gmeow/reifier/z> a owl:Axiom ;\n\
                   owl:annotatedSource gmeow:Alice ;\n\
                   owl:annotatedProperty gmeow:bornOn ;\n\
                   owl:annotatedTarget \"2020-01-01\"^^xsd:date ;\n\
                   gmeow:confidence \"high\" .\n"
            ),
        ];
        for ttl in &fixtures {
            let store_msgs: Vec<String> = check_statement_invariants(&store_from(ttl))
                .into_iter()
                .map(|f| f.message)
                .collect();
            let mut store_sorted = store_msgs.clone();
            store_sorted.sort();
            let native_msgs: Vec<String> = check_statement_invariants_dataset(&dataset_from(ttl))
                .into_iter()
                .map(|f| f.message)
                .collect();
            let mut native_sorted = native_msgs.clone();
            native_sorted.sort();
            // The set of diagnostics must match exactly (cell iteration order differs
            // between oxigraph and the native twin, so compare as sorted sets).
            assert_eq!(
                store_sorted, native_sorted,
                "native invariant twin diverged from Store version"
            );
        }
    }

    /// The native lossless twin must agree with the `Store` version.
    #[test]
    fn native_lossless_parity_with_store() {
        let owl = format!("{PREFIXES}gmeow:Alice gmeow:knows gmeow:Bob .\n");
        let rdf12 = format!("{PREFIXES}gmeow:Alice gmeow:knows gmeow:Carol .\n");

        let mut store_msgs: Vec<String> =
            check_statement_lossless(&store_from(&owl), &store_from(&rdf12))
                .into_iter()
                .map(|f| f.message)
                .collect();
        store_msgs.sort();
        let mut native_msgs: Vec<String> =
            check_statement_lossless_dataset(&dataset_from(&owl), &dataset_from(&rdf12))
                .into_iter()
                .map(|f| f.message)
                .collect();
        native_msgs.sort();
        assert_eq!(store_msgs, native_msgs, "lossless twin diverged");

        // Identical graphs → no findings on both.
        assert!(
            check_statement_lossless_dataset(&dataset_from(&owl), &dataset_from(&owl)).is_empty()
        );
    }
}
