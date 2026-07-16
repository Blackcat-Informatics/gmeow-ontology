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
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::Arc;

use gmeow_errors::Finding;
#[cfg(test)]
use gmeow_logic::reason::InferredAxiom;
use gmeow_logic::reason::{LeaveOneOutAxiom, dl_consistency, leave_one_out_rederived};
use purrdf::{DatasetView, GraphMatch, RdfDataset, TermRef};
#[cfg(test)]
use purrdf::{RdfDatasetBuilder, RdfTerm};

use crate::graph::id;
use crate::model::GMEOW;
use crate::score::{AxisScore, ScoreContext, advisory};

const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
/// The SHACL namespace — the shape vocabulary the disjointness-projection check reads.
const SH_NS: &str = "http://www.w3.org/ns/shacl#";

#[cfg(test)]
/// Normalize an inferred axiom's surface object to a bare IRI, or `None` when the
/// object is a literal / blank (not an IRI surface).
fn surface_iri(object: &str) -> Option<&str> {
    let o = object.trim();
    if o.starts_with('"') || o.is_empty() {
        return None;
    }
    Some(o.trim_start_matches('<').trim_end_matches('>'))
}

#[cfg(test)]
/// Whether the closure contains one target IRI-object axiom.
///
/// A leave-one-out probe asks exactly one membership question. Borrowing the existing
/// strings and stopping on the first match avoids constructing a complete
/// `BTreeSet<String>` for every probe (up to 64 full closure re-indexes per slice).
fn closure_contains_iri(
    inferred: &[InferredAxiom],
    subject: &str,
    predicate: &str,
    object: &str,
) -> bool {
    inferred.iter().any(|axiom| {
        axiom.subject == subject
            && axiom.predicate == predicate
            && surface_iri(&axiom.object) == Some(object)
    })
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

#[cfg(test)]
/// Rebuild the dataset without the single IRI triple `(s, p, o)`, preserving every
/// OTHER quad of every kind — blank-node (`owl:Restriction`-encoded) and literal
/// quads included. Only the exact `(s, p, o)` triple under test is removed; because
/// OWL restrictions and equivalences are blank-node encoded, dropping them would
/// corrupt the reasoned closure and thus the redundancy / clash scores. Blank
/// identity is preserved by round-tripping through the dataset's own
/// scope-qualified owned model (`owned_quads`), so co-referring blanks stay
/// co-referring after the rebuild.
fn edb_without_triple(
    ds: &RdfDataset,
    drop_s: &str,
    drop_p: &str,
    drop_o: &str,
) -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for quad in ds.owned_quads() {
        // The target axiom is always an IRI→IRI-predicate→IRI triple; drop exactly
        // that one and preserve everything else regardless of term kind.
        if quad.predicate == drop_p
            && let (RdfTerm::Iri(s), RdfTerm::Iri(o)) = (&quad.subject, &quad.object)
            && s == drop_s
            && o == drop_o
        {
            continue; // the triple under test — leave it out
        }
        builder.push_owned_quad(&quad);
    }
    builder
        .freeze()
        .unwrap_or_else(|_| Arc::new(RdfDataset::union(&[])))
}

/// Fold the exact batch leave-one-out verdicts into findings in authored-axiom order.
fn redundancy_finding(
    (subject, predicate, object): &(String, String, String),
    redundant: bool,
) -> Option<Finding> {
    redundant.then(|| {
        advisory(
            "slice-quality.reasoner.closure-redundant",
            format!(
                "<{subject}> <{predicate}> <{object}> is re-derived by the reasoner without being asserted — it is closure-redundant (dead weight or an asserted derived fact, Principle 12)."
            ),
        )
    })
}

fn redundancy_probes(
    ds: &RdfDataset,
    axioms: &[(String, String, String)],
) -> gmeow_errors::Result<Vec<Option<Finding>>> {
    let probes = axioms
        .iter()
        .map(|(subject, predicate, object)| LeaveOneOutAxiom::new(subject, predicate, object))
        .collect::<Vec<_>>();
    let rederived = leave_one_out_rederived(ds, &probes)?;
    Ok(axioms
        .iter()
        .zip(rederived)
        .map(|(axiom, redundant)| redundancy_finding(axiom, redundant))
        .collect())
}

/// The reasoner-derived axis primitive.
///
/// The axis dogfoods the native chase in TWO complementary directions, counting the
/// **reasoner obligations the slice meets** over the total it takes on:
///
/// * **Positive space (leave-one-out redundancy):** an authored TBox axiom is
///   load-bearing when the reasoner does NOT re-derive it once it is left out; a
///   re-derived axiom is dead weight or an asserted derived fact (Principle 12).
/// * **Negative space (counter-example clash):** a counter-example fixture whose
///   SHACL shape declares a class-disjointness (`sh:not [ sh:class B ]` on a shape
///   targeting `A`) MUST, when reasoned with the module, force a DL clash — because
///   the SHACL disjointness is a lossy projection of the canonical `A owl:disjointWith
///   B` (Principle 17). A co-typed counter-example the native reasoner finds
///   CONSISTENT is a silent hole: the negative space lives only in the SHACL
///   projection, not in the logic core.
///
/// The combined score is the met-obligation fraction `(load-bearing axioms +
/// clashing logical counter-examples) / (authored axioms + logical counter-examples)`.
///
/// The measure is an intrinsically bounded fraction of met obligations (`1.0` = every
/// axiom earns its place AND every declared logical contradiction actually clashes) —
/// no density ratio, nothing to calibrate. A slice with neither authored axioms nor
/// logical counter-examples is vacuously perfect (1.0) with an informational note.
pub fn reasoner_axis(ctx: &ScoreContext) -> AxisScore {
    let ds = ctx.graph;

    let mut findings = Vec::new();

    // ── Positive space: leave-one-out redundancy over the authored axioms ──────
    let axioms = authored_axioms(ds);
    let cap = axioms.len().min(REDUNDANCY_CAP);
    // The probe reads only the closure's one target IRI-object triple, never the DL
    // verdict, so it takes the verdict-free closure entry point and performs a borrowed
    // early-exit scan instead of indexing the complete closure.
    let probe_findings = match redundancy_probes(ds, &axioms[..cap]) {
        Ok(findings) => findings,
        Err(error) => {
            return AxisScore {
                score: 0.0,
                findings: vec![advisory(
                    "slice-quality.reasoner.no-closure",
                    format!("the native reasoner could not complete leave-one-out: {error}"),
                )],
            };
        }
    };
    let redundant = probe_findings
        .iter()
        .filter(|finding| finding.is_some())
        .count();
    findings.extend(probe_findings.into_iter().flatten());
    let load_bearing = cap - redundant;

    // ── Negative space: counter-example clash verification ─────────────────────
    let clash = counterexample_clash_check(ctx);
    findings.extend(clash.findings);

    // Fully vacuous: no positive obligations and no logical negative space to prove.
    if cap == 0 && clash.population == 0 {
        findings.push(advisory(
            "slice-quality.reasoner.no-obligations",
            "the slice asserts no TBox logical axioms (subclass/domain/range/characteristics) and declares no class-disjointness counter-example — it does no inferential work and pins no DL-provable negative space (Principles 8/17/18).".to_owned(),
        ));
        return AxisScore {
            score: 1.0,
            findings,
        };
    }

    #[allow(clippy::cast_precision_loss)]
    let score = (load_bearing + clash.clashing) as f64 / (cap + clash.population) as f64;
    AxisScore {
        score: score.clamp(0.0, 1.0),
        findings,
    }
}

/// The most authored axioms the always-on axis probes for redundancy.
const REDUNDANCY_CAP: usize = 64;

/// The tally of the counter-example clash sub-check: how many of the slice's
/// **logical counter-examples** (fixtures co-typing a SHACL-declared-disjoint pair)
/// actually clash under the native DL reasoner, plus the advisories for those that
/// do not.
struct ClashTally {
    /// Counter-example fixtures that exercise a SHACL-declared class-disjointness.
    population: usize,
    /// Of those, the ones the reasoner proves inconsistent (module + fixture).
    clashing: usize,
    /// One advisory per non-clashing logical counter-example (the silent holes).
    findings: Vec<Finding>,
}

/// Verify the slice's counter-example fixtures against the DL reasoner: every
/// fixture that co-types an individual under a SHACL-declared-disjoint class pair
/// MUST clash (module + fixture reasons inconsistent). One that reasons consistent
/// is a silent hole in the negative space — reported, named, and scored below
/// perfect on the reasoner axis. Slice-local (only this slice's shapes, module, and
/// own counter-examples) and deterministic (sorted fixtures, sorted pairs).
fn counterexample_clash_check(ctx: &ScoreContext) -> ClashTally {
    let mut findings = Vec::new();

    // The class-disjointness the slice's SHACL shapes DECLARE — the Principle-17
    // projection of the canonical owl:disjointWith the reasoner must back.
    let shapes_path = ctx.slice_dir.join("shapes.ttl");
    let pairs = if shapes_path.is_file() {
        crate::dataset_from_paths(&[&shapes_path])
            .map(|ds| shacl_declared_disjoint_pairs(&ds))
            .unwrap_or_default()
    } else {
        BTreeSet::new()
    };
    let files = counterexample_fixture_files(ctx);
    if pairs.is_empty() || files.is_empty() {
        return ClashTally {
            population: 0,
            clashing: 0,
            findings,
        };
    }

    let module = ctx.slice_dir.join("module.ttl");
    let mut population = 0usize;
    let mut clashing = 0usize;
    for fixture_path in &files {
        let Ok(fixture_ds) = crate::dataset_from_paths(&[fixture_path]) else {
            continue;
        };
        let cotyped = cotyped_disjoint(&fixture_ds, &pairs);
        if cotyped.is_empty() {
            continue; // this counter-example does not exercise the logical negative space
        }
        population += 1;

        // Reason module + this ONE fixture: the co-typing must force a DL clash.
        let mut refs: Vec<&Path> = Vec::new();
        if module.is_file() {
            refs.push(module.as_path());
        }
        refs.push(fixture_path.as_path());
        let clashes = crate::dataset_from_paths(&refs)
            .ok()
            .and_then(|edb| dl_consistency(&edb).ok())
            .is_some_and(|v| !v.consistent || !v.unsatisfiable_classes.is_empty());

        if clashes {
            clashing += 1;
        } else {
            let (indiv, a, b) = &cotyped[0];
            let fname = fixture_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("<counter-example>");
            findings.push(advisory(
                "slice-quality.reasoner.counterexample-no-clash",
                format!(
                    "counter-example {fname} co-types <{indiv}> as both <{a}> and <{b}> — a class-disjointness the slice's SHACL shape declares — yet the native reasoner finds module + fixture CONSISTENT. The negative space is SHACL-only: author the backing owl:disjointWith so the canonical logic derives the contradiction the shape projects (Principle 17)."
                ),
            ));
        }
    }
    ClashTally {
        population,
        clashing,
        findings,
    }
}

/// The unordered `(A, B)` class-disjointness pairs the slice's SHACL shapes DECLARE:
/// a shape with `sh:targetClass A` carrying `sh:not [ sh:class B ]` asserts "an `A`
/// is never a `B`". Under Principle 17 the shape is a lossy projection of the
/// canonical `A owl:disjointWith B`, so the reasoner axis holds the slice to backing
/// it with the logical axiom. Only IRI classes are collected; the pair is sorted so
/// `(A,B)` and `(B,A)` coincide. Deterministic (a `BTreeSet`).
fn shacl_declared_disjoint_pairs(shapes: &RdfDataset) -> BTreeSet<(String, String)> {
    let (Some(target_p), Some(not_p), Some(class_p)) = (
        id(shapes, &format!("{SH_NS}targetClass")),
        id(shapes, &format!("{SH_NS}not")),
        id(shapes, &format!("{SH_NS}class")),
    ) else {
        return BTreeSet::new();
    };
    let mut out = BTreeSet::new();
    for q in shapes.quads_for_pattern(None, Some(not_p), None, GraphMatch::Any) {
        // The `sh:not` object is a nested shape; its `sh:class` names the excluded class.
        let excluded: Vec<String> = shapes
            .quads_for_pattern(Some(q.o), Some(class_p), None, GraphMatch::Any)
            .filter_map(|c| match shapes.resolve(c.o) {
                TermRef::Iri(iri) => Some(iri.to_owned()),
                _ => None,
            })
            .collect();
        if excluded.is_empty() {
            continue; // a sh:not over hasValue/pattern/etc. is a value exclusion, not disjointness
        }
        let targets: Vec<String> = shapes
            .quads_for_pattern(Some(q.s), Some(target_p), None, GraphMatch::Any)
            .filter_map(|t| match shapes.resolve(t.o) {
                TermRef::Iri(iri) => Some(iri.to_owned()),
                _ => None,
            })
            .collect();
        for t in &targets {
            for c in &excluded {
                if t != c {
                    let (a, b) = if t < c {
                        (t.clone(), c.clone())
                    } else {
                        (c.clone(), t.clone())
                    };
                    out.insert((a, b));
                }
            }
        }
    }
    out
}

/// The slice's OWN counter-example fixture files: the `gmeow:exampleFile` of every
/// `gmeow:ExampleConformance` cell whose `gmeow:expectedOutcome` is `gmeow:violates`
/// (the discovered counter-example convention — see `tests/example-conformance.ttl`).
/// Paths are resolved against the slice dir; sorted, deduped, existing files only.
fn counterexample_fixture_files(ctx: &ScoreContext) -> Vec<PathBuf> {
    let ds = ctx.graph;
    let g = |local: &str| format!("{GMEOW}{local}");
    let (Some(type_p), Some(ec), Some(outcome_p), Some(violates), Some(file_p)) = (
        id(ds, RDF_TYPE),
        id(ds, &g("ExampleConformance")),
        id(ds, &g("expectedOutcome")),
        id(ds, &g("violates")),
        id(ds, &g("exampleFile")),
    ) else {
        return Vec::new();
    };
    let mut rels = BTreeSet::new();
    for q in ds.quads_for_pattern(None, Some(type_p), Some(ec), GraphMatch::Any) {
        let expects_violation = ds
            .quads_for_pattern(Some(q.s), Some(outcome_p), Some(violates), GraphMatch::Any)
            .next()
            .is_some();
        if !expects_violation {
            continue;
        }
        for f in ds.quads_for_pattern(Some(q.s), Some(file_p), None, GraphMatch::Any) {
            if let TermRef::Literal { lexical, .. } = ds.resolve(f.o) {
                rels.insert(lexical.to_owned());
            }
        }
    }
    rels.into_iter()
        .map(|rel| ctx.slice_dir.join(rel))
        .filter(|p| p.is_file())
        .collect()
}

/// The `(individual, A, B)` co-typings in `fixture` where `(A,B)` is a
/// SHACL-declared-disjoint pair — the counter-examples that actually EXERCISE the
/// slice's logical negative space (the ones the reasoner must clash on). Only
/// asserted `rdf:type` edges are read, so the check is deterministic and does not
/// depend on the reasoner it is validating. Sorted, deduped.
fn cotyped_disjoint(
    fixture: &RdfDataset,
    pairs: &BTreeSet<(String, String)>,
) -> Vec<(String, String, String)> {
    let Some(type_p) = id(fixture, RDF_TYPE) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (a, b) in pairs {
        let (Some(a_id), Some(b_id)) = (id(fixture, a), id(fixture, b)) else {
            continue;
        };
        for qa in fixture.quads_for_pattern(None, Some(type_p), Some(a_id), GraphMatch::Any) {
            let also_b = fixture
                .quads_for_pattern(Some(qa.s), Some(type_p), Some(b_id), GraphMatch::Any)
                .next()
                .is_some();
            if also_b && let TermRef::Iri(indiv) = fixture.resolve(qa.s) {
                out.push((indiv.to_owned(), a.clone(), b.clone()));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Public proof helper: the named `subClassOf` triples the reasoner re-derives
/// without them (closure-redundant). Exposed for the acceptance fixture.
///
/// # Errors
/// Returns a message if reasoning fails on a reduced graph.
pub fn closure_redundant_subclasses(
    ds: &RdfDataset,
) -> gmeow_errors::Result<Vec<(String, String)>> {
    let subclasses = named_subclass_triples(ds);
    let probes = subclasses
        .iter()
        .map(|(subject, object)| LeaveOneOutAxiom::new(subject, SUBCLASS, object))
        .collect::<Vec<_>>();
    let rederived = leave_one_out_rederived(ds, &probes)?;
    Ok(subclasses
        .into_iter()
        .zip(rederived)
        .filter_map(|(axiom, redundant)| redundant.then_some(axiom))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(ttl: &str) -> Arc<RdfDataset> {
        let ds = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("parse ttl");
        let mut b = RdfDatasetBuilder::new();
        b.push_dataset(&ds);
        b.freeze().expect("freeze")
    }

    /// The number of quads that mention `owl:Restriction` as an object — the
    /// blank-node encoding that must survive leave-one-out.
    fn restriction_quad_count(ds: &RdfDataset) -> usize {
        let owl_restriction = "http://www.w3.org/2002/07/owl#Restriction";
        ds.owned_quads()
            .filter(|q| matches!(&q.object, RdfTerm::Iri(o) if o == owl_restriction))
            .count()
    }

    #[test]
    fn leave_one_out_preserves_blank_node_restrictions() {
        // A class whose subclass axiom is IRI-encoded AND an owl:Restriction that is
        // BLANK-node encoded. Dropping the one subclass triple must not disturb the
        // restriction quads: they are the DL structure the closure depends on.
        let ds = parse(
            r#"
            @prefix ex:   <https://example.org/> .
            @prefix owl:  <http://www.w3.org/2002/07/owl#> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

            ex:A a owl:Class ;
                rdfs:subClassOf ex:B ;
                rdfs:subClassOf [ a owl:Restriction ;
                                  owl:onProperty ex:p ;
                                  owl:someValuesFrom ex:C ] .
            ex:B a owl:Class .
            "#,
        );

        // Precondition: the source graph carries exactly one owl:Restriction blank.
        assert_eq!(
            restriction_quad_count(&ds),
            1,
            "fixture has one restriction"
        );

        let subclass = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
        let reduced = edb_without_triple(
            &ds,
            "https://example.org/A",
            subclass,
            "https://example.org/B",
        );

        // The blank-node restriction and its inner axioms survive the leave-one-out.
        assert_eq!(
            restriction_quad_count(&reduced),
            1,
            "the owl:Restriction blank node must survive leave-one-out (regression: blank/literal quads were silently dropped)"
        );
        let onproperty = reduced.owned_quads().any(|q| {
            q.predicate == "http://www.w3.org/2002/07/owl#onProperty"
                && matches!(&q.object, RdfTerm::Iri(o) if o == "https://example.org/p")
        });
        assert!(onproperty, "the restriction's owl:onProperty edge survives");

        // The single targeted (A subClassOf B) IRI triple is the ONLY thing removed.
        let a_subclass_b = reduced.owned_quads().any(|q| {
            q.predicate == subclass
                && matches!(&q.subject, RdfTerm::Iri(s) if s == "https://example.org/A")
                && matches!(&q.object, RdfTerm::Iri(o) if o == "https://example.org/B")
        });
        assert!(!a_subclass_b, "the targeted triple is removed");
    }

    #[test]
    fn parallel_redundancy_probes_match_serial_findings_and_order() {
        let ds = parse(
            r#"
            @prefix ex:   <https://example.org/> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

            ex:A rdfs:subClassOf ex:B, ex:C .
            ex:B rdfs:subClassOf ex:C .
            ex:C rdfs:subClassOf ex:D .
            "#,
        );
        let axioms = authored_axioms(&ds);
        let serial: Vec<Option<Finding>> = axioms
            .iter()
            .map(|(subject, predicate, object)| {
                let reduced = edb_without_triple(&ds, subject, predicate, object);
                let redundant =
                    gmeow_logic::reason::reason_closure_axioms(&reduced).is_ok_and(|closure| {
                        closure_contains_iri(&closure, subject, predicate, object)
                    });
                redundancy_finding(
                    &(subject.clone(), predicate.clone(), object.clone()),
                    redundant,
                )
            })
            .collect();
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(4)
            .build()
            .expect("four-worker pool");
        for _ in 0..8 {
            let parallel = pool
                .install(|| redundancy_probes(&ds, &axioms))
                .expect("incremental leave-one-out succeeds");
            let summary = |findings: &[Option<Finding>]| {
                findings
                    .iter()
                    .map(|finding| {
                        finding
                            .as_ref()
                            .map(|finding| (finding.code.clone(), finding.message.clone()))
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(summary(&parallel), summary(&serial));
        }
    }

    #[test]
    fn epistemics_batch_matches_scratch_for_the_real_capped_population() {
        fn collect_turtle(dir: &Path, paths: &mut Vec<PathBuf>) {
            for entry in std::fs::read_dir(dir).expect("slice directory reads") {
                let path = entry.expect("slice entry reads").path();
                if path.is_dir() {
                    collect_turtle(&path, paths);
                } else if path.extension().is_some_and(|extension| extension == "ttl") {
                    paths.push(path);
                }
            }
        }

        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("repo root canonicalizes");
        let slice = root.join("slices/core/epistemics");
        let mut paths = Vec::new();
        collect_turtle(&slice, &mut paths);
        paths.sort();
        let refs = paths.iter().map(PathBuf::as_path).collect::<Vec<_>>();
        let ds = crate::dataset_from_paths(&refs).expect("epistemics dataset parses");
        let axioms = authored_axioms(&ds);
        let capped = &axioms[..axioms.len().min(REDUNDANCY_CAP)];
        let batched = redundancy_probes(&ds, capped).expect("batch reasons");
        let scratch = capped
            .iter()
            .map(|(subject, predicate, object)| {
                let reduced = edb_without_triple(&ds, subject, predicate, object);
                let redundant =
                    gmeow_logic::reason::reason_closure_axioms(&reduced).is_ok_and(|closure| {
                        closure_contains_iri(&closure, subject, predicate, object)
                    });
                redundancy_finding(
                    &(subject.clone(), predicate.clone(), object.clone()),
                    redundant,
                )
            })
            .collect::<Vec<_>>();
        let flags =
            |findings: &[Option<Finding>]| findings.iter().map(Option::is_some).collect::<Vec<_>>();
        assert_eq!(flags(&batched), flags(&scratch));
    }

    #[test]
    #[ignore = "exhaustive repo audit; focused real/synthetic parity tests stay on-gate"]
    fn every_real_slice_batch_matches_parallel_scratch() {
        use rayon::prelude::*;

        fn collect_turtle(dir: &Path, paths: &mut Vec<PathBuf>) {
            for entry in std::fs::read_dir(dir).expect("slice directory reads") {
                let path = entry.expect("slice entry reads").path();
                if path.is_dir() {
                    collect_turtle(&path, paths);
                } else if path.extension().is_some_and(|extension| extension == "ttl") {
                    paths.push(path);
                }
            }
        }

        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("repo root canonicalizes");
        let mut slices = Vec::new();
        for group in std::fs::read_dir(root.join("slices")).expect("slice groups read") {
            let group = group.expect("group entry reads").path();
            if !group.is_dir() {
                continue;
            }
            for slice in std::fs::read_dir(group).expect("slice group reads") {
                let slice = slice.expect("slice entry reads").path();
                if slice.join("manifest.ttl").is_file() {
                    slices.push(slice);
                }
            }
        }
        slices.sort();

        for slice in slices {
            let mut paths = Vec::new();
            collect_turtle(&slice, &mut paths);
            paths.sort();
            let refs = paths.iter().map(PathBuf::as_path).collect::<Vec<_>>();
            let ds = crate::dataset_from_paths(&refs).expect("slice dataset parses");
            let axioms = authored_axioms(&ds);
            let capped = &axioms[..axioms.len().min(REDUNDANCY_CAP)];
            let batched = redundancy_probes(&ds, capped).expect("batch reasons");
            let scratch = capped
                .par_iter()
                .map(|(subject, predicate, object)| {
                    let reduced = edb_without_triple(&ds, subject, predicate, object);
                    gmeow_logic::reason::reason_closure_axioms(&reduced).is_ok_and(|closure| {
                        closure_contains_iri(&closure, subject, predicate, object)
                    })
                })
                .collect::<Vec<_>>();
            for (index, ((subject, predicate, object), batch)) in
                capped.iter().zip(batched.iter()).enumerate()
            {
                assert_eq!(
                    batch.is_some(),
                    scratch[index],
                    "{}: batch/scratch mismatch for <{subject}> <{predicate}> <{object}>",
                    slice.display()
                );
            }
        }
    }
}
