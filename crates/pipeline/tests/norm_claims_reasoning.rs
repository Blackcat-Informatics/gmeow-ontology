// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Reasoning-reuse acceptance test over the SHIPPED bundle's `graph/norm-claims` — honestly
//! EMPTY of advice content.
//!
//! Models `crates/logic/tests/ontology_entailments.rs`'s scoped-closure pattern: union a
//! small TBox with a small A-Box in one default-graph world and close it under the native
//! OWL 2 RL chase (`gmeow_logic::reason::rl_closure`). The A-Box here is the REAL
//! `graph/norm-claims` named graph read back from the SHIPPED `generated/dist/gmeow.gts`; the
//! TBox is `slices/extensions/norms/module.ttl`.
//!
//! Advice fires only on a DATA MATCH (see `norm_claims_bundle.rs`'s module docs), and the
//! shipped bundle's base graph is deliberately TBox-only, so `graph/norm-claims` carries no
//! advisory-harvested `gmeow:ComplianceAssessment` / `gmeow:Event` / `gmeow:Norm` triple here
//! — the reified advice wing is honestly EMPTY. This test asserts that honest absence
//! (rather than a specific harvested code no producer emits any more) and, when the A-Box is
//! non-empty for any other reason, proves the native OWL 2 RL reasoner still consumes it
//! without error — reasoning-consumability is a property of the TBox/reader, not of any one
//! demonstrator individual.
//!
//! The positive proof that a REAL advisory event's `ComplianceAssessment` is genuinely
//! reasoning-consumable content (entailing `gmeow:observedFeature` / `rdf:type
//! gmeow:Observation` authored nowhere in the emitted claim) lives in `advice_wing_fixture.rs`,
//! which supplies its own TEST-ONLY anti-pattern individual and drives the whole
//! compile → validate → split → project pipeline over it.
//!
//! Like `norm_claims_bundle.rs`, this test `.expect()`s the committed bundle — it runs
//! green only after `make sync`.

use std::path::{Path, PathBuf};

use gmeow_logic::reason::rl_closure;
use purrdf::gts::model::Graph;
use purrdf::{RdfDatasetBuilder, RdfQuad, parse_dataset};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const GRAPH_NORM_CLAIMS: &str = "https://blackcatinformatics.ca/gmeow/graph/norm-claims";

/// The `advice.` family code prefix (`crates/validate/src/codes.rs::ADVICE_FAMILY`) — the
/// string this test proves is ABSENT from any `gmeow:ComplianceAssessment` subject IRI in the
/// shipped bundle's `graph/norm-claims` A-Box.
const ADVICE_FAMILY: &str = "advice.";

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
/// into owned quads in one default-graph world — the emitted `gmeow:ComplianceAssessment`/
/// `gmeow:Event`/`gmeow:Norm` A-Box, read back through the native GTS reader exactly as
/// `norm_claims_bundle.rs` does. Returns an EMPTY vector, not an error, when the named graph
/// is entirely absent from the bundle (no such graph-name term interned) — an absent graph
/// and an empty graph are both honest "no norm-claims content" states.
fn norm_claims_abox_quads() -> Vec<RdfQuad> {
    let bytes =
        std::fs::read(repo_root().join("generated/dist/gmeow.gts")).expect("committed gmeow.gts");
    let g = purrdf::gts::read_graph(&bytes, true).expect("read_graph");

    let Some(graph_id) = g
        .terms
        .iter()
        .position(|t| t.value.as_deref() == Some(GRAPH_NORM_CLAIMS))
    else {
        return Vec::new();
    };

    let quads: Vec<_> = g
        .quads
        .iter()
        .filter(|&&(_, _, _, gname)| gname == Some(graph_id))
        .map(|&(s, p, o, _)| (s, p, o, None))
        .collect();
    if quads.is_empty() {
        return Vec::new();
    }

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

/// The honest invariant (G1 rework): the shipped `graph/norm-claims` A-Box carries no
/// advisory-harvested `gmeow:ComplianceAssessment` (no subject IRI embeds an `advice.`-family
/// code) — vacuously true when the graph is absent/empty, the expected state for the
/// TBox-only shipped bundle. When the A-Box DOES carry other content, the native OWL 2 RL
/// reasoner must still union + close it with the `norms` TBox without error, proving
/// reasoning-consumability is a property of the reader/TBox pairing, not of any one
/// demonstrator individual.
#[test]
fn shipped_norm_claims_abox_carries_no_advisory_assessment_and_reasons_cleanly() {
    let abox = norm_claims_abox_quads();

    let assessment_class = gmeow_iri("ComplianceAssessment");
    let advisory_assessments: Vec<&str> = abox
        .iter()
        .filter_map(|q| match (&q.subject, q.predicate.as_str(), &q.object) {
            (purrdf::RdfTerm::Iri(s), p, purrdf::RdfTerm::Iri(o))
                if p == RDF_TYPE && o == &assessment_class && s.contains(ADVICE_FAMILY) =>
            {
                Some(s.as_str())
            }
            _ => None,
        })
        .collect();
    assert!(
        advisory_assessments.is_empty(),
        "the TBox-only shipped bundle's graph/norm-claims A-Box must carry NO \
         gmeow:ComplianceAssessment whose IRI embeds an `{ADVICE_FAMILY}` code; found: \
         {advisory_assessments:?}"
    );

    if abox.is_empty() {
        // Absent or empty graph/norm-claims: the honest invariant holds vacuously, and there
        // is no content to union + close — nothing further to prove here.
        return;
    }

    let tbox = turtle_quads(&["slices/extensions/norms/module.ttl"]);

    let mut quads = tbox;
    quads.extend(abox);
    let dataset = dataset_from_quads(quads);
    let closure = rl_closure(dataset.as_ref()).expect(
        "the native OWL 2 RL reasoner must close whatever graph/norm-claims content the \
         shipped bundle carries, together with the norms TBox, without error",
    );
    assert!(
        !closure.triples.is_empty(),
        "a non-empty A-Box union non-empty TBox must close to at least the asserted triples"
    );
}

/// `https://blackcatinformatics.ca/gmeow/<local>`.
fn gmeow_iri(local: &str) -> String {
    format!("{GMEOW}{local}")
}
