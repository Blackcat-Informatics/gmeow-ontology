// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Synthetic contracts for GMEOW carrier placement and compile graph selection.

use std::collections::BTreeMap;
use std::sync::Arc;

use purrdf::{
    BlankScope, ContentStore, DatasetProvenance, PipelineBundle, RdfDataset, RdfDatasetBuilder,
    RdfLocation, RdfLookaside, RdfQuad, RdfTerm,
};

use super::{
    CarrierParts, GRAPH_AUTHORED_DEFAULT, GRAPH_DIAGNOSTICS, compile_logic_object_graphs,
    rooted_in_graph,
};
use crate::node::StageProduct;
use crate::stages::compile_logic::{
    CARRIER_GRAPHS, GRAPH_CORRESPONDENCE, GRAPH_LOGIC, GRAPH_RELATIONAL_CORE,
};

const RELATION: &str = "urn:carrier:relation";
const STANDPOINT: &str = "https://blackcatinformatics.ca/logic/standpoint";

fn assert_graph_owner(dataset: &RdfDataset, graph: &str) {
    let expected = RdfTerm::iri(graph);
    assert_eq!(
        dataset.owned_named_graphs().collect::<Vec<_>>().as_slice(),
        std::slice::from_ref(&expected)
    );
    assert!(
        dataset
            .owned_quads()
            .all(|row| row.graph_name.as_ref() == Some(&expected))
    );
    assert!(
        dataset
            .owned_reifiers()
            .all(|row| row.graph.as_ref() == Some(&expected))
    );
    assert!(
        dataset
            .owned_annotations()
            .all(|row| row.graph.as_ref() == Some(&expected))
    );
}

/// The same claim resource has separate logic and diagnostic ownership. The
/// selected relational-core graph exists without rows; correspondence has only
/// statement-layer rows and is carried only by the complete shipping selection.
fn compile_source(logic_standpoint: &str, diagnostic_standpoint: &str) -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    let subject = builder.intern_iri("urn:carrier:subject").unwrap();
    let relation = builder.intern_iri(RELATION).unwrap();
    let object = builder.intern_iri("urn:carrier:object").unwrap();
    let claim = builder.intern_iri("urn:carrier:claim").unwrap();
    let statement = builder.intern_triple(subject, relation, object).unwrap();
    let standpoint = builder.intern_iri(STANDPOINT).unwrap();
    for (graph, context) in [
        (GRAPH_LOGIC, logic_standpoint),
        (GRAPH_DIAGNOSTICS, diagnostic_standpoint),
    ] {
        let graph = builder.intern_iri(graph).unwrap();
        let context = builder.intern_iri(context).unwrap();
        builder
            .push_quad(subject, relation, object, Some(graph))
            .unwrap();
        builder
            .push_reifier_in_graph(claim, statement, Some(graph))
            .unwrap();
        builder
            .push_annotation_in_graph(claim, standpoint, context, Some(graph))
            .unwrap();
    }
    let relational = builder.intern_iri(GRAPH_RELATIONAL_CORE).unwrap();
    builder.declare_named_graph(relational).unwrap();
    let correspondence = builder.intern_iri(GRAPH_CORRESPONDENCE).unwrap();
    let correspondence_context = builder.intern_iri("urn:standpoint:correspondence").unwrap();
    builder
        .push_reifier_in_graph(claim, statement, Some(correspondence))
        .unwrap();
    builder
        .push_annotation_in_graph(
            claim,
            standpoint,
            correspondence_context,
            Some(correspondence),
        )
        .unwrap();
    builder.freeze().expect("freeze synthetic compile carrier")
}

/// The snapshot's own selection of every shipped compile-logic graph, each composed and
/// materialized alone so a test can inspect one graph's complete surface.
fn compile_logic_carrier_graphs(
    upstream: &BTreeMap<String, StageProduct>,
) -> gmeow_errors::Result<Vec<Arc<RdfDataset>>> {
    CARRIER_GRAPHS
        .iter()
        .map(|graph| {
            let mut parts = CarrierParts::new(upstream);
            parts.push_graph("stage-compile-logic", graph)?;
            parts.materialize()
        })
        .collect()
}

fn compile_inputs(source: Arc<RdfDataset>) -> BTreeMap<String, StageProduct> {
    BTreeMap::from([(
        "stage-compile-logic".to_owned(),
        StageProduct::from_artifacts_over("stage-compile-logic", source, BTreeMap::new())
            .expect("the stage carrier digests"),
    )])
}

fn commitment_bundle(dataset: Arc<RdfDataset>) -> PipelineBundle<()> {
    PipelineBundle::new(
        dataset,
        RdfLookaside::default(),
        Arc::new(ContentStore::new()),
        DatasetProvenance::new(),
    )
}

#[test]
fn rooting_moves_statement_metadata_without_changing_within_input_identity() {
    let mut builder = RdfDatasetBuilder::new();
    let subject = builder.intern_blank("member", BlankScope(7)).unwrap();
    let object = builder.intern_blank("member", BlankScope(9)).unwrap();
    let relation = builder.intern_iri(RELATION).unwrap();
    let claim = builder.intern_blank("claim", BlankScope(7)).unwrap();
    let statement = builder.intern_triple(subject, relation, object).unwrap();
    let old_graph = builder.intern_iri("urn:carrier:previous-owner").unwrap();
    let standpoint = builder.intern_iri(STANDPOINT).unwrap();
    let provenance = builder
        .intern_iri("http://www.w3.org/ns/prov#wasDerivedFrom")
        .unwrap();
    let location = RdfLocation::logical("synthetic compile projection");
    let row = builder
        .push_quad_with_handle(subject, relation, object, None)
        .unwrap();
    builder.attach_location(row, location.clone());
    builder
        .push_reifier_in_graph(claim, statement, Some(old_graph))
        .unwrap();
    builder
        .push_annotation_in_graph(claim, standpoint, subject, None)
        .unwrap();
    builder
        .push_annotation_in_graph(claim, provenance, old_graph, Some(old_graph))
        .unwrap();
    let source = builder.freeze().expect("freeze selected RDF input");
    let original = source.owned_quads().next().expect("ordinary source row");
    assert_ne!(
        original.subject, original.object,
        "source scopes are distinct"
    );

    let rooted = rooted_in_graph(&source, GRAPH_LOGIC).expect("root complete logic carrier");
    assert_graph_owner(&rooted, GRAPH_LOGIC);
    let ordinary: Vec<_> = rooted.owned_quads().collect();
    assert_eq!(
        ordinary,
        [
            RdfQuad::new(original.subject.clone(), RELATION, original.object.clone())
                .in_graph(RdfTerm::iri(GRAPH_LOGIC))
                .with_location(location)
        ]
    );
    let reifiers: Vec<_> = rooted.owned_reifiers().collect();
    let annotations: Vec<_> = rooted.owned_annotations().collect();
    assert_eq!(reifiers.len(), 1);
    assert_eq!(annotations.len(), 2);
    assert_eq!(reifiers[0].statement.subject, original.subject);
    assert_eq!(reifiers[0].statement.predicate, RELATION);
    assert_eq!(reifiers[0].statement.object, original.object);
    for annotation in &annotations {
        assert_eq!(annotation.reifier, reifiers[0].reifier);
    }
    let context = annotations
        .iter()
        .find(|row| row.predicate == STANDPOINT)
        .unwrap();
    assert_eq!(context.object, original.subject);
    let origin = annotations
        .iter()
        .find(|row| row.predicate == "http://www.w3.org/ns/prov#wasDerivedFrom")
        .unwrap();
    assert_eq!(origin.object, RdfTerm::iri("urn:carrier:previous-owner"));
}

#[test]
fn compile_project_reinsert_keeps_all_selected_graph_layers_and_world_boundaries() {
    let source = compile_source("urn:standpoint:logic", "urn:standpoint:diagnostic");
    let upstream = compile_inputs(Arc::clone(&source));
    let shipped = compile_logic_carrier_graphs(&upstream).expect("select shipping carrier");
    assert_eq!(shipped.len(), CARRIER_GRAPHS.len());
    for (graph, dataset) in CARRIER_GRAPHS.iter().zip(&shipped) {
        assert_graph_owner(dataset, graph);
        let expected = source
            .project_graph(Some(&purrdf::RdfTerm::iri(*graph)))
            .unwrap();
        let recovered = dataset
            .project_graph(Some(&purrdf::RdfTerm::iri(*graph)))
            .unwrap();
        assert_eq!(
            recovered.owned_quads().collect::<Vec<_>>(),
            expected.owned_quads().collect::<Vec<_>>()
        );
        assert_eq!(
            recovered.owned_reifiers().collect::<Vec<_>>(),
            expected.owned_reifiers().collect::<Vec<_>>()
        );
        assert_eq!(
            recovered.owned_annotations().collect::<Vec<_>>(),
            expected.owned_annotations().collect::<Vec<_>>()
        );
    }
    let logic = &shipped[0];
    assert_eq!(logic.owned_reifiers().count(), 1);
    assert_eq!(logic.owned_annotations().count(), 1);
    assert_eq!(
        logic
            .owned_annotations()
            .next()
            .expect("logic context")
            .object,
        RdfTerm::iri("urn:standpoint:logic")
    );
    let correspondence = &shipped[2];
    assert_eq!(correspondence.quad_count(), 0);
    assert_eq!(correspondence.owned_reifiers().count(), 1);
    assert_eq!(correspondence.owned_annotations().count(), 1);
    let reasoning = compile_logic_object_graphs(&upstream).expect("select reasoning carrier");
    assert_eq!(reasoning.len(), 2);
    for (dataset, graph) in reasoning.iter().zip([GRAPH_LOGIC, GRAPH_RELATIONAL_CORE]) {
        assert_graph_owner(dataset, graph);
    }
}

#[test]
fn logic_handle_commitment_tracks_reinserted_metadata_and_excludes_diagnostics() {
    let source = compile_source("urn:standpoint:logic", "urn:standpoint:diagnostic");
    let expected = commitment_bundle(Arc::clone(&source))
        .graph_digest(
            GRAPH_LOGIC,
            &mut purrdf::CanonicalizationControl::new(purrdf::RDFC_V2_CORPUS_POLICY, None),
        )
        .unwrap();
    let mut selected = compile_logic_carrier_graphs(&compile_inputs(source)).unwrap();
    let mut bundle = commitment_bundle(selected.remove(0));
    bundle
        .pin_handle(
            GRAPH_LOGIC,
            (),
            expected,
            &mut purrdf::CanonicalizationControl::new(purrdf::RDFC_V2_CORPUS_POLICY, None),
        )
        .expect("reinsertion preserves the logic pin");
    assert_eq!(bundle.handle(GRAPH_LOGIC).unwrap().content_digest, expected);

    let mut changed_metadata = compile_logic_carrier_graphs(&compile_inputs(compile_source(
        "urn:standpoint:changed-logic",
        "urn:standpoint:diagnostic",
    )))
    .unwrap();
    let mut changed = commitment_bundle(changed_metadata.remove(0));
    assert_ne!(
        changed
            .graph_digest(
                GRAPH_LOGIC,
                &mut purrdf::CanonicalizationControl::new(purrdf::RDFC_V2_CORPUS_POLICY, None)
            )
            .unwrap(),
        expected
    );
    assert!(matches!(
        changed.pin_handle(
            GRAPH_LOGIC,
            (),
            expected,
            &mut purrdf::CanonicalizationControl::new(purrdf::RDFC_V2_CORPUS_POLICY, None)
        ),
        Err(purrdf::PipelineBundleError::HandleDigestMismatch { .. })
    ));

    let mut changed_diagnostics = compile_logic_carrier_graphs(&compile_inputs(compile_source(
        "urn:standpoint:logic",
        "urn:standpoint:changed-diagnostic",
    )))
    .unwrap();
    let diagnostic_only = commitment_bundle(changed_diagnostics.remove(0));
    assert_eq!(
        diagnostic_only
            .graph_digest(
                GRAPH_LOGIC,
                &mut purrdf::CanonicalizationControl::new(purrdf::RDFC_V2_CORPUS_POLICY, None)
            )
            .unwrap(),
        expected
    );
}

#[test]
fn empty_selected_input_and_source_declarations_become_the_destination_declaration() {
    for source_declaration in [None, Some("urn:carrier:previous-empty-owner")] {
        let mut builder = RdfDatasetBuilder::new();
        if let Some(graph) = source_declaration {
            let graph = builder.intern_iri(graph).unwrap();
            builder.declare_named_graph(graph).unwrap();
        }
        let source = builder.freeze().expect("freeze empty selection");
        let rooted = rooted_in_graph(&source, GRAPH_RELATIONAL_CORE).expect("root empty carrier");
        assert_eq!(rooted.rdf_row_count(), 0);
        assert_graph_owner(&rooted, GRAPH_RELATIONAL_CORE);
    }
}

#[test]
fn ordinary_projection_reinsert_preserves_content_and_source_location() {
    let ordinary = RdfQuad::new(
        RdfTerm::iri("urn:carrier:subject"),
        RELATION,
        RdfTerm::iri("urn:carrier:object"),
    )
    .with_location(RdfLocation::logical("synthetic ordinary projection"));
    let mut builder = RdfDatasetBuilder::new();
    builder.push_owned_quad(&ordinary).unwrap();
    let source = builder.freeze().unwrap();
    let rooted = rooted_in_graph(&source, GRAPH_LOGIC).unwrap();
    assert_graph_owner(&rooted, GRAPH_LOGIC);
    let recovered = rooted
        .project_graph(Some(&purrdf::RdfTerm::iri(GRAPH_LOGIC)))
        .unwrap();
    assert_eq!(recovered.owned_quads().collect::<Vec<_>>(), [ordinary]);
    assert_eq!(recovered.owned_reifiers().count(), 0);
    assert_eq!(recovered.owned_annotations().count(), 0);
}

/// The snapshot carrier is composed from selections over the producers' datasets instead
/// of the union of re-rooted copies it replaced. The coherence certificate pins the flat
/// canonical N-Quads of that carrier, so they must equal the former construction's, over
/// blank nodes shared by label across producers, an exact
/// duplicate quad from two producers, graph-scoped reifiers and annotations, a twin
/// placement, a whole-dataset placement, a preserved dataset whose blank node spans two
/// graphs and a selected graph the producer does not hold. Blank scoping must keep two
/// producers' equal labels apart and keep one source's blank node one node across
/// graphs. The composed carrier's wire labels differ from the former union's, so the
/// bundle must instead be byte-identical across two compositions of the same parts run
/// on different thread counts.
#[test]
fn composed_carrier_certifies_exactly_what_the_rooted_union_did() {
    const G1: &str = "https://example.org/g1";
    const G2: &str = "https://example.org/g2";
    const TWIN: &str = "https://example.org/twin";
    const WHOLE: &str = "https://example.org/whole";
    const MISSING: &str = "https://example.org/missing";
    const P: &str = "https://example.org/p";
    let source = |world: &str| {
        purrdf::parse_dataset(
            format!(
                r#"@prefix ex: <https://example.org/> .
                   @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
                   ex:s ex:p ex:o .
                   ex:g1 {{
                       ex:s ex:p ex:dup .
                       _:shared ex:p ex:o .
                       ex:claim rdf:reifies <<( _:shared ex:q "value"@en )>> ;
                           ex:accordingTo ex:{world} .
                   }}
                   ex:g2 {{ ex:s ex:p ex:same . _:other ex:p "x" . }}"#
            )
            .as_bytes(),
            "application/trig",
            None,
        )
        .expect("parse synthetic producer")
    };
    let alpha = source("alpha");
    let beta = source("beta");
    let spanning = purrdf::parse_dataset(
        br"@prefix ex: <https://example.org/> .
           ex:g3 { _:b ex:p ex:o . }
           ex:g4 { _:b ex:r ex:o . }",
        "application/trig",
        None,
    )
    .expect("parse synthetic graph-spanning dataset");
    let product = |stage: &str, dataset: &Arc<RdfDataset>| {
        StageProduct::from_artifacts_over(stage, Arc::clone(dataset), BTreeMap::new())
            .expect("the stage carrier digests")
    };
    let upstream = BTreeMap::from([
        ("stage-alpha".to_owned(), product("stage-alpha", &alpha)),
        ("stage-beta".to_owned(), product("stage-beta", &beta)),
    ]);

    let rooted = |dataset: &RdfDataset, graph: &str, placed: &str| {
        rooted_in_graph(
            &dataset
                .project_graph(Some(&RdfTerm::iri(graph)))
                .expect("project producer graph"),
            placed,
        )
        .expect("root producer graph")
    };
    let former = [
        rooted(alpha.as_ref(), G1, G1),
        rooted(beta.as_ref(), G1, G1),
        rooted(alpha.as_ref(), G2, G2),
        rooted(alpha.as_ref(), G1, TWIN),
        rooted(beta.as_ref(), MISSING, MISSING),
        rooted_in_graph(alpha.as_ref(), WHOLE).expect("root whole producer"),
        Arc::clone(&spanning),
    ];
    let former_refs: Vec<&RdfDataset> = former.iter().map(AsRef::as_ref).collect();
    let former = RdfDataset::union(&former_refs).expect("former carrier union");

    let compose = || {
        let mut parts = CarrierParts::new(&upstream);
        parts.push_graph("stage-alpha", G1).unwrap();
        parts.push_graph("stage-beta", G1).unwrap();
        parts.push_graph("stage-alpha", G2).unwrap();
        parts.push_graph_as("stage-alpha", G1, TWIN).unwrap();
        parts.push_graph("stage-beta", MISSING).unwrap();
        parts.push_placed(Arc::clone(&alpha), WHOLE);
        parts.push_dataset(Arc::clone(&spanning));
        parts.materialize().expect("composed carrier")
    };
    let composed = compose();

    let canonical = |dataset: &RdfDataset| {
        purrdf::canonical_flat_nquads(
            dataset,
            &mut purrdf::CanonicalizationControl::new(purrdf::RDFC_V2_CORPUS_POLICY, None),
        )
        .expect("carrier canonicalizes")
        .nquads()
        .to_owned()
    };
    assert_eq!(canonical(composed.as_ref()), canonical(&former));
    let graph_names = |dataset: &RdfDataset| {
        dataset
            .owned_named_graphs()
            .filter_map(|graph| match graph {
                RdfTerm::Iri(iri) => Some(iri),
                _ => None,
            })
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(graph_names(composed.as_ref()), graph_names(&former));
    assert!(graph_names(&former).contains(MISSING));

    let in_graph = |quad: &RdfQuad, graph: &str| quad.graph_name == Some(RdfTerm::iri(graph));
    let o = RdfTerm::iri("https://example.org/o");
    // The blank subjects of `?b predicate ex:o` in `graph`.
    let blank_subjects = |graph: &str, predicate: &str| {
        composed
            .owned_quads()
            .filter(|quad| in_graph(quad, graph) && quad.predicate == predicate && quad.object == o)
            .filter_map(|quad| match quad.subject {
                RdfTerm::BlankNode(label) => Some(label),
                _ => None,
            })
            .collect::<std::collections::BTreeSet<_>>()
    };
    // Two producers' `_:shared` stay two nodes in the graph they both feed.
    assert_eq!(blank_subjects(G1, P).len(), 2);
    // The exact duplicate quad both producers place into G1 is asserted once.
    let duplicate = RdfQuad::new(
        RdfTerm::iri("https://example.org/s"),
        P,
        RdfTerm::iri("https://example.org/dup"),
    );
    assert_eq!(
        composed
            .owned_quads()
            .filter(|quad| in_graph(quad, G1)
                && quad.subject == duplicate.subject
                && quad.predicate == duplicate.predicate
                && quad.object == duplicate.object)
            .count(),
        1
    );
    // One source's blank node stays one node across the graphs it spans.
    let spanning_node = blank_subjects("https://example.org/g3", P);
    assert_eq!(spanning_node.len(), 1);
    assert_eq!(
        blank_subjects("https://example.org/g4", "https://example.org/r"),
        spanning_node
    );
    // The whole-dataset placement keeps the reifier's quoted blank co-referent with the
    // quad that asserts it.
    let whole_node = blank_subjects(WHOLE, P);
    let quoted = composed
        .owned_reifiers()
        .filter(|row| row.graph == Some(RdfTerm::iri(WHOLE)))
        .filter_map(|row| match &row.statement.subject {
            RdfTerm::BlankNode(label) => Some(label.clone()),
            _ => None,
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(whole_node.len(), 1);
    assert_eq!(quoted, whole_node);
    drop(composed);

    let bundle_bytes = || {
        let carrier = compose();
        let mut builder = purrdf::gts_compose::SnapshotBuilder::new();
        let _ingestion = builder
            .add_view(carrier.as_ref())
            .expect("fold composed carrier");
        let content_id = builder.snapshot_content_id();
        // gmeow-test-input: synthetic-only
        let emission = gmeow_gts_profile::emit_gmeow_gts(
            builder,
            Vec::new(),
            Vec::new(),
            None,
            &gmeow_gts_profile::baseline_medium_plan(),
        )
        .expect("emit composed carrier through the production profile");
        (content_id, emission.bytes)
    };
    let pool = |threads: usize| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .expect("build thread pool")
    };
    let narrow = pool(1).install(bundle_bytes);
    let wide = pool(4).install(bundle_bytes);
    assert_eq!(narrow.0, wide.0);
    assert!(
        narrow.1 == wide.1,
        "composed carrier bundle bytes differ between runs"
    );
}

/// Two producers can carry the same raw blank label; the composed carrier keeps them two
/// nodes by scope. The per-graph axiom hashes the coherence certificate pins must keep
/// them apart too: moving a statement from one producer's `_:b` to the other's makes a
/// different graph, so it must make a different hash. Rendering the raw label alone
/// gives both carriers the same lines and the same hash. The hash must also reproduce
/// across two compositions of the same parts.
#[test]
fn axiom_hashes_distinguish_producers_blank_nodes_that_share_a_label() {
    const G: &str = "https://example.org/g";
    let parse = |trig: &str| {
        purrdf::parse_dataset(
            format!("@prefix ex: <https://example.org/> . ex:g {{ {trig} }}").as_bytes(),
            "application/trig",
            None,
        )
        .expect("parse synthetic producer")
    };
    let product = |stage: &str, dataset: Arc<RdfDataset>| {
        StageProduct::from_artifacts_over(stage, dataset, BTreeMap::new())
            .expect("the stage carrier digests")
    };
    let compose = |alpha: &str, beta: &str| {
        let upstream = BTreeMap::from([
            (
                "stage-alpha".to_owned(),
                product("stage-alpha", parse(alpha)),
            ),
            ("stage-beta".to_owned(), product("stage-beta", parse(beta))),
        ]);
        let mut parts = CarrierParts::new(&upstream);
        parts.push_graph("stage-alpha", G).unwrap();
        parts.push_graph("stage-beta", G).unwrap();
        parts.materialize().expect("composed carrier")
    };
    let hashes = |dataset: &RdfDataset| {
        gmeow_logic::certificate::per_graph_axiom_hashes(
            dataset,
            purrdf::gts::writer::digest_string,
        )
    };
    let canonical = |dataset: &RdfDataset| {
        purrdf::canonical_flat_nquads(
            dataset,
            &mut purrdf::CanonicalizationControl::new(purrdf::RDFC_V2_CORPUS_POLICY, None),
        )
        .expect("carrier canonicalizes")
        .nquads()
        .to_owned()
    };

    let on_alpha = compose("_:b ex:p ex:o1 . _:b ex:q ex:x .", "_:b ex:p ex:o2 .");
    let on_beta = compose("_:b ex:p ex:o1 .", "_:b ex:p ex:o2 . _:b ex:q ex:x .");
    assert_ne!(
        canonical(on_alpha.as_ref()),
        canonical(on_beta.as_ref()),
        "the two carriers are different graphs"
    );
    assert_ne!(hashes(on_alpha.as_ref()), hashes(on_beta.as_ref()));
    let again = compose("_:b ex:p ex:o1 . _:b ex:q ex:x .", "_:b ex:p ex:o2 .");
    assert_eq!(hashes(on_alpha.as_ref()), hashes(again.as_ref()));
}

/// The snapshot carrier refuses an empty authored base graph and an empty RDF 1.2
/// statement layer. Either is a regressed upstream read, and composing it would ship
/// empty graphs that every later gate accepts.
#[test]
fn the_carrier_refuses_an_empty_base_or_statement_layer() {
    let product = |stage: &str, dataset: Arc<RdfDataset>| {
        StageProduct::from_artifacts_over(stage, dataset, BTreeMap::new())
            .expect("the stage carrier digests")
    };
    let empty = || {
        RdfDatasetBuilder::new()
            .freeze()
            .expect("freeze empty dataset")
    };
    let refusal =
        |upstream: BTreeMap<String, StageProduct>| match super::assemble_carrier(&upstream) {
            Ok(_) => panic!("the carrier must refuse an empty required layer"),
            Err(error) => error.to_string(),
        };

    let empty_base = refusal(BTreeMap::from([
        (
            "stage-source-load".to_owned(),
            product("stage-source-load", empty()),
        ),
        (
            "stage-statements".to_owned(),
            product("stage-statements", empty()),
        ),
    ]));
    assert!(
        empty_base.contains("authored base graph is empty"),
        "{empty_base}"
    );

    let authored = purrdf::parse_dataset(
        format!(
            "<{GRAPH_AUTHORED_DEFAULT}> {{ <urn:carrier:s> <urn:carrier:p> <urn:carrier:o> . }}"
        )
        .as_bytes(),
        "application/trig",
        None,
    )
    .expect("parse synthetic authored base");
    let empty_statements = refusal(BTreeMap::from([
        (
            "stage-source-load".to_owned(),
            product("stage-source-load", authored),
        ),
        (
            "stage-statements".to_owned(),
            product("stage-statements", empty()),
        ),
    ]));
    assert!(
        empty_statements.contains("empty RDF 1.2 statement layer"),
        "{empty_statements}"
    );
}

/// The sink reads its artifact-only dependencies as released products. Every internal
/// artifact the reasoning archive folds must be in the sink's declared internal reads, or
/// the release drops it and the terminal fails after the whole build. Released with the
/// sink's declaration, the producers still supply the archive; released without it,
/// they do not.
#[test]
fn the_sink_declares_every_internal_artifact_its_reasoning_archive_reads() {
    use crate::node::Stage as _;

    let sink = crate::stages::gts_sink::GtsSinkStage::new();
    let declared = sink.internal_artifact_reads();
    let mut artifacts: BTreeMap<&str, BTreeMap<String, Vec<u8>>> = BTreeMap::new();
    for (producer, artifact) in super::REASONING_INTERNAL_READS {
        artifacts
            .entry(producer)
            .or_default()
            .insert(artifact.to_owned(), artifact.as_bytes().to_vec());
    }
    for path in [
        crate::stages::reason::EXPLANATIONS_PATH,
        crate::stages::reason::LEDGER_PATH,
        crate::stages::reason::PERF_LEDGER_PATH,
    ] {
        artifacts
            .entry("stage-reason")
            .or_default()
            .insert(path.to_owned(), path.as_bytes().to_vec());
    }
    let upstream = |keep: bool| -> BTreeMap<String, StageProduct> {
        artifacts
            .iter()
            .map(|(producer, artifacts)| {
                assert!(
                    sink.consumes().iter().any(|edge| edge == producer)
                        && !sink.carrier_consumes().iter().any(|edge| edge == producer),
                    "{producer} is an artifact-only dependency of the sink"
                );
                let kept: std::collections::BTreeSet<&str> = declared
                    .iter()
                    .filter(|(owner, _)| keep && owner == producer)
                    .map(|(_, artifact)| *artifact)
                    .collect();
                let product = StageProduct::from_artifacts(*producer, artifacts.clone())
                    .expect("synthetic product")
                    .into_carrier_released_keeping(&kept)
                    .expect("release");
                ((*producer).to_owned(), product)
            })
            .collect()
    };

    super::build_reasoning_blob(&upstream(true))
        .expect("the declared internal reads survive release");
    let Err(error) = super::build_reasoning_blob(&upstream(false)) else {
        panic!("without the declaration the release drops the internal artifacts");
    };
    assert!(
        error.to_string().contains("internal_artifact_reads"),
        "{error}"
    );
}
