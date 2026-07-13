// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native OWL 2 RL/RDF deductive closure.
//!
//! This is the Docker/Java-free **primary** entailment authority. The independent
//! in-process `purrdf::entail` OWL-RL evaluator remains the on-gate agreement
//! cross-check; it is not a production fallback.
//!
//! # Why a predicate-as-DATA encoding (not the [`crate::reason::el`] one)
//!
//! The EL/DL lane encodes every quad as the ternary `<predicate>(s, o, "world")`
//! form, where the RDF predicate becomes the relation *symbol*. That
//! encoding is structurally incapable of expressing OWL 2 RL meta-rules that
//! quantify over the property position (`prp-dom`, `prp-rng`, `prp-trp`,
//! `prp-inv`, `prp-spo1`, `prp-spo2`, `prp-symp`, `prp-fp`, `prp-eqp`) — a
//! relation symbol can never be a variable. `el.rs` names exactly this as its
//! honest gap ("they require a predicate-as-data reformulation").
//!
//! This module IS that reformulation: every quad is encoded as the **4-ary
//! generic-triple relation** `triple(?s, ?p, ?o, ?w)` with the predicate carried
//! as a *data* term in the second position, so the RL meta-rules can bind `?p`
//! to a variable and quantify over it. The world `?w` threads through unchanged,
//! so the closure is computed RDF-1.2-first (world-scoped, per-graph), never
//! flattened to a world-less RDF-1.0 representation.
//!
//! The chase machinery is the shared native forward evaluator
//! — the same one [`crate::reason::el`]/[`crate::reason::dl`] and
//! `gmeow_logic.materialize` drive. Only the encoding and the (fixed,
//! ontology-independent) RL rule set differ; the 4-ary `triple` facts here are
//! the live exercise of the typed bridge's n-ary capability.
//!
//! # Rule families implemented
//!
//! Driven by the constructs the 8 conversion suites exercise (verified by the
//! native↔`purrdf::entail` agreement loop) — a sound subset of OWL 2 RL/RDF:
//!
//! * **cax-sco** — class subsumption: `x a C1`, `C1 ⊑ C2` ⟹ `x a C2`.
//! * **scm-sco** — subclass transitivity: `C1 ⊑ C2`, `C2 ⊑ C3` ⟹ `C1 ⊑ C3`.
//! * **scm-eqc1/2 / cax-eqc1/2** — class equivalence ⟺ mutual subsumption.
//! * **scm-spo** — sub-property transitivity.
//! * **prp-spo1** — sub-property: `x P1 y`, `P1 ⊑ P2` ⟹ `x P2 y`.
//! * **prp-eqp1/2** — property equivalence ⟺ mutual sub-property.
//! * **prp-dom** — domain: `x P y`, `P rdfs:domain C` ⟹ `x a C`.
//! * **prp-rng** — range: `x P y`, `P rdfs:range C` ⟹ `y a C`.
//! * **prp-trp** — transitive property: `x P y`, `y P z` ⟹ `x P z`.
//! * **prp-symp** — symmetric property: `x P y` ⟹ `y P x`.
//! * **prp-inv1/2** — inverse properties: `x P1 y` ⟺ `y P2 x`.
//! * **prp-spo2** — length-2 property chains
//!   (`P owl:propertyChainAxiom ( P1 P2 )`): `x P1 y`, `y P2 z` ⟹ `x P z`.
//! * **scm-dom1/dom2 / scm-rng1/rng2** — domain/range propagate up the class
//!   hierarchy and down the sub-property hierarchy.
//! * **cls-svf1** — `owl:someValuesFrom` restriction membership.
//! * **cls-avf / cls-hv / cls-oneOf / cls-union** — the bundle's finite DL
//!   class-expression surface that has positive entailment consequences:
//!   universal restrictions, value restrictions, nominals, unions, and
//!   disjoint-union member subsumption.
//! * **cls-int1** — length-2 `owl:intersectionOf` membership; together with
//!   cls-svf1 + scm-eqc1 this recognizes the `owl:equivalentClass` defined
//!   classes (e.g. `PlaceNaming ≡ NameUsage ⊓ ∃usageNamed.Place`).
//! * **eq-sym / eq-trans / eq-rep-{s,p,o}** — `owl:sameAs` is an equivalence
//!   relation and substitutes in every position.
//!
//! `prp-fp` (functional-property `sameAs` derivation) and the disjointness
//! clash rules (`cax-dw`, `prp-irp`, …) are intentionally NOT materialised as
//! *positive* entailments here: they either derive only `owl:sameAs` edges the
//! suites never assert, or they detect inconsistency (the [`crate::reason::dl`]
//! lane's job). The independent `purrdf::entail` cross-check confirms this subset
//! on every fixture the suites use.

use std::collections::HashMap;

use crate::facts::{SKOLEM_PREFIX, TypedFactSet, skolem_iri};
use purrdf::{RdfDataset, RdfTerm, TermValue};

/// Wrap a reasoning-driver condition message as a typed diagnostic on the shared
/// substrate, preserving the authored text verbatim.
fn reason_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Reason { detail })
}

/// IRI scheme prefix for an interned-literal surrogate (see [`encode_generic_edb`]).
const LIT_SURROGATE_PREFIX: &str = "urn:gmeow-rl-lit:";

/// The relation name of the 4-ary generic-triple encoding: every closure fact
/// is `triple(subject, predicate-as-data, object, world)`.
const TRIPLE_RELATION: &str = "triple";

/// The relation name of the RDF-list membership helper `structured_rl_rules()` declares
/// (`list_member(?l, ?x, ?w)`) — internal bookkeeping for the finite
/// class-expression rules, never a closure fact.
const LIST_MEMBER_RELATION: &str = "list_member";

/// The sentinel world IRI a default-graph (un-named) triple is encoded under.
///
/// The 8 conversion suites build an rdflib default graph (no named graph), so
/// the closure runs in a single world. Derived triples carry this IRI, which the
/// Python helper drops when folding the closure back into the default graph.
pub const DEFAULT_WORLD: &str = "https://blackcatinformatics.ca/gmeow/graph/rl-default";

// The fixed OWL 2 RL/RDF calculus is authored as typed rules in `rl_rules.rs`.

/// One triple in the RL closure, decoded from a generic-triple chase row.
///
/// `subject`/`predicate` are bare IRI strings; `object` is the N-Triples object
/// form (`<iri>`, or a quoted literal `"v"` / `"v"@lang` / `"v"^^<dt>` resolved
/// back from its surrogate); `world` is the named-graph IRI.
/// `is_edb` distinguishes asserted facts (`true`) from rule-derived ones.
/// `rule_name` is the firing rule's `#[name(...)]` (`None` for EDB).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RlTriple {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub world: String,
    pub is_edb: bool,
    pub rule_name: Option<String>,
}

/// The result of an OWL 2 RL closure run: every asserted + derived triple.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RlClosure {
    pub triples: Vec<RlTriple>,
}

impl RlClosure {
    /// Render the full closure as a deterministic N-Triples document.
    ///
    /// This is the native replacement for the per-row term rendering the Python
    /// helper (`gmeow_tools.native_rl`) used to do — moved into Rust so the
    /// reasoning path crosses the FFI boundary exactly once. A
    /// skolemized blank-node IRI (`{SKOLEM_PREFIX}…`) is mapped back to an
    /// N-Triples blank-node label so a source blank node round-trips as a blank
    /// node; every other subject/predicate is a NamedNode and the object is
    /// already in N-Triples object form (`<iri>` or a quoted literal). The world
    /// axis is dropped (default-graph N-Triples); lines are de-duplicated and
    /// sorted for a byte-stable result.
    #[must_use]
    pub fn to_ntriples(&self) -> String {
        let mut lines: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for t in &self.triples {
            let s = render_nt_resource(&t.subject);
            let p = render_nt_resource(&t.predicate);
            let o = render_nt_object(&t.object);
            lines.insert(format!("{s} {p} {o} ."));
        }
        let mut out = String::new();
        for line in lines {
            out.push_str(&line);
            out.push('\n');
        }
        out
    }
}

/// Render an engine subject/predicate IRI (bare) as an N-Triples term: a skolem
/// IRI becomes a blank-node label, every other value a NamedNode.
fn render_nt_resource(value: &str) -> String {
    if let Some(tail) = value.strip_prefix(SKOLEM_PREFIX) {
        format!("_:{}", skolem_label(tail))
    } else {
        format!("<{value}>")
    }
}

/// Render an engine object term (already N-Triples display form) for re-parse. An
/// IRI object that is a skolem IRI is rewritten to a blank-node label; literals
/// (the engine emits valid N-Triples literals) pass through verbatim.
fn render_nt_object(obj_nt: &str) -> String {
    if let Some(inner) = obj_nt.strip_prefix('<').and_then(|s| s.strip_suffix('>')) {
        if let Some(tail) = inner.strip_prefix(SKOLEM_PREFIX) {
            return format!("_:{}", skolem_label(tail));
        }
        return obj_nt.to_owned();
    }
    // Literal (`"v"`, `"v"@lang`, `"v"^^<dt>`) — already valid N-Triples.
    obj_nt.to_owned()
}

/// Derive a syntactically-valid N-Triples blank-node label from a skolem tail
/// (the identifier after [`SKOLEM_PREFIX`]): prefix `b`, replace any character not
/// permitted in a label with `_`.
fn skolem_label(tail: &str) -> String {
    let mut label = String::with_capacity(tail.len() + 1);
    label.push('b');
    for ch in tail.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            label.push(ch);
        } else {
            label.push('_');
        }
    }
    label
}

/// Render a literal as its N-Triples object form (`"v"`, `"v"@lang`,
/// `"v"^^<dt>`) — the form rdflib parses back losslessly.
fn literal_nt(lit: &purrdf::RdfLiteral) -> String {
    // N-Triples requires escaping `\`, `"`, newline, CR, and tab in the value.
    let escaped = lit
        .lexical_form
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
    if let Some(lang) = &lit.language {
        format!("\"{escaped}\"@{lang}")
    } else if let Some(dt) = &lit.datatype {
        if dt == "http://www.w3.org/2001/XMLSchema#string" {
            format!("\"{escaped}\"")
        } else {
            format!("\"{escaped}\"^^<{dt}>")
        }
    } else {
        format!("\"{escaped}\"")
    }
}

/// The interning state threaded through one [`rl_closure`] run.
///
/// OWL 2 RL rules never inspect a *literal value* (they match on IRIs in the
/// property / class positions), so a literal object is mapped to an opaque
/// surrogate IRI before the chase and mapped back afterwards. This is sound —
/// the closure is identical to one over the literals themselves — and keeps the
/// native rule core resource-only while round-tripping literal identity.
#[derive(Default)]
struct Interner {
    /// `surrogate IRI` → original literal N-Triples object form.
    by_surrogate: HashMap<String, String>,
    /// Literal N-Triples object form → surrogate IRI (dedup so equal literals
    /// intern to one surrogate).
    by_nt: HashMap<String, String>,
}

impl Interner {
    /// Intern a literal, returning its stable surrogate IRI.
    fn intern_literal(&mut self, lit: &purrdf::RdfLiteral) -> String {
        let nt = literal_nt(lit);
        if let Some(s) = self.by_nt.get(&nt) {
            return s.clone();
        }
        let surrogate = format!("{LIT_SURROGATE_PREFIX}{}", self.by_nt.len());
        self.by_nt.insert(nt.clone(), surrogate.clone());
        self.by_surrogate.insert(surrogate.clone(), nt);
        surrogate
    }

    /// Resolve a decoded object IRI back to its N-Triples object form. A
    /// surrogate IRI maps to the original literal; any other IRI is itself.
    fn resolve_object(&self, iri: &str) -> String {
        match self.by_surrogate.get(iri) {
            Some(nt) => nt.clone(),
            None => format!("<{iri}>"),
        }
    }
}

/// Coerce a subject/object IRI-or-bnode term to a typed resource term; a
/// literal is interned to its surrogate IRI; a triple term is unsupported.
fn resource_term(term: &RdfTerm, interner: &mut Interner) -> Option<TermValue> {
    match term {
        RdfTerm::Iri(iri) => Some(TermValue::iri(iri)),
        RdfTerm::BlankNode(id) => Some(TermValue::Iri(skolem_iri(id))),
        RdfTerm::Literal(lit) => Some(TermValue::Iri(interner.intern_literal(lit))),
        RdfTerm::Triple(_) => None,
    }
}

/// Encode an [`RdfDataset`] into a typed generic-triple `triple(?s,?p,?o,?w)`
/// EDB — the live proof of the typed bridge's n-ary capability.
///
/// Every quad becomes a 4-ary `triple` fact with the predicate as DATA (so RL's
/// property-quantifying rules can bind it): the predicate IRI travels in the
/// second ARGUMENT position as an IRI term, while the relation NAME is the
/// constant `triple`. IRIs/bnodes go through verbatim (bnodes skolemized);
/// literal objects are interned to opaque surrogate IRIs via `interner` (RL
/// rules never inspect a literal value — see [`Interner`]). The named graph is
/// the world, interned as a plain string literal; a default-graph (or
/// blank-node-graph) triple is encoded under [`DEFAULT_WORLD`] so an un-named
/// rdflib graph still closes in a single world (RDF-1.2-first; the world axis
/// is never flattened away). A triple-term subject/object is skipped —
/// unsupported in this RL encoding and absent from the suites' RL fixtures.
fn encode_generic_edb(store: &RdfDataset, interner: &mut Interner) -> TypedFactSet {
    let mut facts = TypedFactSet::new();
    for quad in store.owned_quads() {
        let Some(subj) = resource_term(&quad.subject, interner) else {
            continue;
        };
        let Some(obj) = resource_term(&quad.object, interner) else {
            continue;
        };
        let pred = TermValue::iri(&quad.predicate);

        let world = match &quad.graph_name {
            Some(RdfTerm::Iri(iri)) => iri.clone(),
            _ => DEFAULT_WORLD.to_owned(),
        };

        let s = facts.intern(&subj);
        let p = facts.intern(&pred);
        let o = facts.intern(&obj);
        let w = facts.intern(&TermValue::simple_literal(&world));
        facts.push_fact(TRIPLE_RELATION, vec![s, p, o, w]);
    }
    facts
}

/// The bare IRI string of a typed generic-triple argument.
///
/// Every subject/predicate/object position of the `triple/4` encoding carries
/// an IRI term (literals were interned to surrogate IRIs before the chase), so
/// any other shape is a hard error.
fn rl_iri(term: &TermValue, position: &str) -> gmeow_errors::Result<String> {
    match term {
        TermValue::Iri(iri) => Ok(iri.clone()),
        other => Err(reason_err(format!(
            "RL closure row {position} must be an IRI term \
             (literals are interned to surrogate IRIs), got {other:?}"
        ))),
    }
}

/// Compute the native OWL 2 RL/RDF deductive closure of `edb`.
///
/// Loads `edb` into the typed generic-triple encoding, runs the native structured
/// generic evaluator once over `structured_rl_rules()`, and coerces every
/// `triple/4` typed row back into an [`RlTriple`] (asserted + derived). The
/// closure is world-scoped: derived triples carry the world IRI of the facts
/// they were derived from.
///
/// # Errors
///
/// Returns an error if the chase fails to validate, evaluate, or decode, or if a
/// materialized row is not one of the two relations `structured_rl_rules()`
/// declares (`triple/4`, `list_member/3`).
pub fn rl_closure(edb: &RdfDataset) -> gmeow_errors::Result<RlClosure> {
    let mut interner = Interner::default();
    let edb_facts = encode_generic_edb(edb, &mut interner);
    if edb_facts.is_empty() {
        return Ok(RlClosure { triples: vec![] });
    }
    let rules = super::rl_rules::structured_rl_rules();
    let chase = crate::physical::materialize_generic(&edb_facts, &rules)?;

    let mut triples: Vec<RlTriple> = Vec::new();
    for (row, prov) in &chase.rows {
        // The RL rule text is repo-owned and declares exactly TWO relations:
        // the 4-ary generic-triple closure relation and the ternary RDF-list
        // membership helper. The helper is internal bookkeeping — explicitly
        // not a closure fact — and any OTHER row shape indicates a rule-text
        // bug: hard-error, never skip silently (same doctrine as
        // ordinary materialization; its non-quad bucket exists for caller-supplied
        // structured programs, which these are not).
        match (row.predicate.as_str(), row.args.len()) {
            (TRIPLE_RELATION, 4) => {}
            (LIST_MEMBER_RELATION, 3) => continue,
            (pred, arity) => {
                return Err(reason_err(format!(
                    "RL chase produced an unexpected row {pred:?} (arity {arity}): \
                     the fixed RL rule text declares only triple/4 and \
                     list_member/3, so this is a rule-text bug"
                )));
            }
        }
        let subject = rl_iri(&row.args[0], "subject")?;
        // A literal surrogate in the SUBJECT position is a derived literal-typing
        // entailment (e.g. `prp-rng` typing an interned literal object) with no
        // standard-RDF form — a literal can never be a triple subject. The native
        // authority drops the non-standard D-entailment row (sound: no supported RL
        // rule depends on a literal's type, and the suites never assert one).
        if subject.starts_with(LIT_SURROGATE_PREFIX) {
            continue;
        }
        let predicate = rl_iri(&row.args[1], "predicate")?;
        // The object is always an IRI in the chase (literals were interned to
        // surrogate IRIs); resolve a surrogate back to its original literal.
        let object = interner.resolve_object(&rl_iri(&row.args[2], "object")?);
        // The world is the 4th argument: a plain string literal.
        let world = match &row.args[3] {
            TermValue::Literal {
                lexical_form,
                datatype,
                language: None,
                ..
            } if datatype == "http://www.w3.org/2001/XMLSchema#string" => lexical_form.clone(),
            other => {
                return Err(reason_err(format!(
                    "RL closure row world must be a plain string literal, got {other:?}"
                )));
            }
        };

        triples.push(RlTriple {
            subject,
            predicate,
            object,
            world,
            is_edb: prov.is_edb,
            rule_name: prov.rule_name.clone(),
        });
    }
    Ok(RlClosure { triples })
}

#[cfg(test)]
mod tests {
    use super::*;
    use purrdf::{RdfDatasetBuilder, RdfQuad, RdfTerm};

    const W: &str = "http://gmeow.example/w";
    const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
    const SUBPROP: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
    const DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
    const RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
    const FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
    const REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
    const NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
    const TRANSITIVE: &str = "http://www.w3.org/2002/07/owl#TransitiveProperty";
    const SYMMETRIC: &str = "http://www.w3.org/2002/07/owl#SymmetricProperty";
    const INVERSE_OF: &str = "http://www.w3.org/2002/07/owl#inverseOf";
    const CHAIN: &str = "http://www.w3.org/2002/07/owl#propertyChainAxiom";

    const A: &str = "http://gmeow.example/A";
    const B: &str = "http://gmeow.example/B";
    const C: &str = "http://gmeow.example/C";
    const P: &str = "http://gmeow.example/p";
    const P1: &str = "http://gmeow.example/p1";
    const P2: &str = "http://gmeow.example/p2";
    const X: &str = "http://gmeow.example/x";
    const Y: &str = "http://gmeow.example/y";
    const Z: &str = "http://gmeow.example/z";

    fn quad(s: &str, p: &str, o: &str) -> RdfQuad {
        RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(W))
    }

    fn dataset(quads: Vec<RdfQuad>) -> std::sync::Arc<RdfDataset> {
        let mut builder = RdfDatasetBuilder::new();
        for quad in quads {
            builder.push_owned_quad(&quad);
        }
        builder.freeze().expect("valid test dataset")
    }

    fn has(closure: &RlClosure, s: &str, p: &str, o: &str) -> bool {
        let obj = format!("<{o}>");
        closure
            .triples
            .iter()
            .any(|t| t.subject == s && t.predicate == p && t.object == obj)
    }

    #[test]
    fn cax_sco_type_propagates_through_subclass() {
        // x a A, A ⊑ B, B ⊑ C ⇒ x a B, x a C.
        let store = dataset(vec![
            quad(X, TYPE, A),
            quad(A, SUBCLASS, B),
            quad(B, SUBCLASS, C),
        ]);
        let c = rl_closure(store.as_ref()).expect("RL closure should succeed");
        assert!(has(&c, X, TYPE, B), "x a B via cax-sco");
        assert!(has(&c, X, TYPE, C), "x a C via cax-sco + scm-sco");
    }

    #[test]
    fn prp_dom_and_rng_derive_types() {
        // p domain A, p range B, x p y ⇒ x a A, y a B.
        let store = dataset(vec![quad(P, DOMAIN, A), quad(P, RANGE, B), quad(X, P, Y)]);
        let c = rl_closure(store.as_ref()).expect("RL closure should succeed");
        assert!(has(&c, X, TYPE, A), "x a A via prp-dom");
        assert!(has(&c, Y, TYPE, B), "y a B via prp-rng");
    }

    #[test]
    fn prp_spo1_propagates_assertions_up_the_property_hierarchy() {
        // p1 ⊑ p2, x p1 y ⇒ x p2 y.
        let store = dataset(vec![quad(P1, SUBPROP, P2), quad(X, P1, Y)]);
        let c = rl_closure(store.as_ref()).expect("RL closure should succeed");
        assert!(has(&c, X, P2, Y), "x p2 y via prp-spo1");
    }

    #[test]
    fn prp_trp_closes_a_transitive_chain() {
        // p transitive, x p y, y p z ⇒ x p z.
        let store = dataset(vec![
            quad(P, TYPE, TRANSITIVE),
            quad(X, P, Y),
            quad(Y, P, Z),
        ]);
        let c = rl_closure(store.as_ref()).expect("RL closure should succeed");
        assert!(has(&c, X, P, Z), "x p z via prp-trp");
    }

    #[test]
    fn prp_symp_mirrors_a_symmetric_edge() {
        // p symmetric, x p y ⇒ y p x.
        let store = dataset(vec![quad(P, TYPE, SYMMETRIC), quad(X, P, Y)]);
        let c = rl_closure(store.as_ref()).expect("RL closure should succeed");
        assert!(has(&c, Y, P, X), "y p x via prp-symp");
    }

    #[test]
    fn prp_inv_derives_both_directions() {
        // p1 inverseOf p2, x p1 y ⇒ y p2 x.
        let store = dataset(vec![quad(P1, INVERSE_OF, P2), quad(X, P1, Y)]);
        let c = rl_closure(store.as_ref()).expect("RL closure should succeed");
        assert!(has(&c, Y, P2, X), "y p2 x via prp-inv1");
    }

    #[test]
    fn prp_spo2_fires_a_length_two_property_chain() {
        // p propertyChainAxiom ( p1 p2 ), x p1 y, y p2 z ⇒ x p z.
        let l0 = "http://gmeow.example/l0";
        let l1 = "http://gmeow.example/l1";
        let store = dataset(vec![
            quad(P, CHAIN, l0),
            quad(l0, FIRST, P1),
            quad(l0, REST, l1),
            quad(l1, FIRST, P2),
            quad(l1, REST, NIL),
            quad(X, P1, Y),
            quad(Y, P2, Z),
        ]);
        let c = rl_closure(store.as_ref()).expect("RL closure should succeed");
        assert!(has(&c, X, P, Z), "x p z via prp-spo2");
    }

    #[test]
    fn cls_svf_and_int_recognize_an_equivalent_defined_class() {
        // C ≡ (C1 ⊓ ∃P.D): defined-class recognition via cls-int1 + cls-svf1 +
        // scm-eqc1. x a C1, x P y, y a D ⇒ x a C.
        const EQUIV_NODE: &str = "http://gmeow.example/equiv";
        const RESTR: &str = "http://gmeow.example/restr";
        const L0: &str = "http://gmeow.example/l0";
        const L1: &str = "http://gmeow.example/l1";
        const C1: &str = "http://gmeow.example/C1";
        const D: &str = "http://gmeow.example/D";
        const DEFINED: &str = "http://gmeow.example/Defined";
        const INTERSECTION: &str = "http://www.w3.org/2002/07/owl#intersectionOf";
        const SVF: &str = "http://www.w3.org/2002/07/owl#someValuesFrom";
        const ONPROP: &str = "http://www.w3.org/2002/07/owl#onProperty";
        const EQC: &str = "http://www.w3.org/2002/07/owl#equivalentClass";

        let store = dataset(vec![
            // Defined ≡ equiv-node ; equiv-node intersectionOf ( C1 restr )
            quad(DEFINED, EQC, EQUIV_NODE),
            quad(EQUIV_NODE, INTERSECTION, L0),
            quad(L0, FIRST, C1),
            quad(L0, REST, L1),
            quad(L1, FIRST, RESTR),
            quad(L1, REST, NIL),
            // restr = ∃P.D
            quad(RESTR, ONPROP, P),
            quad(RESTR, SVF, D),
            // A-Box: x a C1, x P y, y a D
            quad(X, TYPE, C1),
            quad(X, P, Y),
            quad(Y, TYPE, D),
        ]);
        let c = rl_closure(store.as_ref()).expect("RL closure should succeed");
        assert!(
            has(&c, X, TYPE, DEFINED),
            "x must be classified into the equivalent defined class Defined"
        );
    }

    // ── rule-local coverage for the five RL clause families (feedback) ──
    // Each test exercises exactly one clause family with the minimal axioms that
    // make it fire, in the same shape as the cls-svf/cls-int test above.

    const ONPROP: &str = "http://www.w3.org/2002/07/owl#onProperty";
    const ALL_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#allValuesFrom";
    const HAS_VALUE: &str = "http://www.w3.org/2002/07/owl#hasValue";
    const ONE_OF: &str = "http://www.w3.org/2002/07/owl#oneOf";
    const UNION_OF: &str = "http://www.w3.org/2002/07/owl#unionOf";
    const DISJOINT_UNION_OF: &str = "http://www.w3.org/2002/07/owl#disjointUnionOf";

    #[test]
    fn cls_avf_types_property_values_under_all_values_from() {
        // R onProperty P; R allValuesFrom C; x a R; x P y ⇒ y a C.
        const R: &str = "http://gmeow.example/R";
        let store = dataset(vec![
            quad(R, ONPROP, P),
            quad(R, ALL_VALUES_FROM, C),
            quad(X, TYPE, R),
            quad(X, P, Y),
        ]);
        let c = rl_closure(store.as_ref()).expect("RL closure should succeed");
        assert!(has(&c, Y, TYPE, C), "y a C via cls-avf");
    }

    #[test]
    fn cls_hv1_and_hv2_assert_and_recognize_has_value() {
        // R onProperty P; R hasValue V.
        //   cls-hv1: x a R          ⇒ x P V   (assert the value).
        //   cls-hv2: z P V          ⇒ z a R   (recognize the restriction).
        const R: &str = "http://gmeow.example/R";
        const V: &str = "http://gmeow.example/v";
        let store = dataset(vec![
            quad(R, ONPROP, P),
            quad(R, HAS_VALUE, V),
            quad(X, TYPE, R),
            quad(Z, P, V),
        ]);
        let c = rl_closure(store.as_ref()).expect("RL closure should succeed");
        assert!(has(&c, X, P, V), "x P V via cls-hv1");
        assert!(has(&c, Z, TYPE, R), "z a R via cls-hv2");
    }

    #[test]
    fn cls_oneof_types_each_nominal_member() {
        // C oneOf ( x y ) ⇒ x a C, y a C.
        let l0 = "http://gmeow.example/l0";
        let l1 = "http://gmeow.example/l1";
        let store = dataset(vec![
            quad(C, ONE_OF, l0),
            quad(l0, FIRST, X),
            quad(l0, REST, l1),
            quad(l1, FIRST, Y),
            quad(l1, REST, NIL),
        ]);
        let c = rl_closure(store.as_ref()).expect("RL closure should succeed");
        assert!(has(&c, X, TYPE, C), "x a C via cls-oneOf");
        assert!(has(&c, Y, TYPE, C), "y a C via cls-oneOf");
    }

    #[test]
    fn cls_union_member_subclasses_each_member_to_the_union() {
        // C unionOf ( A B ) ⇒ A ⊑ C, B ⊑ C.
        let l0 = "http://gmeow.example/l0";
        let l1 = "http://gmeow.example/l1";
        let store = dataset(vec![
            quad(C, UNION_OF, l0),
            quad(l0, FIRST, A),
            quad(l0, REST, l1),
            quad(l1, FIRST, B),
            quad(l1, REST, NIL),
        ]);
        let c = rl_closure(store.as_ref()).expect("RL closure should succeed");
        assert!(has(&c, A, SUBCLASS, C), "A ⊑ C via cls-union-member");
        assert!(has(&c, B, SUBCLASS, C), "B ⊑ C via cls-union-member");
    }

    #[test]
    fn cls_disjoint_union_member_subclasses_each_member_to_the_union() {
        // C disjointUnionOf ( A B ) ⇒ A ⊑ C, B ⊑ C.
        let l0 = "http://gmeow.example/l0";
        let l1 = "http://gmeow.example/l1";
        let store = dataset(vec![
            quad(C, DISJOINT_UNION_OF, l0),
            quad(l0, FIRST, A),
            quad(l0, REST, l1),
            quad(l1, FIRST, B),
            quad(l1, REST, NIL),
        ]);
        let c = rl_closure(store.as_ref()).expect("RL closure should succeed");
        assert!(
            has(&c, A, SUBCLASS, C),
            "A ⊑ C via cls-disjointUnion-member"
        );
        assert!(
            has(&c, B, SUBCLASS, C),
            "B ⊑ C via cls-disjointUnion-member"
        );
    }

    #[test]
    fn literal_objects_round_trip_through_interning() {
        // A hyphenated language tag (`@x-gmeow-english`), an escaped quote, and a
        // typed integer all retain exact identity through interning. prp-spo1 must
        // carry the literal object through the closure unchanged.
        let subprop = SUBPROP;
        let p1 = P1;
        let p2 = P2;
        let label = "http://www.w3.org/2000/01/rdf-schema#label";
        let lit_quad = RdfQuad::new(
            RdfTerm::iri(X),
            p1,
            RdfTerm::Literal(purrdf::RdfLiteral::language_tagged(
                "say \"hi\"",
                "x-gmeow-english",
            )),
        )
        .in_graph(RdfTerm::iri(W));
        let int_quad = RdfQuad::new(
            RdfTerm::iri(X),
            label,
            RdfTerm::Literal(purrdf::RdfLiteral::typed(
                "5",
                "http://www.w3.org/2001/XMLSchema#integer",
            )),
        )
        .in_graph(RdfTerm::iri(W));
        let store = dataset(vec![quad(p1, subprop, p2), lit_quad, int_quad]);
        let c = rl_closure(store.as_ref()).expect("RL closure should succeed");

        // The interned language literal propagates up the sub-property hierarchy.
        let derived = c
            .triples
            .iter()
            .find(|t| t.subject == X && t.predicate == p2)
            .expect("x p2 <lang-literal> must be derived via prp-spo1");
        assert_eq!(derived.object, "\"say \\\"hi\\\"\"@x-gmeow-english");
        // The typed integer literal round-trips with its datatype intact.
        assert!(
            c.triples.iter().any(|t| t.subject == X
                && t.predicate == label
                && t.object == "\"5\"^^<http://www.w3.org/2001/XMLSchema#integer>"),
            "typed integer literal must round-trip"
        );
    }

    #[test]
    fn closure_carries_the_world_and_edb_flags() {
        let store = dataset(vec![quad(X, TYPE, A), quad(A, SUBCLASS, B)]);
        let c = rl_closure(store.as_ref()).expect("RL closure should succeed");
        let derived = c
            .triples
            .iter()
            .find(|t| t.subject == X && t.predicate == TYPE && t.object == format!("<{B}>"))
            .expect("x a B must be derived");
        assert!(!derived.is_edb, "derived triple must not be is_edb");
        assert_eq!(derived.world, W, "derived triple carries its world");
        assert_eq!(
            derived.rule_name.as_deref(),
            Some("rl:cax-sco"),
            "derived triple cites the firing rule"
        );
    }

    #[test]
    fn to_ntriples_renders_blank_literal_dedups_and_sorts() {
        // The native render that replaced the Python `gmeow_tools.native_rl` row
        // formatter: skolem IRI → blank-node label, literal pass-through,
        // de-dup, and byte-stable sort.
        let lit = |s: &str, p: &str, o: &str| RlTriple {
            subject: s.to_owned(),
            predicate: p.to_owned(),
            object: o.to_owned(),
            world: W.to_owned(),
            is_edb: false,
            rule_name: None,
        };
        let closure = RlClosure {
            triples: vec![
                lit(B, TYPE, &format!("<{C}>")),
                lit(A, TYPE, &format!("<{B}>")),
                // Skolemized blank-node subject + a language-tagged literal object.
                lit(&format!("{SKOLEM_PREFIX}abc123"), P, "\"hi\"@en"),
                // Exact duplicate of the first row — must collapse to one line.
                lit(B, TYPE, &format!("<{C}>")),
            ],
        };
        let nt = closure.to_ntriples();
        let lines: Vec<&str> = nt.lines().collect();
        assert_eq!(lines.len(), 3, "deduped to 3 distinct lines: {nt:?}");
        let mut sorted = lines.clone();
        sorted.sort_unstable();
        assert_eq!(lines, sorted, "lines are sorted for determinism");
        assert!(
            nt.contains(&format!("_:babc123 <{P}> \"hi\"@en .\n")),
            "skolem IRI → blank-node label, literal preserved: {nt}"
        );
        assert!(
            nt.contains(&format!("<{A}> <{TYPE}> <{B}> .\n")),
            "named-node triple rendered verbatim: {nt}"
        );
    }
}
