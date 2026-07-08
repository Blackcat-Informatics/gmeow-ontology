// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The reasoner-derived axis: dogfood the native chase as the measuring device.
//!
//! Two metrics, both read from the DL closure rather than from text:
//! - **inferential density** — new structural entailments per authored triple. A
//!   slice whose axioms entail nothing in the closure is decoration.
//! - **closure redundancy by proof** — an authored `subClassOf` triple the reasoner
//!   re-derives without it (leave-one-out) is dead weight or an asserted derived
//!   fact (Principle 12), caught mechanically, not by a heuristic.
//!
//! The metrics compare only IRI-object triples (the DL calculus's structural
//! output — `rdf:type`, `rdfs:subClassOf`, `rdfs:subPropertyOf`, …); literal
//! entailments are not part of the structural inferential-work signal.

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

/// Every EDB IRI-object triple, keyed for closure comparison.
fn edb_iri_keys(ds: &RdfDataset) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for q in ds.quads_for_pattern(None, None, None, GraphMatch::Any) {
        if let (TermRef::Iri(s), TermRef::Iri(p), TermRef::Iri(o)) =
            (ds.resolve(q.s), ds.resolve(q.p), ds.resolve(q.o))
        {
            out.insert(key(s, p, o));
        }
    }
    out
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

/// `rdfs:subClassOf` triples between two named classes — the DL-derivable
/// candidates worth a leave-one-out redundancy probe.
fn named_subclass_triples(ds: &RdfDataset) -> Vec<(String, String)> {
    let Some(subclass) = id(ds, SUBCLASS) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for q in ds.quads_for_pattern(None, Some(subclass), None, GraphMatch::Any) {
        if let (TermRef::Iri(s), TermRef::Iri(o)) = (ds.resolve(q.s), ds.resolve(q.o)) {
            out.push((s.to_owned(), o.to_owned()));
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Rebuild the dataset without the single `subClassOf` triple `(s, o)`, preserving
/// every IRI-object triple (the structural facts DL reasoning consumes). Literal
/// and blank triples are dropped — they do not affect `subClassOf` derivation.
fn edb_without_subclass(ds: &RdfDataset, drop_s: &str, drop_o: &str) -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for q in ds.quads_for_pattern(None, None, None, GraphMatch::Any) {
        if let (TermRef::Iri(s), TermRef::Iri(p), TermRef::Iri(o)) =
            (ds.resolve(q.s), ds.resolve(q.p), ds.resolve(q.o))
        {
            if p == SUBCLASS && s == drop_s && o == drop_o {
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
pub fn reasoner_axis(ctx: &ScoreContext) -> AxisScore {
    let ds = ctx.graph;
    let result = match reason_all(ds) {
        Ok(r) => r,
        Err(e) => {
            return AxisScore {
                score: 0.0,
                findings: vec![advisory(
                    "slice-quality.reasoner.no-closure",
                    format!("the native reasoner could not establish a closure for the slice: {e}"),
                )],
            };
        }
    };

    let edb = edb_iri_keys(ds);
    let closure = closure_iri_keys(&result);
    let new_entailments = closure.difference(&edb).count();
    #[allow(clippy::cast_precision_loss)]
    let density = new_entailments as f64 / edb.len().max(1) as f64;
    let mut score = density.min(1.0);
    let mut findings = Vec::new();
    if new_entailments == 0 {
        findings.push(advisory(
            "slice-quality.reasoner.inert",
            "the slice's axioms entail nothing new in the closure — strengthen characteristics, disjointness, or rules (Principles 8/18)."
                .to_owned(),
        ));
    }

    // Closure-redundancy by proof: bounded leave-one-out over named subClassOf.
    let candidates = named_subclass_triples(ds);
    let cap = candidates.len().min(REDUNDANCY_CAP);
    let mut redundant = 0usize;
    for (s, o) in candidates.iter().take(cap) {
        let reduced = edb_without_subclass(ds, s, o);
        if let Ok(r2) = reason_all(&reduced)
            && closure_iri_keys(&r2).contains(&key(s, SUBCLASS, o))
        {
            redundant += 1;
            findings.push(advisory(
                "slice-quality.reasoner.closure-redundant",
                format!("{s} rdfs:subClassOf {o} is re-derived by the reasoner without being asserted — it is closure-redundant (dead weight or an asserted derived fact, Principle 12)."),
            ));
        }
    }
    if cap > 0 {
        #[allow(clippy::cast_precision_loss)]
        let redundancy_ratio = redundant as f64 / cap as f64;
        score *= 1.0 - 0.5 * redundancy_ratio;
    }

    AxisScore {
        score: score.clamp(0.0, 1.0),
        findings,
    }
}

/// The most named-subClassOf triples the always-on axis probes for redundancy.
const REDUNDANCY_CAP: usize = 12;

/// Public proof helper: the named `subClassOf` triples the reasoner re-derives
/// without them (closure-redundant). Exposed for the acceptance fixture.
///
/// # Errors
/// Returns a message if reasoning fails on a reduced graph.
pub fn closure_redundant_subclasses(ds: &RdfDataset) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for (s, o) in named_subclass_triples(ds) {
        let reduced = edb_without_subclass(ds, &s, &o);
        let r = reason_all(&reduced)?;
        if closure_iri_keys(&r).contains(&key(&s, SUBCLASS, &o)) {
            out.push((s, o));
        }
    }
    Ok(out)
}
