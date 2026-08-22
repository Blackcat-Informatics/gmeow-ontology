// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! OWL 2 RL/RDF deductive closure, computed by purrdf's `entail` chase.
//!
//! This is the Docker/Java-free entailment authority for the RL lane. The closure
//! is purrdf's OWL 2 RL materialization ([`purrdf::entail::materialize`] with
//! [`purrdf::entail::Materialization::OwlRl`]) — the full 78-rule calculus of
//! OWL 2 Profiles §4.3 Tables 4–9 — surfaced through the [`RlClosure`]/[`RlTriple`]
//! contract this module's consumers depend on. The retired native rule table only
//! ever fired a sound 32-rule subset; the cutover keeps the public shape and widens
//! the entailments to the whole profile.
//!
//! # The RDF 1.2 world ⇔ graph mapping (the encode/decode boundary)
//!
//! Every reasoning fact is world-scoped. The world of a triple is the RDF 1.2
//! quad's fourth position — its named graph. This module folds that axis at exactly
//! two points, and the two are inverses:
//!
//! * **encode** ([`lower_edb_for_rl`]): the input EDB is lowered into the dataset the
//!   chase closes. A named IRI graph becomes its own world; a default or blank-node
//!   graph folds to the [`DEFAULT_WORLD`] named graph, so an un-named graph still
//!   closes in one world. Because everything is emitted into a NAMED graph, the
//!   default graph the chase closes is empty, so each world closes against itself
//!   alone and two worlds never mix — the same per-world independence the native
//!   encoder gave with its explicit `?w` thread.
//! * **decode** ([`rl_closure`]): each closure quad's graph slot is read back to the
//!   world string — a named IRI graph to its IRI, anything else to [`DEFAULT_WORLD`].
//!
//! # `is_edb` and rule attribution
//!
//! purrdf's OWL 2 RL closure carries the asserted quads (the seed is copied into the
//! result) alongside every derived one. A closure quad present in the lowered EDB is
//! `is_edb` (asserted); the rest are derived. `is_edb` is decided eagerly and cheaply
//! (set membership against the lowered EDB).
//!
//! The firing rule of a derived triple is attributed by re-explaining the conclusion
//! over the retained lowered EDB ([`RlClosure::rule_name`] → [`rule_name_for_conclusion`]).
//! This is computed ON DEMAND, not eagerly, because purrdf exposes no BULK per-triple
//! provenance: [`purrdf::entail::materialize`]'s report gives only aggregate
//! `rules_fired()` counts, and [`purrdf::entail::explain_conclusion`] re-runs the whole
//! closure fixpoint for each conclusion. Attributing every derived triple eagerly is
//! therefore `O(derived × closure)` — measured at ~27s for a two-module scoped closure
//! whose materialization alone is ~0.1s — so a caller pays that cost only for the
//! triples it actually asks about. OWL 2 RL has no existential heads, so every derived
//! conclusion has a checkable derivation and the attribution never refuses.

use crate::facts::{SKOLEM_PREFIX, skolem_iri};
use purrdf::{RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm, TermValue};

/// Wrap a reasoning-driver condition message as a typed diagnostic on the shared
/// substrate, preserving the authored text verbatim.
fn reason_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Reason { detail })
}

/// The `xsd:string` datatype IRI — the datatype a plain literal carries once purrdf
/// has expanded it, and the one an N-Triples object form leaves implicit.
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

/// The sentinel world IRI a default-graph (un-named) triple is closed under.
///
/// A default or blank-node graph carries no world of its own, so its triples fold to
/// this single named world; a named IRI graph keeps its own IRI. The value is the
/// same one the retired native encoder used, so a downstream consumer folding the
/// closure back into an un-named graph still recognizes it.
pub const DEFAULT_WORLD: &str = "https://blackcatinformatics.ca/gmeow/graph/rl-default";

/// One triple in the RL closure, decoded from a closure quad.
///
/// `subject`/`predicate` are bare IRI strings (a blank-node subject is rendered as
/// its skolem IRI); `object` is the N-Triples object form (`<iri>`, a skolem
/// `<iri>` for a blank node, or a quoted literal `"v"` / `"v"@lang` / `"v"^^<dt>`);
/// `world` is the named-graph IRI (or [`DEFAULT_WORLD`]).
/// `is_edb` distinguishes asserted facts (`true`) from rule-derived ones.
///
/// The firing rule of a derived triple is NOT stored here — it is attributed on
/// demand by [`RlClosure::rule_name`]; see the module docs for why eager attribution
/// is `O(derived × closure)` under purrdf's public API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RlTriple {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub world: String,
    pub is_edb: bool,
}

/// The result of an OWL 2 RL closure run: every asserted + derived triple.
///
/// Also retains the lowered EDB and the materialized closure so [`Self::rule_name`]
/// can attribute a derived triple's firing rule on demand. Those two carriers are
/// reasoner state, not closure content, so [`PartialEq`]/[`Eq`] compare only
/// [`Self::triples`]: two closures with equal triples are equal.
#[derive(Debug, Clone)]
pub struct RlClosure {
    pub triples: Vec<RlTriple>,
    /// The lowered EDB the closure was reasoned from, for on-demand attribution.
    /// `None` for a hand-constructed closure (e.g. a render fixture).
    edb: Option<std::sync::Arc<RdfDataset>>,
    /// The materialized closure, for recovering a derived triple's typed terms during
    /// attribution. `None` for a hand-constructed closure.
    closure: Option<std::sync::Arc<RdfDataset>>,
}

impl PartialEq for RlClosure {
    fn eq(&self, other: &Self) -> bool {
        self.triples == other.triples
    }
}

impl Eq for RlClosure {}

impl RlClosure {
    /// Render the full closure as a deterministic N-Triples document.
    ///
    /// A skolemized blank-node IRI (`{SKOLEM_PREFIX}…`) is mapped back to an
    /// N-Triples blank-node label so a source blank node round-trips as a blank
    /// node; every other subject/predicate is a NamedNode and the object is already
    /// in N-Triples object form (`<iri>` or a quoted literal). The world axis is
    /// dropped (default-graph N-Triples); lines are de-duplicated and sorted for a
    /// byte-stable result.
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

    /// The OWL 2 RL specification rule name credited with deriving `triple`, or `None`
    /// for an asserted (EDB) triple.
    ///
    /// Computed ON DEMAND via purrdf's [`purrdf::entail::explain_conclusion`] over the
    /// retained lowered EDB — see the module docs for why eager attribution is
    /// impractical under purrdf's public API. Deterministic: the specification-table-first
    /// rule the derivation cites (the single head rule is not exposed on purrdf's public
    /// `ChaseProof` surface).
    ///
    /// # Errors
    ///
    /// A [`Reason`](crate::error::Reason) diagnostic if this closure carries no reasoner
    /// input (a hand-constructed closure), if `triple` is not a member of it, or if purrdf
    /// refuses to explain a derived triple (impossible for OWL 2 RL, which has no
    /// existential heads).
    pub fn rule_name(&self, triple: &RlTriple) -> gmeow_errors::Result<Option<String>> {
        if triple.is_edb {
            return Ok(None);
        }
        let (Some(edb), Some(closure)) = (self.edb.as_ref(), self.closure.as_ref()) else {
            return Err(reason_err(format!(
                "RL closure carries no reasoner input, so the firing rule of derived triple \
                 {} {} {} cannot be attributed",
                triple.subject, triple.predicate, triple.object
            )));
        };
        // Recover the conclusion's typed terms from the materialized closure: a blank
        // node's original identity is not recoverable from its rendered skolem string, so
        // attribution re-explains over the typed terms the closure still holds.
        for quad in closure.quads() {
            let subj_value = closure.term_value(quad.s);
            let pred_value = closure.term_value(quad.p);
            let obj_value = closure.term_value(quad.o);
            let (Some(subject), Some(predicate), Some(object)) = (
                render_subject_value(&subj_value),
                pred_value.as_iri().map(str::to_owned),
                render_object_value(&obj_value),
            ) else {
                continue;
            };
            let graph_value = quad.g.map(|g| closure.term_value(g));
            let world = world_string_of(graph_value.as_ref());
            if subject == triple.subject
                && predicate == triple.predicate
                && object == triple.object
                && world == triple.world
            {
                return rule_name_for_conclusion(
                    edb.as_ref(),
                    graph_value.as_ref(),
                    &subj_value,
                    &pred_value,
                    &obj_value,
                );
            }
        }
        Err(reason_err(format!(
            "derived triple {} {} {} (world {}) is not a member of this RL closure",
            triple.subject, triple.predicate, triple.object, triple.world
        )))
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
/// (already valid N-Triples literals) pass through verbatim.
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

/// Render a literal's parts as its N-Triples object form (`"v"`, `"v"@lang`,
/// `"v"^^<dt>`) — the form a downstream N-Triples parser reads back losslessly.
fn literal_nt(lexical_form: &str, datatype: &str, language: Option<&str>) -> String {
    // N-Triples requires escaping `\`, `"`, newline, CR, and tab in the value.
    let escaped = lexical_form
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
    if let Some(lang) = language {
        format!("\"{escaped}\"@{lang}")
    } else if datatype == XSD_STRING {
        format!("\"{escaped}\"")
    } else {
        format!("\"{escaped}\"^^<{datatype}>")
    }
}

/// Render a closure subject term to the bare-string form [`RlTriple::subject`] holds:
/// an IRI verbatim, a blank node as its skolem IRI (so [`RlClosure::to_ntriples`]
/// re-derives a blank-node label). A literal or triple-term subject has no
/// standard-RDF form and drops the row — a literal can never be a triple subject and
/// a triple term is unsupported in this lane, exactly the rows the native authority
/// dropped.
fn render_subject_value(term: &TermValue) -> Option<String> {
    match term {
        TermValue::Iri(iri) => Some(iri.clone()),
        TermValue::Blank { label, .. } => Some(skolem_iri(label)),
        TermValue::Literal { .. } | TermValue::Triple { .. } => None,
    }
}

/// Render a closure object term to the N-Triples object form [`RlTriple::object`]
/// holds: `<iri>` for an IRI, `<skolem>` for a blank node (so
/// [`RlClosure::to_ntriples`] re-derives a blank-node label), the quoted literal form
/// for a literal. A triple-term object is unsupported and drops the row.
fn render_object_value(term: &TermValue) -> Option<String> {
    match term {
        TermValue::Iri(iri) => Some(format!("<{iri}>")),
        TermValue::Blank { label, .. } => Some(format!("<{}>", skolem_iri(label))),
        TermValue::Literal {
            lexical_form,
            datatype,
            language,
            ..
        } => Some(literal_nt(lexical_form, datatype, language.as_deref())),
        TermValue::Triple { .. } => None,
    }
}

/// The world an RDF 1.2 quad's fourth-position graph slot denotes: a named IRI graph
/// is its own world; a default or blank-node graph folds to [`DEFAULT_WORLD`], the
/// single world an un-named graph closes in — the decode inverse of
/// [`lower_edb_for_rl`]'s encode fold.
fn world_string_of(graph: Option<&TermValue>) -> String {
    match graph {
        Some(TermValue::Iri(iri)) => iri.clone(),
        _ => DEFAULT_WORLD.to_owned(),
    }
}

/// Lower the input EDB into the [`RdfDataset`] the OWL 2 RL chase closes.
///
/// Two boundary transforms, both matching the retired native encoder:
///
/// * **canonical → W3C spelling.** Every quad is emitted under each spelling
///   [`super::edb_predicate_spellings`] yields, so a canonical `logic:subClassOf` /
///   `logic:subPropertyOf` (or restriction slot) also appears under the `rdfs:`/`owl:`
///   spelling the fixed RL rules match. The authored canonical edge is kept too, so
///   the projection ADDS the W3C view rather than replacing the authored one, and both
///   are asserted.
/// * **world ⇔ graph** (see the module docs): a named IRI graph is its own world; a
///   default or blank-node graph folds to the [`DEFAULT_WORLD`] named graph.
///
/// A triple-term subject or object is skipped: it has no place in this lane's encoding
/// and never appears in an RL fixture. `Ok(None)` means the lowered EDB is empty (no
/// quad survived), for which the closure is empty and no chase is run.
///
/// # Errors
///
/// A [`Reason`](crate::error::Reason) diagnostic if the lowered dataset cannot be
/// frozen.
fn lower_edb_for_rl(edb: &RdfDataset) -> gmeow_errors::Result<Option<std::sync::Arc<RdfDataset>>> {
    let mut builder = RdfDatasetBuilder::new();
    let mut pushed = false;
    for quad in edb.owned_quads() {
        if matches!(quad.subject, RdfTerm::Triple(_)) || matches!(quad.object, RdfTerm::Triple(_)) {
            continue;
        }
        let world = match &quad.graph_name {
            Some(RdfTerm::Iri(iri)) => iri.clone(),
            _ => DEFAULT_WORLD.to_owned(),
        };
        for predicate in super::edb_predicate_spellings(&quad.predicate) {
            let lowered = RdfQuad::new(quad.subject.clone(), predicate, quad.object.clone())
                .in_graph(RdfTerm::iri(world.clone()));
            builder.push_owned_quad(&lowered);
            pushed = true;
        }
    }
    if !pushed {
        return Ok(None);
    }
    builder
        .freeze()
        .map(Some)
        .map_err(|e| reason_err(format!("freeze lowered RL EDB: {e}")))
}

/// The OWL 2 RL specification rule name to credit a DERIVED closure triple to.
///
/// purrdf's [`purrdf::entail::explain_conclusion`] rebuilds the conclusion's
/// derivation over the same lowered EDB, and [`ChaseProof::rules`] returns every rule
/// that derivation cites, in specification-table order, deduplicated. The single head
/// rule is not exposed on the public `ChaseProof` surface, so the deterministic
/// representative used here is the specification-table-first cited rule. A conclusion
/// citing no rule is asserted rather than derived and carries no name.
///
/// [`ChaseProof::rules`]: purrdf::entail::ChaseProof::rules
///
/// # Errors
///
/// A [`Reason`](crate::error::Reason) diagnostic if purrdf refuses to explain a triple
/// the closure derived — for OWL 2 RL (no existential heads) that never happens, so a
/// refusal is a real defect surfaced rather than a wrong or absent attribution.
fn rule_name_for_conclusion(
    lowered: &RdfDataset,
    graph: Option<&TermValue>,
    subject: &TermValue,
    predicate: &TermValue,
    object: &TermValue,
) -> gmeow_errors::Result<Option<String>> {
    let proof = purrdf::entail::explain_conclusion(
        lowered,
        purrdf::entail::Regime::OwlRl,
        graph,
        subject,
        predicate,
        object,
    )
    .map_err(|e| {
        reason_err(format!(
            "OWL 2 RL rule attribution refused for a derived triple \
             ({subject:?} {predicate:?} {object:?}): {e}"
        ))
    })?;
    Ok(proof.rules().first().map(|rule| rule.as_str().to_owned()))
}

/// Compute the OWL 2 RL/RDF deductive closure of `edb`.
///
/// Lowers `edb` into the chase dataset ([`lower_edb_for_rl`]), runs purrdf's OWL 2 RL
/// materialization once, and decodes every closure quad back into an [`RlTriple`]
/// (asserted + derived). The closure is world-scoped: a derived triple carries the
/// world of the graph that produced it.
///
/// # Errors
///
/// Returns an error if purrdf's materialization refuses (an evaluation ceiling, an
/// inconsistency witness, or a build failure) or if the rule attribution of a derived
/// triple refuses.
pub fn rl_closure(edb: &RdfDataset) -> gmeow_errors::Result<RlClosure> {
    let Some(lowered) = lower_edb_for_rl(edb)? else {
        return Ok(RlClosure {
            triples: vec![],
            edb: None,
            closure: None,
        });
    };

    let (closure, _report) =
        purrdf::entail::materialize(lowered.as_ref(), purrdf::entail::Materialization::OwlRl)
            .map_err(|e| reason_err(format!("OWL 2 RL materialization refused: {e}")))?;

    // The asserted rows of the lowered EDB, keyed by the same rendered surfaces the
    // closure decode produces, so a closure row present in the EDB reads `is_edb`.
    let mut edb_rows: std::collections::HashSet<(String, String, String, String)> =
        std::collections::HashSet::new();
    for quad in lowered.quads() {
        let (Some(subject), Some(predicate), Some(object)) = (
            render_subject_value(&lowered.term_value(quad.s)),
            lowered.term_value(quad.p).as_iri().map(str::to_owned),
            render_object_value(&lowered.term_value(quad.o)),
        ) else {
            continue;
        };
        let world = world_string_of(quad.g.map(|g| lowered.term_value(g)).as_ref());
        edb_rows.insert((subject, predicate, object, world));
    }

    let mut triples: Vec<RlTriple> = Vec::new();
    for quad in closure.quads() {
        let subj_value = closure.term_value(quad.s);
        let pred_value = closure.term_value(quad.p);
        let obj_value = closure.term_value(quad.o);
        let (Some(subject), Some(predicate), Some(object)) = (
            render_subject_value(&subj_value),
            pred_value.as_iri().map(str::to_owned),
            render_object_value(&obj_value),
        ) else {
            continue;
        };
        let graph_value = quad.g.map(|g| closure.term_value(g));
        let world = world_string_of(graph_value.as_ref());
        let is_edb = edb_rows.contains(&(
            subject.clone(),
            predicate.clone(),
            object.clone(),
            world.clone(),
        ));
        triples.push(RlTriple {
            subject,
            predicate,
            object,
            world,
            is_edb,
        });
    }
    Ok(RlClosure {
        triples,
        edb: Some(lowered),
        closure: Some(closure),
    })
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

    /// The canonical `logic:` subsumption spelling drives the fixed RDFS-vocabulary
    /// RL calculus. GMEOW's authored `module.ttl` surface spells subsumption
    /// `logic:subClassOf` / `logic:subPropertyOf` (Principle 17 — `rdfs:` is one of
    /// its projections), while the W3C RL rules match `rdfs:` by specification. A
    /// chase over authored sources must therefore lower the canonical edge at the
    /// EDB boundary, or a re-authored taxonomy derives NOTHING — silently, with an
    /// empty closure instead of an error.
    #[test]
    fn canonical_logic_subsumption_drives_the_rdfs_vocabulary_calculus() {
        let store = dataset(vec![
            quad(X, TYPE, A),
            quad(A, gmeow_ns::LOGIC_SUB_CLASS_OF, B),
            quad(B, gmeow_ns::LOGIC_SUB_CLASS_OF, C),
            quad(P1, gmeow_ns::LOGIC_SUB_PROPERTY_OF, P2),
            quad(X, P1, Y),
        ]);
        let c = rl_closure(store.as_ref()).expect("RL closure should succeed");
        assert!(
            has(&c, X, TYPE, B),
            "x a B via cax-sco over logic:subClassOf"
        );
        assert!(
            has(&c, X, TYPE, C),
            "x a C via cax-sco + scm-sco over logic:subClassOf"
        );
        assert!(
            has(&c, X, P2, Y),
            "x p2 y via prp-spo1 over logic:subPropertyOf"
        );
        // The projection ADDS the RDFS view; the authored canonical edge survives.
        assert!(
            has(&c, A, gmeow_ns::LOGIC_SUB_CLASS_OF, B),
            "the authored canonical edge is kept, not rewritten away"
        );
        assert!(
            has(&c, A, SUBCLASS, B),
            "the canonical edge is materialized under its rdfs: projection"
        );
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

    // ── rule-local coverage for the RL class-expression clause families ──
    // Each test exercises one clause family with the minimal axioms that make it
    // fire, in the same shape as the cls-svf/cls-int test above.

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
        assert!(has(&c, X, TYPE, C), "x a C via cls-oo");
        assert!(has(&c, Y, TYPE, C), "y a C via cls-oo");
    }

    #[test]
    fn cls_union_member_subclasses_each_member_to_the_union() {
        // C unionOf ( A B ) ⇒ A ⊑ C, B ⊑ C (OWL 2 RL scm-uni).
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
        assert!(has(&c, A, SUBCLASS, C), "A ⊑ C via scm-uni");
        assert!(has(&c, B, SUBCLASS, C), "B ⊑ C via scm-uni");
    }

    #[test]
    fn disjoint_union_is_not_an_owl_2_rl_subclass_entailment() {
        // C disjointUnionOf ( A B ) is a DL construct: `C ≡ A ⊔ B` with A, B pairwise
        // disjoint. OWL 2 RL's scm/cls rule tables have NO clause over
        // owl:disjointUnionOf (purrdf reads it only in the Direct-Semantics lane), so
        // the member-subclass edge the retired native custom rule derived is NOT an RL
        // entailment. This asserts that boundary — and that unionOf's scm-uni does not
        // spuriously fire on disjointUnionOf — so a regression that re-added the custom
        // rule, or mis-routed disjointUnionOf into scm-uni, would flip it.
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
            !has(&c, A, SUBCLASS, C),
            "A ⊑ C is not an OWL 2 RL entailment of owl:disjointUnionOf"
        );
        assert!(
            !has(&c, B, SUBCLASS, C),
            "B ⊑ C is not an OWL 2 RL entailment of owl:disjointUnionOf"
        );
    }

    #[test]
    fn literal_objects_round_trip_through_the_closure() {
        // A hyphenated language tag (`@x-gmeow-english`), an escaped quote, and a
        // typed integer all retain exact identity through the closure. prp-spo1 must
        // carry the literal object through unchanged.
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

        // The language literal propagates up the sub-property hierarchy unchanged.
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
        let rule_name = c
            .rule_name(derived)
            .expect("attribution should succeed for a derived triple");
        assert_eq!(
            rule_name.as_deref(),
            Some("cax-sco"),
            "derived triple cites the firing rule"
        );
        // An asserted (EDB) triple carries no firing rule.
        let asserted = c
            .triples
            .iter()
            .find(|t| t.is_edb)
            .expect("the closure keeps the asserted triples");
        assert_eq!(
            c.rule_name(asserted).expect("attribution should succeed"),
            None,
            "an asserted triple has no firing rule"
        );
    }

    #[test]
    fn to_ntriples_renders_blank_literal_dedups_and_sorts() {
        // The render: skolem IRI → blank-node label, literal pass-through, de-dup, and
        // byte-stable sort.
        let lit = |s: &str, p: &str, o: &str| RlTriple {
            subject: s.to_owned(),
            predicate: p.to_owned(),
            object: o.to_owned(),
            world: W.to_owned(),
            is_edb: false,
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
            edb: None,
            closure: None,
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
