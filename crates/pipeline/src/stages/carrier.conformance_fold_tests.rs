// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// Read every named-graph IRI present in a folded snapshot's quad table.
fn folded_graph_names(gts: &[u8]) -> std::collections::BTreeSet<String> {
    let g = purrdf::gts::read_graph(gts, true).expect("read_graph");
    let mut names = std::collections::BTreeSet::new();
    for &(_, _, _, gname) in &g.quads {
        if let Some(gid) = gname
            && let Some(value) = g.terms.get(gid).and_then(|t| t.value.clone())
        {
            names.insert(value);
        }
    }
    names
}

/// A synthetic divergence Finding folds into the `graph/conformance` named
/// graph of the emitted snapshot — the C3 fold contract. Constructed
/// independently of the (currently all-agree) committed corpus so the assertion
/// holds regardless of whether a real divergence exists today.
#[test]
fn synthetic_divergence_lands_in_graph_conformance() {
    // One CorpusOnly + one DlGap divergence, projected to gmeow:Finding N-Quads
    // in the conformance graph by the shared emitter.
    let conformance = gmeow_conformance::divergence::emit_divergence_nq(
        "w3c-owl2-el",
        &[
            gmeow_logic::reason::ExternalComparison {
                case: "clash".to_owned(),
                world: "https://gmeow.example/w3c-owl2-el/clash/w".to_owned(),
                native: "consistent".to_owned(),
                published: "inconsistent".to_owned(),
            },
            gmeow_logic::reason::ExternalComparison {
                case: "beyond-el".to_owned(),
                world: "https://gmeow.example/w3c-owl2-el/beyond-el/w".to_owned(),
                native: "incomplete".to_owned(),
                published: "consistent".to_owned(),
            },
        ],
    );
    assert!(
        !conformance.is_empty(),
        "the synthetic divergence must emit Findings"
    );

    // Fold it through the SAME add_named path the snapshot serialization uses, emit,
    // and read the bundle back.
    let mut builder = SnapshotBuilder::new();
    // A non-empty default graph so the bundle is well-formed.
    add_base_nq(
        &mut builder,
        b"<https://blackcatinformatics.ca/gmeow/> \
              <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
              <http://www.w3.org/2002/07/owl#Ontology> .\n",
        "base",
    )
    .expect("fold base graph");
    add_named(
        &mut builder,
        conformance.as_bytes(),
        GRAPH_CONFORMANCE,
        "conformance",
    )
    .expect("fold conformance graph");

    // gmeow-test-input: synthetic-only
    let gts = emit_gts(
        &builder,
        "dist",
        Some(vec!["gzip".to_string()]),
        Vec::new(),
        Vec::new(),
        None,
        None,
        None,
        purrdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
        &purrdf::gts_compose::MediumPlan::dist_default(Some(&["gzip".to_string()])),
    )
    .expect("emit snapshot");

    let names = folded_graph_names(&gts);
    assert!(
        names.contains(GRAPH_CONFORMANCE),
        "the folded snapshot must carry the graph/conformance named graph; got {names:?}"
    );
}

/// A reified `gmeow:CapabilityGap` individual (the G3 ontology image of a committed
/// divergence case's structured `gmeow:gapShape`) lands in the `graph/conformance`
/// named graph after the fold, mirroring
/// [`synthetic_divergence_lands_in_graph_conformance`] but for the capability-gap
/// emitter rather than the divergence-comparison one.
#[test]
fn capability_gap_lands_in_graph_conformance() {
    let (_, block) = gmeow_conformance::divergence::emit_capability_gap_nq(
        "entailment-mini-divergence",
        "multi-triple-conclusion",
        gmeow_logic::entail::CapabilityGapShape::VendoringMultiGoal,
    );
    assert!(!block.is_empty(), "the capability gap emitter must emit");

    let mut builder = SnapshotBuilder::new();
    add_base_nq(
        &mut builder,
        b"<https://blackcatinformatics.ca/gmeow/> \
              <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
              <http://www.w3.org/2002/07/owl#Ontology> .\n",
        "base",
    )
    .expect("fold base graph");
    add_named(
        &mut builder,
        block.as_bytes(),
        GRAPH_CONFORMANCE,
        "conformance",
    )
    .expect("fold conformance graph");

    // gmeow-test-input: synthetic-only
    let gts = emit_gts(
        &builder,
        "dist",
        Some(vec!["gzip".to_string()]),
        Vec::new(),
        Vec::new(),
        None,
        None,
        None,
        purrdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
        &purrdf::gts_compose::MediumPlan::dist_default(Some(&["gzip".to_string()])),
    )
    .expect("emit snapshot");

    let names = folded_graph_names(&gts);
    assert!(
        names.contains(GRAPH_CONFORMANCE),
        "the folded snapshot must carry the graph/conformance named graph; got {names:?}"
    );

    let g = purrdf::gts::read_graph(&gts, true).expect("read_graph");
    let capability_gap_type = "https://blackcatinformatics.ca/gmeow/CapabilityGap";
    let has_capability_gap = g.quads.iter().any(|&(_, p, o, _)| {
        let p_val = g.terms.get(p).and_then(|t| t.value.as_deref());
        let o_val = g.terms.get(o).and_then(|t| t.value.as_deref());
        p_val == Some("http://www.w3.org/1999/02/22-rdf-syntax-ns#type")
            && o_val == Some(capability_gap_type)
    });
    assert!(
        has_capability_gap,
        "the folded graph/conformance graph must carry a gmeow:CapabilityGap individual"
    );
}

/// An empty divergence (the all-agree corpus) is skipped — folding empty bytes
/// must NOT add a phantom `graph/conformance` slot.
#[test]
fn empty_divergence_adds_no_conformance_graph() {
    let mut builder = SnapshotBuilder::new();
    add_base_nq(
        &mut builder,
        b"<https://blackcatinformatics.ca/gmeow/> \
              <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
              <http://www.w3.org/2002/07/owl#Ontology> .\n",
        "base",
    )
    .expect("fold base graph");
    // Mirror the snapshot serialization guard: an empty graph is never add_named'd.
    let conformance: Vec<u8> = Vec::new();
    if !conformance.is_empty() {
        add_named(&mut builder, &conformance, GRAPH_CONFORMANCE, "conformance").expect("fold");
    }

    // gmeow-test-input: synthetic-only
    let gts = emit_gts(
        &builder,
        "dist",
        Some(vec!["gzip".to_string()]),
        Vec::new(),
        Vec::new(),
        None,
        None,
        None,
        purrdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
        &purrdf::gts_compose::MediumPlan::dist_default(Some(&["gzip".to_string()])),
    )
    .expect("emit snapshot");

    assert!(
        !folded_graph_names(&gts).contains(GRAPH_CONFORMANCE),
        "an all-agree corpus must not fold a phantom graph/conformance"
    );
}
