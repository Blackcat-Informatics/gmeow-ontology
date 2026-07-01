// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Per-subject subgraph extraction — the **Symmetric Concise Bounded Description**
//! (SCBD) of a resource.
//!
//! The documentation site exports each term's (and each slice's) RDF in every
//! serialization format. To do that it needs the subgraph that *describes* a subject.
//! A plain CBD (outgoing triples + forward blank closure) would under-represent the
//! very thing GMEOW exists to showcase: the **incoming** links — `skos:exactMatch`
//! targets, `rdfs:subPropertyOf`/`subClassOf` children, authority back-references. So
//! `describe` returns the **symmetric** CBD:
//!
//! 1. every triple where the subject is the **subject** (outgoing), and
//! 2. every triple where the subject is the **object** (incoming), and
//! 3. the transitive **blank-node** closure on both directions (a definition hung off
//!    a blank restriction surfaces in full), and
//! 4. the RDF-1.2 statement-layer **reifiers** whose reified triple's subject *or*
//!    object lies in the closure, together with their annotations.
//!
//! Named-node endpoints do **not** expand (that would pull in the whole graph); only
//! blank nodes do. Reification is standpoint-scoped and carries no graph dimension, so
//! reifiers are selected by reified-triple membership, never by graph.
//!
//! The extracted subgraph is a fresh, structurally valid [`RdfDataset`] that can be
//! handed straight to the `native_codecs` serializers (Turtle / N-Triples / N-Quads /
//! TriG / RDF-XML) and the JSON-LD serializer — the one serialization seam.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::Arc;

use gmeow_rdf_core::{
    QuadIds, RdfDataset, RdfDatasetBuilder, RdfDiagnostic, TermId, TermRef, TermValue,
};

/// A reusable extractor: it builds the subject/object adjacency and the reifier
/// endpoint index **once**, so extracting the SCBD of many subjects (one per exported
/// term/slice) is cheap — each extraction is a bounded graph walk over the index, not
/// a full re-scan of the dataset.
pub struct Describer<'a> {
    dataset: &'a RdfDataset,
    /// term id → the quads that touch it as subject or object.
    by_endpoint: BTreeMap<TermId, Vec<QuadIds>>,
    /// term id → the `(reifier, triple-term)` bindings whose reified triple has this
    /// id as its subject or object.
    reifiers_by_endpoint: BTreeMap<TermId, Vec<(TermId, TermId)>>,
}

impl<'a> Describer<'a> {
    /// Build the adjacency indices over `dataset`.
    #[must_use]
    pub fn new(dataset: &'a RdfDataset) -> Self {
        let mut by_endpoint: BTreeMap<TermId, Vec<QuadIds>> = BTreeMap::new();
        for q in dataset.quads() {
            by_endpoint.entry(q.s).or_default().push(q);
            // Avoid double-listing a reflexive `s p s` quad under the same key.
            if q.o != q.s {
                by_endpoint.entry(q.o).or_default().push(q);
            }
        }

        let mut reifiers_by_endpoint: BTreeMap<TermId, Vec<(TermId, TermId)>> = BTreeMap::new();
        for (reifier, triple) in dataset.reifiers() {
            if let TermRef::Triple { s, p: _, o } = dataset.resolve(triple) {
                reifiers_by_endpoint
                    .entry(s)
                    .or_default()
                    .push((reifier, triple));
                if o != s {
                    reifiers_by_endpoint
                        .entry(o)
                        .or_default()
                        .push((reifier, triple));
                }
            }
        }

        Self {
            dataset,
            by_endpoint,
            reifiers_by_endpoint,
        }
    }

    /// The SCBD of the IRI `subject`, or an **empty** dataset if the dataset contains
    /// no such subject. (An absent subject is not an error — a term may legitimately
    /// carry no asserted or incoming triples.)
    ///
    /// # Errors
    /// Propagates a freeze diagnostic if the extracted subgraph is somehow invalid
    /// (it never should be, being a subset of an already-valid dataset).
    pub fn describe_iri(&self, subject: &str) -> Result<Arc<RdfDataset>, RdfDiagnostic> {
        let seed = self
            .dataset
            .term_id_by_value(&TermValue::Iri(subject.to_string()));
        self.describe_seeds(seed.into_iter().collect())
    }

    /// The union SCBD of several IRI subjects — the slice-scope export (every subject
    /// the slice module mints, described as one subgraph).
    ///
    /// # Errors
    /// Propagates a freeze diagnostic (see [`describe_iri`](Self::describe_iri)).
    pub fn describe_iris(
        &self,
        subjects: impl IntoIterator<Item = &'a str>,
    ) -> Result<Arc<RdfDataset>, RdfDiagnostic> {
        let seeds: Vec<TermId> = subjects
            .into_iter()
            .filter_map(|s| {
                self.dataset
                    .term_id_by_value(&TermValue::Iri(s.to_string()))
            })
            .collect();
        self.describe_seeds(seeds)
    }

    /// The shared walk: BFS from `seeds`, expanding only blank-node endpoints, then
    /// re-intern the collected quads + statement layer into a fresh dataset.
    fn describe_seeds(&self, seeds: Vec<TermId>) -> Result<Arc<RdfDataset>, RdfDiagnostic> {
        let mut anchors: BTreeSet<TermId> = BTreeSet::new();
        let mut frontier: Vec<TermId> = Vec::new();
        for s in seeds {
            if anchors.insert(s) {
                frontier.push(s);
            }
        }

        let mut quads: HashSet<QuadIds> = HashSet::new();
        while let Some(anchor) = frontier.pop() {
            let Some(touching) = self.by_endpoint.get(&anchor) else {
                continue;
            };
            for &q in touching {
                quads.insert(q);
                // Only blank-node endpoints expand the closure; a named node would
                // drag in the entire neighbourhood of the graph.
                for end in [q.s, q.o] {
                    if self.is_blank(end) && anchors.insert(end) {
                        frontier.push(end);
                    }
                }
            }
        }

        // Reifiers whose reified triple is about any anchored node, plus their
        // annotations.
        let mut reifiers: BTreeSet<(TermId, TermId)> = BTreeSet::new();
        for anchor in &anchors {
            if let Some(bindings) = self.reifiers_by_endpoint.get(anchor) {
                for &b in bindings {
                    reifiers.insert(b);
                }
            }
        }
        let reifier_ids: BTreeSet<TermId> = reifiers.iter().map(|&(r, _)| r).collect();

        // Re-intern the selected quads + statement layer into a fresh dataset. A remap
        // memoizes old-id → new-id so the owned-term round-trip runs once per term.
        let mut builder = RdfDatasetBuilder::new();
        let mut remap: BTreeMap<TermId, TermId> = BTreeMap::new();
        for q in &quads {
            let s = self.map_id(&mut builder, &mut remap, q.s);
            let p = self.map_id(&mut builder, &mut remap, q.p);
            let o = self.map_id(&mut builder, &mut remap, q.o);
            let g = q.g.map(|g| self.map_id(&mut builder, &mut remap, g));
            builder.push_quad(s, p, o, g);
        }
        for &(reifier, triple) in &reifiers {
            let r = self.map_id(&mut builder, &mut remap, reifier);
            let t = self.map_id(&mut builder, &mut remap, triple);
            builder.push_reifier(r, t);
        }
        for (reifier, p, o) in self.dataset.annotations() {
            if reifier_ids.contains(&reifier) {
                let r = self.map_id(&mut builder, &mut remap, reifier);
                let p = self.map_id(&mut builder, &mut remap, p);
                let o = self.map_id(&mut builder, &mut remap, o);
                builder.push_annotation(r, p, o);
            }
        }

        builder.freeze()
    }

    /// Whether a term id is a blank node in the source dataset.
    fn is_blank(&self, id: TermId) -> bool {
        matches!(self.dataset.resolve(id), TermRef::Blank { .. })
    }

    /// Intern a source term id into `builder`, memoized. `to_owned_term` recurses
    /// through triple terms, and `intern_owned_term` re-interns them, so quoted
    /// triples inside reifiers rebuild faithfully.
    fn map_id(
        &self,
        builder: &mut RdfDatasetBuilder,
        remap: &mut BTreeMap<TermId, TermId>,
        old: TermId,
    ) -> TermId {
        if let Some(&new) = remap.get(&old) {
            return new;
        }
        let owned = self.dataset.to_owned_term(old);
        let new = builder.intern_owned_term(&owned);
        remap.insert(old, new);
        new
    }
}

/// One-shot convenience: the SCBD of a single IRI subject in `dataset`.
///
/// For extracting many subjects (the docs export walks every term) build a
/// [`Describer`] once and reuse it — this rebuilds the adjacency index per call.
///
/// # Errors
/// Propagates a freeze diagnostic (see [`Describer::describe_iri`]).
pub fn describe(dataset: &RdfDataset, subject: &str) -> Result<Arc<RdfDataset>, RdfDiagnostic> {
    Describer::new(dataset).describe_iri(subject)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_rdf_core::{RdfLiteral, RdfQuad, RdfTerm};

    const S: &str = "https://e/s";
    const OTHER: &str = "https://e/other";

    fn iri(v: &str) -> RdfTerm {
        RdfTerm::iri(v)
    }

    /// Build a dataset from owned quads (default graph) with an optional reifier +
    /// annotation on the first quad.
    fn dataset(quads: &[RdfQuad]) -> Arc<RdfDataset> {
        let mut b = RdfDatasetBuilder::new();
        for q in quads {
            b.push_owned_quad(q);
        }
        b.freeze().expect("freeze test dataset")
    }

    fn triple(s: &str, p: &str, o: RdfTerm) -> RdfQuad {
        RdfQuad::new(iri(s), p.to_string(), o)
    }

    /// The set of `(s, p, o)` IRI/lexical strings in a described subgraph, for terse
    /// membership assertions (blank labels are scope-qualified, so compare by kind).
    fn objects_for(ds: &RdfDataset, subject: &str, predicate: &str) -> Vec<String> {
        let mut out = Vec::new();
        for q in ds.quad_refs() {
            let s = match q.s {
                TermRef::Iri(i) => i.to_string(),
                _ => continue,
            };
            let p = match q.p {
                TermRef::Iri(i) => i.to_string(),
                _ => continue,
            };
            if s == subject && p == predicate {
                if let TermRef::Iri(o) = q.o {
                    out.push(o.to_string());
                }
            }
        }
        out.sort();
        out
    }

    #[test]
    fn describes_outgoing_triples() {
        let ds = dataset(&[
            triple(S, "https://e/p", iri("https://e/o1")),
            triple(S, "https://e/p", iri("https://e/o2")),
            triple(OTHER, "https://e/p", iri("https://e/x")),
        ]);
        let scbd = describe(&ds, S).unwrap();
        assert_eq!(
            objects_for(&scbd, S, "https://e/p"),
            vec!["https://e/o1".to_string(), "https://e/o2".to_string()]
        );
        // The unrelated OTHER subject's triple must NOT be pulled in.
        assert!(objects_for(&scbd, OTHER, "https://e/p").is_empty());
    }

    #[test]
    fn describes_incoming_triples_symmetrically() {
        // A plain (forward-only) CBD would miss this: OTHER points AT S.
        let ds = dataset(&[triple(OTHER, "https://e/refersTo", iri(S))]);
        let scbd = describe(&ds, S).unwrap();
        assert_eq!(
            objects_for(&scbd, OTHER, "https://e/refersTo"),
            vec![S.to_string()],
            "the incoming link OTHER -> S must be present in the symmetric CBD"
        );
    }

    #[test]
    fn named_node_neighbours_do_not_expand() {
        // S -> N, and N -> deep. `deep` must NOT come along: named nodes don't expand.
        let ds = dataset(&[
            triple(S, "https://e/p", iri("https://e/n")),
            triple("https://e/n", "https://e/p", iri("https://e/deep")),
        ]);
        let scbd = describe(&ds, S).unwrap();
        // The N -> deep triple is neither outgoing-from nor incoming-to S, so absent.
        assert!(objects_for(&scbd, "https://e/n", "https://e/p").is_empty());
        assert_eq!(
            objects_for(&scbd, S, "https://e/p"),
            vec!["https://e/n".to_string()]
        );
    }

    #[test]
    fn blank_nodes_expand_transitively() {
        // S -> _:b (restriction) -> onProperty target. The blank closure must bring the
        // blank's own triples along, both hops.
        let mut b = RdfDatasetBuilder::new();
        let s = b.intern_iri(S.to_string());
        let has = b.intern_iri("https://e/hasRestriction".to_string());
        let bnode = b.intern_blank("r1".to_string(), gmeow_rdf_core::BlankScope::DEFAULT);
        let on = b.intern_iri("https://e/onProperty".to_string());
        let target = b.intern_iri("https://e/target".to_string());
        b.push_quad(s, has, bnode, None);
        b.push_quad(bnode, on, target, None);
        let ds = b.freeze().unwrap();

        let scbd = describe(&ds, S).unwrap();
        // Both quads survive: S -> _:b and _:b -> target.
        assert_eq!(
            scbd.quad_count(),
            2,
            "blank-node closure must keep both hops"
        );
        // The blank's onProperty edge is present (object is the named target).
        let has_target = scbd.quad_refs().any(|q| {
            matches!(q.p, TermRef::Iri(i) if i == "https://e/onProperty")
                && matches!(q.o, TermRef::Iri(i) if i == "https://e/target")
        });
        assert!(has_target, "the blank node's own triple must be included");
    }

    #[test]
    fn absent_subject_yields_empty() {
        let ds = dataset(&[triple(S, "https://e/p", iri("https://e/o"))]);
        let scbd = describe(&ds, "https://e/nope").unwrap();
        assert_eq!(scbd.quad_count(), 0);
    }

    #[test]
    fn includes_reifiers_about_the_subject() {
        // S p o, with a reifier annotating that statement (a certainty note).
        let mut b = RdfDatasetBuilder::new();
        let s = b.intern_iri(S.to_string());
        let p = b.intern_iri("https://e/p".to_string());
        let o = b.intern_iri("https://e/o".to_string());
        b.push_quad(s, p, o, None);
        let triple_term = b.intern_triple(s, p, o);
        let reifier = b.intern_blank("stmt1".to_string(), gmeow_rdf_core::BlankScope::DEFAULT);
        b.push_reifier(reifier, triple_term);
        let certainty = b.intern_iri("https://e/certainty".to_string());
        let high = b.intern_literal(RdfLiteral::simple("high"));
        b.push_annotation(reifier, certainty, high);
        let ds = b.freeze().unwrap();

        let scbd = describe(&ds, S).unwrap();
        // The reifier binding (reifier rdf:reifies << s p o >>) and its annotation ride
        // along because the reified statement's subject is S.
        assert_eq!(
            scbd.reifiers().count(),
            1,
            "the reifier about S must be kept"
        );
        assert_eq!(scbd.annotations().count(), 1, "its annotation must be kept");
    }

    #[test]
    fn slice_scope_unions_subjects() {
        let ds = dataset(&[
            triple(S, "https://e/p", iri("https://e/o1")),
            triple(OTHER, "https://e/p", iri("https://e/o2")),
        ]);
        let d = Describer::new(&ds);
        let scbd = d.describe_iris([S, OTHER]).unwrap();
        assert_eq!(scbd.quad_count(), 2, "both subjects' triples in the union");
    }

    #[test]
    fn round_trips_through_every_serializer() {
        use crate::native_codecs::jsonld::serialize_dataset_to_jsonld;
        use crate::{parse_dataset, serialize_dataset, SerializeGraph};

        let ds = dataset(&[
            triple(S, "https://e/p", iri("https://e/o")),
            triple(
                S,
                "https://e/label",
                RdfTerm::literal(RdfLiteral::simple("hi")),
            ),
        ]);
        let scbd = describe(&ds, S).unwrap();

        // Every native RDF format serializes non-empty bytes.
        for media in [
            "text/turtle",
            "application/n-triples",
            "application/n-quads",
            "application/trig",
            "application/rdf+xml",
        ] {
            let bytes = serialize_dataset(&scbd, media, SerializeGraph::Dataset)
                .unwrap_or_else(|e| panic!("serialize {media}: {e}"));
            assert!(!bytes.is_empty(), "{media} produced empty output");
        }
        // JSON-LD rides the separate native_codecs path (not a NativeRdfFormat).
        let jsonld = serialize_dataset_to_jsonld(&scbd).expect("jsonld");
        assert!(jsonld.trim_start().starts_with('{') || jsonld.contains("@graph"));

        // A Turtle round-trip preserves the two triples.
        let ttl = serialize_dataset(&scbd, "text/turtle", SerializeGraph::Dataset).unwrap();
        let back = parse_dataset(&ttl, "text/turtle", None).unwrap();
        assert_eq!(back.quad_count(), 2);
    }
}
