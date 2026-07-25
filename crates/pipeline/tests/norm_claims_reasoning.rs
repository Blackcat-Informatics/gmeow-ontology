// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Reasoning-reuse acceptance test over the SHIPPED bundle's `graph/norm-claims`.
//!
//! Models `crates/logic/tests/ontology_entailments.rs`'s scoped-closure pattern: union a
//! small TBox with a small A-Box in one default-graph world and close it under the native
//! OWL 2 RL chase (`gmeow_logic::reason::rl_closure`). The A-Box here is the REAL
//! `graph/norm-claims` named graph read back from the SHIPPED `generated/dist/gmeow.gts`; the
//! TBox is `slices/core/norms/module.ttl`.
//!
//! Advice fires from a DATA MATCH (see `norm_claims_bundle.rs`'s module docs), and the shipped
//! bundle's base graph folds bare `gmeow:Entity` A-Box individuals that match the advisory guard,
//! so `graph/norm-claims` DOES carry advisory-harvested `gmeow:ComplianceAssessment` content. This
//! test asserts the wing SHIPS (at least one `advice.`-family `ComplianceAssessment`, keyed on the
//! family rather than a per-focus-digest code) AND that the native OWL 2 RL reasoner unions + closes
//! that real A-Box with the `norms` TBox without error — the emitted claim is genuinely
//! reasoning-consumable content, not inert bytes.
//!
//! The isolated, deterministic proof over a controlled fixture lives in `advice_wing_fixture.rs`.
//!
//! Like `norm_claims_bundle.rs`, this test `.expect()`s the committed bundle — it runs green only
//! after `make sync`.

use std::path::{Path, PathBuf};

use gmeow_logic::reason::rl_closure;
use purrdf::gts::model::Graph;
use purrdf::{RdfDatasetBuilder, RdfQuad, parse_dataset};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const GRAPH_NORM_CLAIMS: &str = "https://blackcatinformatics.ca/gmeow/graph/norm-claims";

/// The `advice.` family code prefix (`crates/validate/src/codes.rs::ADVICE_FAMILY`) — the family
/// this test proves SHIPS (and reasons cleanly) in the bundle's `graph/norm-claims` A-Box.
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

/// The invariant: the shipped `graph/norm-claims` A-Box carries at least one advisory-harvested
/// `gmeow:ComplianceAssessment` (an `advice.`-family code in its subject IRI — the wing ships), and
/// the native OWL 2 RL reasoner unions + closes that real A-Box with the `norms` TBox without error
/// — the emitted claim is genuinely reasoning-consumable content.
#[test]
fn shipped_norm_claims_abox_carries_the_advisory_assessment_and_reasons_cleanly() {
    let abox = norm_claims_abox_quads();
    assert!(
        !abox.is_empty(),
        "the shipped bundle's graph/norm-claims A-Box must be non-empty — the base graph folds \
         bare gmeow:Entity individuals that match the advisory guard, so the advice wing ships"
    );

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
        !advisory_assessments.is_empty(),
        "the shipped bundle's graph/norm-claims A-Box must carry at least one \
         gmeow:ComplianceAssessment whose IRI embeds an `{ADVICE_FAMILY}` code (the wing ships)"
    );

    let tbox = turtle_quads(&["slices/core/norms/module.ttl"]);

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
