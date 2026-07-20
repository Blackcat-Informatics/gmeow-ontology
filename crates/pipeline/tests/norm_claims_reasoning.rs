// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Reasoning-reuse acceptance test over the SHIPPED bundle
//! demonstration.
//!
//! Models `crates/logic/tests/ontology_entailments.rs`'s scoped-closure pattern: union a
//! small TBox with a small A-Box in one default-graph world and close it under the native
//! OWL 2 RL chase (`gmeow_logic::reason::rl_closure`), then assert a concrete triple is
//! ABSENT before closure and PRESENT after — the "authored nowhere, entailed" contrast
//! that proves the content is reasoning-consumable, not merely CLI-rendered prose.
//!
//! Unlike `ontology_entailments.rs` (a synthetic A-Box), the A-Box here is the REAL
//! `graph/norm-claims` named graph read back from the SHIPPED `generated/dist/gmeow.gts`
//! — the emitted `gmeow:ComplianceAssessment` / `gmeow:Event` claim for the demonstrator
//! advisory code. The TBox is `slices/extensions/norms/module.ttl`, which carries both
//! axioms this test exercises:
//!   * `gmeow:ComplianceAssessment rdfs:subClassOf gmeow:Observation` (line ~41)
//!   * `gmeow:assessedEvent rdfs:subPropertyOf gmeow:observedFeature` (line ~416)
//!
//! Two independent entailments are proven, both authored nowhere in the emitted claim:
//!   1. `(assessment, gmeow:observedFeature, event)` — RL `prp-spo1` over the
//!      `assessedEvent ⊑ observedFeature` sub-property axiom.
//!   2. `(assessment, rdf:type, gmeow:Observation)` — RL `cax-sco` over the
//!      `ComplianceAssessment ⊑ Observation` sub-class axiom.
//!
//! `crates/pipeline/Cargo.toml` depends on `gmeow-logic` directly (the compile-logic /
//! reason-native producers), so `gmeow_logic::reason::rl_closure` is reachable from this
//! integration test without moving it into `crates/logic/tests/`.
//!
//! Like `norm_claims_bundle.rs`, this test `.expect()`s the committed bundle — it runs
//! green only after `make sync`.

use std::path::{Path, PathBuf};

use gmeow_logic::reason::{RlClosure, rl_closure};
use purrdf::gts::model::Graph;
use purrdf::{RdfDatasetBuilder, RdfQuad, parse_dataset};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const GRAPH_NORM_CLAIMS: &str = "https://blackcatinformatics.ca/gmeow/graph/norm-claims";

/// A harvested advisory rule's code both advice wings project — `advice.` family
/// prefix + the `logic:candAdviceAvoidBareEntity` candidate local name (harvesting
/// gmeow:Entity's `avoidWhen`) — embedded in the `graph/norm-claims` claim's
/// content-addressed IRIs (`NORM_CLAIMS_BASE_IRI`).
const ADVICE_CODE: &str = "advice.candAdviceAvoidBareEntity";

/// `https://blackcatinformatics.ca/gmeow/<local>`.
fn gmeow(local: &str) -> String {
    format!("{GMEOW}{local}")
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

/// Parse Turtle TBox sources (repo-relative paths) into owned quads in one default-graph
/// world — the `turtle_quads` idiom of `crates/logic/tests/ontology_entailments.rs`.
fn turtle_quads(rel_paths: &[&str]) -> Vec<RdfQuad> {
    let root = repo_root();
    let mut quads = Vec::new();
    for rel in rel_paths {
        let path = root.join(rel);
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("missing ontology source {}: {e}", path.display()));
        let dataset = parse_dataset(&bytes, "text/turtle", None)
            .unwrap_or_else(|e| panic!("Turtle parse failed for {}: {e}", path.display()));
        quads.extend(dataset.owned_quads());
    }
    quads
}

/// The `graph/norm-claims` named graph of the committed `generated/dist/gmeow.gts`, folded
/// into owned quads in one default-graph world (the "union graph/norm-claims into the
/// reasoning EDB" step) — the emitted `gmeow:ComplianceAssessment`/`gmeow:Event`/`gmeow:Norm`
/// A-Box, read back through the native GTS reader exactly as `norm_claims_bundle.rs` does.
fn norm_claims_abox_quads() -> Vec<RdfQuad> {
    let bytes =
        std::fs::read(repo_root().join("generated/dist/gmeow.gts")).expect("committed gmeow.gts");
    let g = purrdf::gts::read_graph(&bytes, true).expect("read_graph");

    let graph_id = g
        .terms
        .iter()
        .position(|t| t.value.as_deref() == Some(GRAPH_NORM_CLAIMS))
        .expect("graph/norm-claims graph-name term must be interned in the shipped bundle");

    let quads: Vec<_> = g
        .quads
        .iter()
        .filter(|&&(_, _, _, gname)| gname == Some(graph_id))
        .map(|&(s, p, o, _)| (s, p, o, None))
        .collect();
    assert!(
        !quads.is_empty(),
        "graph/norm-claims must carry a non-empty triple set in the shipped bundle"
    );

    let filtered = Graph {
        terms: g.terms,
        quads,
        ..Graph::default()
    };
    let dataset = purrdf::gts::dataset_from_gts_graph(&filtered)
        .expect("build an RdfDataset from the graph/norm-claims quads");
    dataset.owned_quads().collect()
}

fn dataset_from_quads(quads: Vec<RdfQuad>) -> std::sync::Arc<purrdf::RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for quad in quads {
        builder.push_owned_quad(&quad);
    }
    builder.freeze().expect("valid TBox+ABox dataset")
}

/// Strip an optional surrounding `<…>` so closure terms compare against bare IRIs
/// regardless of how `RlTriple` renders each position — the `unwrap_iri` idiom of
/// `crates/logic/tests/ontology_entailments.rs`.
fn unwrap_iri(term: &str) -> &str {
    term.strip_prefix('<')
        .and_then(|t| t.strip_suffix('>'))
        .unwrap_or(term)
}

/// `true` iff the closure contains the IRI triple `s p o`.
fn contains(closure: &RlClosure, s: &str, p: &str, o: &str) -> bool {
    closure.triples.iter().any(|t| {
        unwrap_iri(&t.subject) == s && unwrap_iri(&t.predicate) == p && unwrap_iri(&t.object) == o
    })
}

/// `true` iff `s p o` (as IRIs) is one of the asserted quads.
fn asserted(quads: &[RdfQuad], s: &str, p: &str, o: &str) -> bool {
    quads.iter().any(|q| {
        matches!(&q.subject, purrdf::RdfTerm::Iri(qs) if qs == s)
            && q.predicate == p
            && matches!(&q.object, purrdf::RdfTerm::Iri(qo) if qo == o)
    })
}

/// The reasoning-reuse demonstration (Completion-Adversary F2): union the real shipped
/// `graph/norm-claims` A-Box with the `norms` TBox, close under native OWL 2 RL, and prove
/// two entailments that are authored nowhere in the emitted claim — the demonstrator advice
/// event's `ComplianceAssessment` is genuinely reasoning-consumable content, not a CLI line.
#[test]
fn shipped_norm_claims_abox_is_consumed_by_the_native_rl_reasoner() {
    let abox = norm_claims_abox_quads();
    let tbox = turtle_quads(&["slices/extensions/norms/module.ttl"]);

    let assessment_iri =
        format!("https://blackcatinformatics.ca/gmeow/norm-claims/{ADVICE_CODE}/assessment");
    let event_iri = format!("https://blackcatinformatics.ca/gmeow/norm-claims/{ADVICE_CODE}/event");
    let observed_feature = gmeow("observedFeature");
    let observation_class = gmeow("Observation");

    // The A-Box asserts assessedEvent, never observedFeature or rdf:type Observation.
    assert!(
        asserted(&abox, &assessment_iri, &gmeow("assessedEvent"), &event_iri),
        "the shipped graph/norm-claims A-Box must assert {assessment_iri} gmeow:assessedEvent \
         {event_iri} — the property-chain premise this test reasons over"
    );
    assert!(
        !asserted(&abox, &assessment_iri, &observed_feature, &event_iri),
        "{assessment_iri} gmeow:observedFeature {event_iri} must be ABSENT from the raw A-Box \
         (it must be entailed, not authored, or this is not a reasoning-reuse proof)"
    );
    assert!(
        !asserted(&abox, &assessment_iri, RDF_TYPE, &observation_class),
        "{assessment_iri} rdf:type gmeow:Observation must be ABSENT from the raw A-Box (it must \
         be entailed, not authored, or this is not a reasoning-reuse proof)"
    );

    let mut quads = tbox;
    quads.extend(abox);
    let dataset = dataset_from_quads(quads);
    let closure = rl_closure(dataset.as_ref()).expect("scoped OWL 2 RL closure should succeed");

    // Entailment 1: assessedEvent ⊑ observedFeature (prp-spo1) — the reified assessment is
    // discoverable through the generic Observation query surface (observedFeature) despite
    // never asserting that property directly.
    assert!(
        contains(&closure, &assessment_iri, &observed_feature, &event_iri),
        "{assessment_iri} gmeow:observedFeature {event_iri} must be ENTAILED by the native OWL \
         2 RL closure via gmeow:assessedEvent rdfs:subPropertyOf gmeow:observedFeature \
         (slices/extensions/norms/module.ttl)"
    );

    // Entailment 2: ComplianceAssessment ⊑ Observation (cax-sco) — the assessment is
    // classified into the universal claim construct, authored nowhere in the emitted claim.
    assert!(
        contains(&closure, &assessment_iri, RDF_TYPE, &observation_class),
        "{assessment_iri} rdf:type gmeow:Observation must be ENTAILED by the native OWL 2 RL \
         closure via gmeow:ComplianceAssessment rdfs:subClassOf gmeow:Observation \
         (slices/extensions/norms/module.ttl)"
    );

    // Non-vacuity floor: the closure over the real shipped A-Box is non-trivial.
    assert!(
        closure.triples.len() > abox_triple_count_floor(),
        "the scoped RL closure over the shipped graph/norm-claims A-Box should be non-trivial; \
         got {} triples",
        closure.triples.len()
    );
}

/// A conservative non-vacuity floor: fewer triples than this would indicate the TBox or
/// A-Box silently failed to parse/union rather than genuinely closing.
fn abox_triple_count_floor() -> usize {
    10
}
