// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::ScoringEnv;
use crate::score::ReasonerProbeCounts;
use std::collections::BTreeMap;

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

/// G9 canonical-subsumption sweep: `INFERENTIAL_PREDS` — the population whose
/// load-bearingness the reasoner axis measures — must count a TBox axiom
/// authored with the canonical `logic:subClassOf`/`logic:subPropertyOf` spelling,
/// not only the `rdfs:` projection (crates/ns/src/lib.rs:106-166), or a
/// re-authored axiom silently leaves the axis's own denominator.
#[test]
fn authored_axioms_counts_canonical_logic_subsumption_edges() {
    let ds = parse(
        r#"
            @prefix ex:    <https://example.org/> .
            @prefix logic: <https://blackcatinformatics.ca/logic/> .

            ex:A logic:subClassOf ex:B .
            ex:p logic:subPropertyOf ex:q .
            "#,
    );
    let axioms = authored_axioms(&ds);
    assert!(
        axioms.contains(&(
            "https://example.org/A".to_owned(),
            gmeow_ns::LOGIC_SUB_CLASS_OF.to_owned(),
            "https://example.org/B".to_owned(),
        )),
        "authored_axioms must count the canonical logic:subClassOf edge: {axioms:?}"
    );
    assert!(
        axioms.contains(&(
            "https://example.org/p".to_owned(),
            gmeow_ns::LOGIC_SUB_PROPERTY_OF.to_owned(),
            "https://example.org/q".to_owned(),
        )),
        "authored_axioms must count the canonical logic:subPropertyOf edge: {axioms:?}"
    );
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
            let redundant = prepare_reasoning_input(&reduced)
                .and_then(|input| {
                    gmeow_logic::reason::reason_closure_axioms(input, &slice_theory_domains()?)
                })
                .is_ok_and(|closure| closure_contains_iri(&closure, subject, predicate, object));
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
        let files = BTreeMap::new();
        let ctx = ScoreContext::new(
            "https://example.org/slice".to_owned(),
            &files,
            &ds,
            ScoringEnv::Repo {
                slice_dir: std::path::PathBuf::from("/tmp/example-slice"),
            },
        );
        let parallel = pool
            .install(|| redundancy_probes(&ctx, &ds, &axioms))
            .expect("incremental leave-one-out succeeds");
        assert_eq!(
            ctx.reasoner_probe_counts(),
            ReasonerProbeCounts {
                certified: axioms.len(),
                native: 0,
            }
        );
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
