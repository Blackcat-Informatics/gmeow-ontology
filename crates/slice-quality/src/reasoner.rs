// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The reasoner-derived axis: dogfood the native chase as the measuring device.
//!
//! The score is the **fraction of the slice's authored TBox axioms that are
//! load-bearing** — proven by leave-one-out: an axiom the reasoner re-derives
//! without it is closure-redundant (dead weight or an asserted derived fact,
//! Principle 12), caught mechanically rather than by a text heuristic. The measure
//! is intrinsically bounded — `1.0` means every authored axiom earns its place —
//! so there is nothing to calibrate. An unbounded entailments-per-triple *density
//! ratio* is deliberately NOT used: it has no principled 0-1 meaning.
//!
//! The proof compares only IRI-object triples (the DL calculus's structural output
//! — `rdf:type`, `rdfs:subClassOf`, `rdfs:domain`, characteristics, …).

use std::collections::BTreeSet;
use std::sync::Arc;

use gmeow_logic::reason::reason_all;
use purrdf::{DatasetView, GraphMatch, RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm, TermRef};

use crate::graph::id;
use crate::score::{AxisScore, ScoreContext, advisory};

const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";

/// A canonical `s\tp\to` key for an IRI-object triple.
fn key(subject: &str, predicate: &str, object: &str) -> String {
    format!("{subject}\t{predicate}\t{object}")
}

/// Normalize an inferred axiom's surface object to a bare IRI, or `None` when the
/// object is a literal / blank (not an IRI surface).
fn surface_iri(object: &str) -> Option<&str> {
    let o = object.trim();
    if o.starts_with('"') || o.is_empty() {
        return None;
    }
    Some(o.trim_start_matches('<').trim_end_matches('>'))
}

/// The closure's IRI-object triples, keyed the same way.
fn closure_iri_keys(result: &gmeow_logic::result::ReasoningResult) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for ax in result.inferred() {
        if let Some(o) = surface_iri(&ax.object) {
            out.insert(key(&ax.subject, &ax.predicate, o));
        }
    }
    out
}

/// The inferential OWL/RDFS predicates whose IRI-object triples are authored TBox
/// axioms — the population whose load-bearingness the reasoner axis measures.
const INFERENTIAL_PREDS: &[&str] = &[
    SUBCLASS,
    "http://www.w3.org/2000/01/rdf-schema#subPropertyOf",
    "http://www.w3.org/2000/01/rdf-schema#domain",
    "http://www.w3.org/2000/01/rdf-schema#range",
    "http://www.w3.org/2002/07/owl#disjointWith",
    "http://www.w3.org/2002/07/owl#equivalentClass",
    "http://www.w3.org/2002/07/owl#equivalentProperty",
    "http://www.w3.org/2002/07/owl#inverseOf",
];

/// The `rdf:type` objects that assert an OWL property characteristic (also authored
/// TBox axioms).
const CHARACTERISTICS: &[&str] = &[
    "http://www.w3.org/2002/07/owl#TransitiveProperty",
    "http://www.w3.org/2002/07/owl#SymmetricProperty",
    "http://www.w3.org/2002/07/owl#AsymmetricProperty",
    "http://www.w3.org/2002/07/owl#ReflexiveProperty",
    "http://www.w3.org/2002/07/owl#IrreflexiveProperty",
    "http://www.w3.org/2002/07/owl#FunctionalProperty",
    "http://www.w3.org/2002/07/owl#InverseFunctionalProperty",
];

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// The slice's authored TBox logical axioms as `(s, p, o)` IRI triples — the
/// inferential-predicate triples plus the property-characteristic assertions.
/// Annotation and A-Box data are excluded: they are not axioms doing inferential
/// work, and counting them would penalize a slice for having ordinary content.
fn authored_axioms(ds: &RdfDataset) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for pred in INFERENTIAL_PREDS {
        if let Some(pid) = id(ds, pred) {
            for q in ds.quads_for_pattern(None, Some(pid), None, GraphMatch::Any) {
                if let (TermRef::Iri(s), TermRef::Iri(o)) = (ds.resolve(q.s), ds.resolve(q.o)) {
                    out.push((s.to_owned(), (*pred).to_owned(), o.to_owned()));
                }
            }
        }
    }
    if let Some(type_id) = id(ds, RDF_TYPE) {
        for characteristic in CHARACTERISTICS {
            if let Some(cid) = id(ds, characteristic) {
                for q in ds.quads_for_pattern(None, Some(type_id), Some(cid), GraphMatch::Any) {
                    if let TermRef::Iri(s) = ds.resolve(q.s) {
                        out.push((
                            s.to_owned(),
                            RDF_TYPE.to_owned(),
                            (*characteristic).to_owned(),
                        ));
                    }
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// `rdfs:subClassOf` triples between two named classes — retained for the public
/// closure-redundancy proof helper the acceptance fixture drives.
fn named_subclass_triples(ds: &RdfDataset) -> Vec<(String, String)> {
    authored_axioms(ds)
        .into_iter()
        .filter(|(_, p, _)| p == SUBCLASS)
        .map(|(s, _, o)| (s, o))
        .collect()
}

/// Rebuild the dataset without the single IRI triple `(s, p, o)`, preserving every
/// other IRI-object triple (the structural facts DL reasoning consumes). Literal
/// and blank triples are dropped — they do not affect the structural closure.
fn edb_without_triple(
    ds: &RdfDataset,
    drop_s: &str,
    drop_p: &str,
    drop_o: &str,
) -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for q in ds.quads_for_pattern(None, None, None, GraphMatch::Any) {
        if let (TermRef::Iri(s), TermRef::Iri(p), TermRef::Iri(o)) =
            (ds.resolve(q.s), ds.resolve(q.p), ds.resolve(q.o))
        {
            if s == drop_s && p == drop_p && o == drop_o {
                continue; // the triple under test — leave it out
            }
            let quad = RdfQuad::new(
                RdfTerm::iri(s.to_owned()),
                p.to_owned(),
                RdfTerm::iri(o.to_owned()),
            );
            builder.push_owned_quad(&quad);
        }
    }
    builder
        .freeze()
        .unwrap_or_else(|_| Arc::new(RdfDataset::union(&[])))
}

/// The reasoner-derived axis primitive.
///
/// The score is the **fraction of the slice's authored TBox axioms that are
/// load-bearing** — not closure-redundant. An axiom is load-bearing when the
/// reasoner does NOT re-derive it once it is left out (leave-one-out proof); a
/// re-derived axiom is dead weight or an asserted derived fact (Principle 12). The
/// score has intrinsic 0-1 meaning (`1.0` = no redundant axioms, every one earns
/// its place) — there is no density ratio and nothing to calibrate. A slice with
/// no TBox axioms is vacuously non-redundant (1.0) and gets an informational note.
pub fn reasoner_axis(ctx: &ScoreContext) -> AxisScore {
    let ds = ctx.graph;
    // Reasoning must succeed once to establish the baseline closure exists.
    if let Err(e) = reason_all(ds) {
        return AxisScore {
            score: 0.0,
            findings: vec![advisory(
                "slice-quality.reasoner.no-closure",
                format!("the native reasoner could not establish a closure for the slice: {e}"),
            )],
        };
    }

    let axioms = authored_axioms(ds);
    if axioms.is_empty() {
        return AxisScore {
            score: 1.0,
            findings: vec![advisory(
                "slice-quality.reasoner.no-axioms",
                "the slice asserts no TBox logical axioms (subclass/domain/range/characteristics) — it does no inferential work (Principles 8/18).".to_owned(),
            )],
        };
    }

    // Leave-one-out over every authored axiom (bounded): if the reasoner re-derives
    // it without it, the axiom is closure-redundant and not load-bearing.
    let cap = axioms.len().min(REDUNDANCY_CAP);
    let mut redundant = 0usize;
    let mut findings = Vec::new();
    for (s, p, o) in axioms.iter().take(cap) {
        let reduced = edb_without_triple(ds, s, p, o);
        if let Ok(r2) = reason_all(&reduced)
            && closure_iri_keys(&r2).contains(&key(s, p, o))
        {
            redundant += 1;
            findings.push(advisory(
                "slice-quality.reasoner.closure-redundant",
                format!("<{s}> <{p}> <{o}> is re-derived by the reasoner without being asserted — it is closure-redundant (dead weight or an asserted derived fact, Principle 12)."),
            ));
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let score = (cap - redundant) as f64 / cap as f64;
    AxisScore {
        score: score.clamp(0.0, 1.0),
        findings,
    }
}

/// The most authored axioms the always-on axis probes for redundancy.
const REDUNDANCY_CAP: usize = 64;

/// Public proof helper: the named `subClassOf` triples the reasoner re-derives
/// without them (closure-redundant). Exposed for the acceptance fixture.
///
/// # Errors
/// Returns a message if reasoning fails on a reduced graph.
pub fn closure_redundant_subclasses(ds: &RdfDataset) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for (s, o) in named_subclass_triples(ds) {
        let reduced = edb_without_triple(ds, &s, SUBCLASS, &o);
        let r = reason_all(&reduced)?;
        if closure_iri_keys(&r).contains(&key(&s, SUBCLASS, &o)) {
            out.push((s, o));
        }
    }
    Ok(out)
}
