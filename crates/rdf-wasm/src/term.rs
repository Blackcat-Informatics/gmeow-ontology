// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! RDF/JS `Term` and `Quad` types over the owned [`gmeow_rdf`] model.
//!
//! The [RDF/JS](https://rdf.js.org/data-model-spec/) data model is by-value: a term is
//! a plain object with a `termType` discriminator, a `value` string, and structural
//! `equals`. We wrap the engine's owned [`RdfTerm`]/[`RdfQuad`] (NOT the interned IR id
//! space — JS objects own their data), extended with the two RDF/JS term kinds the RDF
//! data model lacks (`Variable`, `DefaultGraph`).
//!
//! The **RDF-1.2 wedge** lives here: a quoted-triple term (`RdfTerm::Triple`) surfaces
//! as `termType: "Quad"` with `subject`/`predicate`/`object` accessors, and literals
//! carry the RDF-1.2 base `direction` — neither of which any incumbent RDF/JS library
//! models.

use gmeow_rdf::{RdfLiteral, RdfTerm, RdfTextDirection, RdfTriple};
use wasm_bindgen::prelude::*;

/// `xsd:string` — the datatype of a plain literal (RDF 1.1 §3.3).
pub(crate) const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
/// `rdf:langString` — the datatype of a language-tagged literal.
pub(crate) const RDF_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";
/// `rdf:dirLangString` — the datatype of a directional language-tagged literal (RDF 1.2).
pub(crate) const RDF_DIR_LANG_STRING: &str =
    "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString";

/// The internal shape of a [`Term`]. Covers the four RDF term kinds plus the two
/// query/graph term kinds RDF/JS adds (`Variable`, `DefaultGraph`).
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum TermInner {
    /// `termType: "NamedNode"` — an IRI.
    Named(String),
    /// `termType: "BlankNode"` — a blank-node label (without the `_:` prefix).
    Blank(String),
    /// `termType: "Literal"`.
    Literal(RdfLiteral),
    /// `termType: "Quad"` used as a term — an RDF-1.2 quoted triple.
    Quoted(Box<RdfTriple>),
    /// `termType: "Variable"` — a SPARQL variable name (without the `?`).
    Variable(String),
    /// `termType: "DefaultGraph"`.
    DefaultGraph,
}

/// An RDF/JS [Term](https://rdf.js.org/data-model-spec/#term-interface).
#[wasm_bindgen]
#[derive(Clone)]
pub struct Term {
    pub(crate) inner: TermInner,
}

impl Term {
    pub(crate) fn from_inner(inner: TermInner) -> Self {
        Self { inner }
    }

    /// Build a [`Term`] from the engine's owned [`RdfTerm`].
    pub(crate) fn from_rdf_term(t: &RdfTerm) -> Self {
        let inner = match t {
            RdfTerm::Iri(iri) => TermInner::Named(iri.clone()),
            RdfTerm::BlankNode(label) => TermInner::Blank(label.clone()),
            RdfTerm::Literal(lit) => TermInner::Literal(lit.clone()),
            RdfTerm::Triple(triple) => TermInner::Quoted(triple.clone()),
        };
        Self { inner }
    }

    /// Lower this term to the engine's owned [`RdfTerm`].
    ///
    /// Errors for `Variable`/`DefaultGraph`, which are not part of the RDF data model
    /// and cannot occur as a subject/predicate/object/graph-name of a stored quad.
    pub(crate) fn to_rdf_term(&self) -> Result<RdfTerm, String> {
        match &self.inner {
            TermInner::Named(iri) => Ok(RdfTerm::Iri(iri.clone())),
            TermInner::Blank(label) => Ok(RdfTerm::BlankNode(label.clone())),
            TermInner::Literal(lit) => Ok(RdfTerm::Literal(lit.clone())),
            TermInner::Quoted(triple) => Ok(RdfTerm::Triple(triple.clone())),
            TermInner::Variable(_) => Err("a Variable is not a valid RDF term".to_owned()),
            TermInner::DefaultGraph => {
                Err("the DefaultGraph is not a valid subject/predicate/object".to_owned())
            }
        }
    }

    /// The effective datatype IRI of a literal per RDF 1.2: `rdf:dirLangString` when a
    /// base direction is present, `rdf:langString` for a plain language tag, the
    /// explicit datatype otherwise, falling back to `xsd:string`.
    fn literal_datatype_iri(lit: &RdfLiteral) -> String {
        if lit.direction.is_some() {
            RDF_DIR_LANG_STRING.to_owned()
        } else if lit.language.is_some() {
            RDF_LANG_STRING.to_owned()
        } else {
            lit.datatype
                .clone()
                .unwrap_or_else(|| XSD_STRING.to_owned())
        }
    }
}

#[wasm_bindgen]
impl Term {
    /// `termType` — the RDF/JS discriminator.
    #[wasm_bindgen(getter = termType)]
    pub fn term_type(&self) -> String {
        match &self.inner {
            TermInner::Named(_) => "NamedNode",
            TermInner::Blank(_) => "BlankNode",
            TermInner::Literal(_) => "Literal",
            TermInner::Quoted(_) => "Quad",
            TermInner::Variable(_) => "Variable",
            TermInner::DefaultGraph => "DefaultGraph",
        }
        .to_owned()
    }

    /// `value` — the IRI, blank label, lexical form, or variable name. Empty for a
    /// quoted triple and the default graph (per RDF/JS).
    #[wasm_bindgen(getter)]
    pub fn value(&self) -> String {
        match &self.inner {
            TermInner::Named(v) | TermInner::Blank(v) | TermInner::Variable(v) => v.clone(),
            TermInner::Literal(lit) => lit.lexical_form.clone(),
            TermInner::Quoted(_) | TermInner::DefaultGraph => String::new(),
        }
    }

    /// `language` — the literal's language tag, or `""` for a non-language-tagged term
    /// (RDF/JS uses the empty string, not `undefined`).
    #[wasm_bindgen(getter)]
    pub fn language(&self) -> String {
        match &self.inner {
            TermInner::Literal(lit) => lit.language.clone().unwrap_or_default(),
            _ => String::new(),
        }
    }

    /// `direction` — the RDF-1.2 base direction (`"ltr"`/`"rtl"`), or `""` when absent.
    /// The deliberate extension to stock RDF/JS (`.goals`: overcome, don't inherit).
    #[wasm_bindgen(getter)]
    pub fn direction(&self) -> String {
        match &self.inner {
            TermInner::Literal(lit) => lit.direction.map(|d| d.as_str().to_owned()),
            _ => None,
        }
        .unwrap_or_default()
    }

    /// `datatype` — the literal's datatype as a `NamedNode`, or `undefined` for a
    /// non-literal.
    #[wasm_bindgen(getter)]
    pub fn datatype(&self) -> Option<Term> {
        match &self.inner {
            TermInner::Literal(lit) => Some(Term::from_inner(TermInner::Named(
                Self::literal_datatype_iri(lit),
            ))),
            _ => None,
        }
    }

    /// `subject` of a quoted-triple term (`termType: "Quad"`), else `undefined`.
    #[wasm_bindgen(getter)]
    pub fn subject(&self) -> Option<Term> {
        match &self.inner {
            TermInner::Quoted(t) => Some(Term::from_rdf_term(&t.subject)),
            _ => None,
        }
    }

    /// `predicate` of a quoted-triple term as a `NamedNode`, else `undefined`.
    #[wasm_bindgen(getter)]
    pub fn predicate(&self) -> Option<Term> {
        match &self.inner {
            TermInner::Quoted(t) => Some(Term::from_inner(TermInner::Named(t.predicate.clone()))),
            _ => None,
        }
    }

    /// `object` of a quoted-triple term, else `undefined`.
    #[wasm_bindgen(getter)]
    pub fn object(&self) -> Option<Term> {
        match &self.inner {
            TermInner::Quoted(t) => Some(Term::from_rdf_term(&t.object)),
            _ => None,
        }
    }

    /// `graph` of a quoted-triple term — always the default graph (a quoted triple has
    /// no graph slot), else `undefined`.
    #[wasm_bindgen(getter)]
    pub fn graph(&self) -> Option<Term> {
        match &self.inner {
            TermInner::Quoted(_) => Some(Term::from_inner(TermInner::DefaultGraph)),
            _ => None,
        }
    }

    /// Structural RDF/JS term equality.
    pub fn equals(&self, other: &Term) -> bool {
        self.inner == other.inner
    }
}

/// An RDF/JS [Quad](https://rdf.js.org/data-model-spec/#quad-interface) — a statement
/// `(subject, predicate, object, graph)` with `termType: "Quad"`.
#[wasm_bindgen]
#[derive(Clone)]
pub struct Quad {
    pub(crate) subject: Term,
    pub(crate) predicate: Term,
    pub(crate) object: Term,
    pub(crate) graph: Term,
}

impl Quad {
    pub(crate) fn from_parts(subject: Term, predicate: Term, object: Term, graph: Term) -> Self {
        Self {
            subject,
            predicate,
            object,
            graph,
        }
    }
}

#[wasm_bindgen]
impl Quad {
    /// Always `"Quad"` (a Quad is itself an RDF/JS term).
    #[wasm_bindgen(getter = termType)]
    pub fn term_type(&self) -> String {
        "Quad".to_owned()
    }

    /// Empty for a Quad (per RDF/JS).
    #[wasm_bindgen(getter)]
    pub fn value(&self) -> String {
        String::new()
    }

    #[wasm_bindgen(getter)]
    pub fn subject(&self) -> Term {
        self.subject.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn predicate(&self) -> Term {
        self.predicate.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn object(&self) -> Term {
        self.object.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn graph(&self) -> Term {
        self.graph.clone()
    }

    /// A quoted-triple [`Term`] (`termType: "Quad"`) viewing this quad's `(s, p, o)` —
    /// the RDF-1.2 wedge: pass the result as a subject/object to embed it.
    #[wasm_bindgen(js_name = asTerm)]
    pub fn as_term(&self) -> Result<Term, JsError> {
        let triple = RdfTriple::new(
            self.subject.to_rdf_term().map_err(|e| JsError::new(&e))?,
            match &self.predicate.inner {
                TermInner::Named(iri) => iri.clone(),
                _ => return Err(JsError::new("a quad predicate must be a NamedNode")),
            },
            self.object.to_rdf_term().map_err(|e| JsError::new(&e))?,
        );
        Ok(Term::from_inner(TermInner::Quoted(Box::new(triple))))
    }

    /// Structural RDF/JS quad equality.
    pub fn equals(&self, other: &Quad) -> bool {
        self.subject.inner == other.subject.inner
            && self.predicate.inner == other.predicate.inner
            && self.object.inner == other.object.inner
            && self.graph.inner == other.graph.inner
    }
}

/// Parse a base direction string (`"ltr"`/`"rtl"`) into the engine enum.
///
/// Returns a plain `String` error (native-testable; a `JsError` panics off wasm).
pub(crate) fn parse_direction(direction: &str) -> Result<RdfTextDirection, String> {
    match direction {
        "ltr" => Ok(RdfTextDirection::Ltr),
        "rtl" => Ok(RdfTextDirection::Rtl),
        other => Err(format!(
            "invalid base direction {other:?} (expected \"ltr\" or \"rtl\")"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_node_round_trips_through_rdf_term() {
        let t = Term::from_inner(TermInner::Named("https://e/s".to_owned()));
        assert_eq!(t.term_type(), "NamedNode");
        assert_eq!(t.value(), "https://e/s");
        assert_eq!(t.to_rdf_term().unwrap(), RdfTerm::iri("https://e/s"));
    }

    #[test]
    fn plain_literal_reports_xsd_string_datatype() {
        let t = Term::from_inner(TermInner::Literal(RdfLiteral::simple("hi")));
        assert_eq!(t.term_type(), "Literal");
        assert_eq!(t.value(), "hi");
        assert_eq!(t.language(), "");
        assert_eq!(t.datatype().unwrap().value(), XSD_STRING);
    }

    #[test]
    fn directional_literal_reports_dir_lang_string_and_direction() {
        let lit = RdfLiteral {
            lexical_form: "مرحبا".to_owned(),
            datatype: None,
            language: Some("ar".to_owned()),
            direction: Some(RdfTextDirection::Rtl),
        };
        let t = Term::from_inner(TermInner::Literal(lit));
        assert_eq!(t.language(), "ar");
        assert_eq!(t.direction(), "rtl");
        assert_eq!(t.datatype().unwrap().value(), RDF_DIR_LANG_STRING);
    }

    #[test]
    fn quoted_triple_term_exposes_components() {
        let triple = RdfTriple::new(
            RdfTerm::iri("https://e/s"),
            "https://e/p",
            RdfTerm::iri("https://e/o"),
        );
        let t = Term::from_inner(TermInner::Quoted(Box::new(triple)));
        assert_eq!(t.term_type(), "Quad");
        assert_eq!(t.value(), "");
        assert_eq!(t.subject().unwrap().value(), "https://e/s");
        assert_eq!(t.predicate().unwrap().value(), "https://e/p");
        assert_eq!(t.predicate().unwrap().term_type(), "NamedNode");
        assert_eq!(t.object().unwrap().value(), "https://e/o");
        assert_eq!(t.graph().unwrap().term_type(), "DefaultGraph");
    }

    #[test]
    fn quad_getters_and_equality() {
        let q = Quad::from_parts(
            Term::from_inner(TermInner::Named("https://e/s".to_owned())),
            Term::from_inner(TermInner::Named("https://e/p".to_owned())),
            Term::from_inner(TermInner::Literal(RdfLiteral::simple("v"))),
            Term::from_inner(TermInner::DefaultGraph),
        );
        assert_eq!(q.term_type(), "Quad");
        assert_eq!(q.subject().value(), "https://e/s");
        assert_eq!(q.object().value(), "v");
        assert_eq!(q.graph().term_type(), "DefaultGraph");
        assert!(q.equals(&q.clone()));
    }

    #[test]
    fn variable_and_default_graph_are_not_rdf_terms() {
        assert!(Term::from_inner(TermInner::Variable("x".to_owned()))
            .to_rdf_term()
            .is_err());
        assert!(Term::from_inner(TermInner::DefaultGraph)
            .to_rdf_term()
            .is_err());
    }

    #[test]
    fn equals_is_structural() {
        let a = Term::from_inner(TermInner::Named("https://e/x".to_owned()));
        let b = Term::from_inner(TermInner::Named("https://e/x".to_owned()));
        let c = Term::from_inner(TermInner::Named("https://e/y".to_owned()));
        assert!(a.equals(&b));
        assert!(!a.equals(&c));
    }
}
