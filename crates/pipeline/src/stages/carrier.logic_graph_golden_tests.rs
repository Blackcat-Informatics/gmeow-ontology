// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_logic_compile::ir::{ContextualScope, LogicAxiom, LogicProgram};

const GRAPH_LOGIC: &str = crate::stages::compile_logic::GRAPH_LOGIC;

/// A small, FIXED clean program — its canonical RDF-1.2 projection is the byte
/// golden subject. Deliberately synthetic (not the real module) so the golden is
/// stable and the per-graph fold is regression-pinned independent of the full
/// gmeow.gts and independent of any logic-module edit.
fn fixed_program() -> LogicProgram {
    let ax = |s: &str, o: &str| {
        LogicAxiom::new(
            s,
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
            gmeow_logic_compile::ir::AtomicTerm::resource(o),
            false,
            ContextualScope::default(),
        )
        .expect("valid axiom")
    };
    LogicProgram::new(
        vec![
            ax(
                "https://blackcatinformatics.ca/gmeow/Animal",
                "https://blackcatinformatics.ca/logic/Kind",
            ),
            ax(
                "https://blackcatinformatics.ca/gmeow/Cat",
                "https://blackcatinformatics.ca/logic/Subkind",
            ),
        ],
        vec![],
        vec![],
        None,
    )
}

/// Read one selected GMEOW graph as sorted N-Triples through the native
/// decoder and serializer. The golden retains literal facets and statement
/// metadata; lexical strings alone are not an RDF graph.
fn folded_graph_ntriples(gts: &[u8], graph_iri: &str) -> String {
    let graph = purrdf::gts::read_graph(gts, true).expect("read graph");
    let bundle = purrdf::import_gts_graph(graph).expect("import full native graph");
    let graph_name = purrdf::TermValue::Iri(graph_iri.to_owned());
    let bytes = purrdf::serialize_dataset(
        &bundle.dataset,
        "application/n-triples",
        purrdf::SerializeGraph::Named(&graph_name),
    )
    .expect("serialize the selected graph faithfully");
    let text = String::from_utf8(bytes).expect("N-Triples is UTF-8");
    let mut rows: Vec<_> = text.lines().collect();
    rows.sort_unstable();
    rows.join("\n")
}

/// Byte golden: the `graph/logic` named-graph content of an emitted
/// snapshot, over a FIXED synthetic program. Pins the per-graph fold path
/// (canonical RDF-1.2 → N-Quads → add_named canonicalization → emit → read-back)
/// byte-for-byte, independent of the full gmeow.gts. A second emit is asserted
/// byte-identical (determinism).
#[test]
fn graph_logic_fold_byte_golden() {
    let arts =
        gmeow_logic_compile::projections::compile_program(&fixed_program(), |_| Default::default())
            .expect("compile fixed program");
    let logic_nq = turtle_to_nquads(arts.canonical_rdf12.clone().into_text().content.as_bytes())
        .expect("turtle → nq");

    let build = || {
        let mut builder = SnapshotBuilder::new();
        add_base_nq(
            &mut builder,
            b"<https://blackcatinformatics.ca/gmeow/> \
                  <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
                  <http://www.w3.org/2002/07/owl#Ontology> .\n",
            "base",
        )
        .expect("fold base graph");
        add_named(&mut builder, &logic_nq, GRAPH_LOGIC, "logic").expect("fold graph/logic");
        // gmeow-test-input: synthetic-only
        emit_gts(
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
        .expect("emit snapshot")
    };

    let gts = build();
    let folded = folded_graph_ntriples(&gts, GRAPH_LOGIC);
    assert!(!folded.is_empty(), "graph/logic must carry the projection");
    insta::assert_snapshot!("graph_logic_fold", folded);

    // Determinism: a second build folds the SAME graph/logic content.
    let gts2 = build();
    assert_eq!(
        folded_graph_ntriples(&gts2, GRAPH_LOGIC),
        folded,
        "the graph/logic fold must be byte-deterministic"
    );
}

const GRAPH_REASONING: &str = gmeow_logic::result_rdf::GRAPH_REASONING;

/// A FIXED synthetic reasoning result — the byte-golden subject for the
/// `graph/reasoning` fold (deliberately synthetic so the golden is stable and
/// independent of any reasoner output).
fn fixed_reasoning_result() -> gmeow_logic::result::ReasoningResult {
    use gmeow_logic::result::{
        CompletenessStatus, EvaluationStatus, InformationState, InputStatus, PreservationClaim,
        ReasoningResult, ResultPayload, ResultProvenance,
    };
    ReasoningResult::new(
        InputStatus::Valid,
        EvaluationStatus::Completed,
        CompletenessStatus::CompleteForFragment,
        PreservationClaim::exact(),
        InformationState::Supported,
        ResultProvenance::native(
            "contract:golden",
            "https://blackcatinformatics.ca/gmeow/graph/world/actual",
        ),
        ResultPayload::Empty,
    )
}

/// Byte golden: the `graph/reasoning` named-graph content of an emitted
/// snapshot, over a FIXED synthetic reasoning result. Pins the per-graph fold path
/// (project → N-Triples → add_named canonicalization → emit → read-back)
/// byte-for-byte, independent of the full gmeow.gts. A second emit is asserted
/// byte-identical (determinism).
#[test]
fn graph_reasoning_fold_byte_golden() {
    let reasoning_nt = gmeow_logic::result_rdf::project_reasoning_result(&fixed_reasoning_result())
        .expect("project the valid synthetic golden result");

    let build = || {
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
            reasoning_nt.as_bytes(),
            GRAPH_REASONING,
            "reasoning",
        )
        .expect("fold graph/reasoning");
        // gmeow-test-input: synthetic-only
        emit_gts(
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
        .expect("emit snapshot")
    };

    let gts = build();
    let folded = folded_graph_ntriples(&gts, GRAPH_REASONING);
    assert!(
        !folded.is_empty(),
        "graph/reasoning must carry the projection"
    );
    insta::assert_snapshot!("graph_reasoning_fold", folded);

    // Determinism: a second build folds the SAME graph/reasoning content.
    let gts2 = build();
    assert_eq!(
        folded_graph_ntriples(&gts2, GRAPH_REASONING),
        folded,
        "the graph/reasoning fold must be byte-deterministic"
    );
}

const GRAPH_RELATIONAL_CORE: &str = crate::stages::compile_logic::GRAPH_RELATIONAL_CORE;

/// A FIXED synthetic relational-core program — the byte-golden subject for the
/// `graph/relational-core` fold (a clean Horn program with one rule, so the golden
/// is stable and independent of the real module).
fn fixed_relational_core() -> gmeow_logic_compile::relational_core::RelationalCoreProgram {
    use gmeow_logic_compile::ir::{ContextualScope, LogicAxiom, LogicProgram, LogicRule};
    use gmeow_logic_compile::relational_core::lower_program;
    let sc = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
    let ax = |s: &str, p: &str, o: &str| {
        LogicAxiom::new(
            s,
            p,
            gmeow_logic_compile::ir::AtomicTerm::resource(o),
            false,
            ContextualScope::default(),
        )
        .expect("axiom")
    };
    // ?x sc ?z :- ?x sc ?y, ?y sc ?z .
    let rule = LogicRule::new(
        ax("?x", sc, "?z"),
        vec![ax("?x", sc, "?y"), ax("?y", sc, "?z")],
        vec![],
        ContextualScope::default(),
    );
    let program = LogicProgram::new(
        vec![ax(
            "https://blackcatinformatics.ca/gmeow/Cat",
            sc,
            "https://blackcatinformatics.ca/gmeow/Animal",
        )],
        vec![rule],
        vec![],
        None,
    );
    lower_program(&program)
}

/// Byte golden: the `graph/relational-core` named-graph content of an
/// emitted snapshot, over a FIXED synthetic relational-core program. Pins the
/// native per-graph fold path (lower → native projection → GMEOW GTS → read-back)
/// byte-for-byte, independent of the full gmeow.gts. A second emit
/// is asserted byte-identical (determinism).
#[test]
fn graph_relational_core_fold_byte_golden() {
    let program = fixed_relational_core();
    let projection =
        gmeow_logic_compile::relational_core::project_relational_core_dataset(&program)
            .expect("native relational projection");

    let build = || {
        let mut builder = SnapshotBuilder::new();
        add_base_nq(
            &mut builder,
            b"<https://blackcatinformatics.ca/gmeow/> \
                  <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
                  <http://www.w3.org/2002/07/owl#Ontology> .\n",
            "base",
        )
        .expect("fold base graph");
        builder
            .add_dataset_scoped(&projection, Some(GRAPH_RELATIONAL_CORE), None)
            .expect("fold native graph/relational-core");
        // gmeow-test-input: synthetic-only
        {
            let emission = gmeow_gts_profile::emit_gmeow_gts(
                builder,
                Vec::new(),
                Vec::new(),
                None,
                &gmeow_gts_profile::baseline_medium_plan(),
            )
            .expect("emit production-profile snapshot");
            assert!(
                emission.ingestion.declarations_omitted.is_empty(),
                "unexpected GMEOW fixture graph omissions: {:?}",
                emission.ingestion.declarations_omitted
            );
            emission.bytes
        }
    };

    let gts = build();
    let folded = folded_graph_ntriples(&gts, GRAPH_RELATIONAL_CORE);
    let imported = purrdf::import_gts_graph(purrdf::gts::read_graph(&gts, true).unwrap()).unwrap();
    let graph = imported.dataset.project_named_graph(GRAPH_RELATIONAL_CORE);
    let restored = gmeow_logic_compile::relational_core::parse_relational_core(&graph).unwrap();
    assert_eq!(
        program.projection_key().unwrap(),
        restored.projection_key().unwrap(),
        "the shipped graph must restore the complete relational program"
    );
    assert!(
        !folded.is_empty(),
        "graph/relational-core must carry the projection"
    );
    insta::assert_snapshot!("graph_relational_core_fold", folded);

    // Determinism: a second build folds the SAME graph/relational-core content.
    let gts2 = build();
    assert_eq!(
        folded_graph_ntriples(&gts2, GRAPH_RELATIONAL_CORE),
        folded,
        "the graph/relational-core fold must be byte-deterministic"
    );
}

const GRAPH_CORRESPONDENCE: &str = crate::stages::compile_logic::GRAPH_CORRESPONDENCE;

/// Byte golden: the `graph/correspondence` named-graph content of an
/// emitted snapshot, over the §14 affine-triangle worked example. Pins the per-graph
/// fold path (construct → project N-Triples → add_named canonicalization → emit →
/// read-back) byte-for-byte, independent of the full gmeow.gts. Also asserts the
/// load-bearing correctness point in the folded bytes: `skos:relatedMatch` present,
/// `skos:exactMatch` + `owl:equivalentClass` absent, the loss-ledger row present. A
/// second emit is asserted byte-identical (determinism).
#[test]
fn graph_correspondence_fold_byte_golden() {
    let correspondence = crate::stages::compile_logic::synthetic_affine_program();
    let corr_nt =
        gmeow_logic_compile::projections::correspondence::project_correspondence(&correspondence);

    let build = || {
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
            corr_nt.as_bytes(),
            GRAPH_CORRESPONDENCE,
            "correspondence",
        )
        .expect("fold graph/correspondence");
        // gmeow-test-input: synthetic-only
        emit_gts(
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
        .expect("emit snapshot")
    };

    let gts = build();
    let folded = folded_graph_ntriples(&gts, GRAPH_CORRESPONDENCE);
    let dataset = purrdf::parse_dataset(folded.as_bytes(), "application/n-triples", None)
        .expect("the fold golden is actual RDF");
    let restored = gmeow_logic_compile::projections::correspondence::parse_correspondence(&dataset)
        .expect("rehydrate the folded correspondence");
    assert_eq!(
        restored, correspondence,
        "GMEOW folding must preserve the complete typed correspondence"
    );
    assert!(
        !folded.is_empty(),
        "graph/correspondence must carry the projection"
    );
    // The load-bearing correctness point, asserted on the FOLDED snapshot bytes —
    // checking the alignment PREDICATE position, not bare substrings (the loss-ledger
    // prose mentions the forbidden predicate names as disclosure, not as edges).
    assert!(
        folded.contains("<http://www.w3.org/2004/02/skos/core#relatedMatch>"),
        "the folded correspondence graph keeps the overlap at skos:relatedMatch:\n{folded}"
    );
    assert!(
        !folded.contains("<http://www.w3.org/2004/02/skos/core#exactMatch>"),
        "the folded correspondence graph MUST NOT emit a skos:exactMatch edge:\n{folded}"
    );
    assert!(
        !folded.contains("<http://www.w3.org/2002/07/owl#equivalentClass>"),
        "the folded correspondence graph MUST NOT emit an owl:equivalentClass edge:\n{folded}"
    );
    assert!(
        folded.contains("lossyDrop"),
        "the folded correspondence graph MUST carry the loss-ledger row:\n{folded}"
    );
    insta::assert_snapshot!("graph_correspondence_fold", folded);

    // Determinism: a second build folds the SAME graph/correspondence content.
    let gts2 = build();
    assert_eq!(
        folded_graph_ntriples(&gts2, GRAPH_CORRESPONDENCE),
        folded,
        "the graph/correspondence fold must be byte-deterministic"
    );
}

const GRAPH_PROVENANCE: &str = crate::stages::provenance_graph::GRAPH_PROVENANCE;

/// A FIXED synthetic provenance projection — the byte-golden subject for the
/// `graph/provenance` fold. Three units (root / source / import) so every
/// `OriginKind` branch is exercised; deliberately synthetic so the golden is
/// stable and independent of the real ontology (whose unit set churns).
fn fixed_provenance_projection() -> Vec<(usize, String, String, String, Option<String>)> {
    vec![
        (
            0,
            "imports/prov.ttl".to_string(),
            "import".to_string(),
            "imports/prov.ttl".to_string(),
            None,
        ),
        (
            1,
            "ontology/gmeow.ttl".to_string(),
            "root-ontology".to_string(),
            "ontology/gmeow.ttl".to_string(),
            None,
        ),
        (
            2,
            "slices/core/epistemics/module.ttl".to_string(),
            "source".to_string(),
            "slices/core/epistemics/module.ttl".to_string(),
            None,
        ),
    ]
}

/// Byte golden: the `graph/provenance` named-graph content of an emitted
/// snapshot, over a FIXED synthetic provenance projection. Pins the per-graph fold
/// path (public projection → N-Triples → add_named canonicalization → emit →
/// read-back) byte-for-byte, independent of the full gmeow.gts. A second emit is
/// asserted byte-identical (determinism). The golden ALSO proves S0.5 (no runtime id).
#[test]
fn graph_provenance_fold_byte_golden() {
    let prov_nt =
        crate::stages::provenance_graph::project_provenance_graph(&fixed_provenance_projection());

    let build = || {
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
            prov_nt.as_bytes(),
            GRAPH_PROVENANCE,
            "provenance",
        )
        .expect("fold graph/provenance");
        // gmeow-test-input: synthetic-only
        emit_gts(
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
        .expect("emit snapshot")
    };

    let gts = build();
    let folded = folded_graph_ntriples(&gts, GRAPH_PROVENANCE);
    assert!(
        !folded.is_empty(),
        "graph/provenance must carry the projection"
    );
    // S0.5: the folded bytes must NOT contain any runtime id.
    assert!(
        !folded.contains("unit#"),
        "no runtime UnitId in graph/provenance"
    );
    assert!(
        !folded.contains("artifact#"),
        "no runtime ArtifactId in graph/provenance"
    );
    assert!(
        !folded.contains("origin-set#"),
        "no runtime OriginSetId in graph/provenance"
    );
    insta::assert_snapshot!("graph_provenance_fold", folded);

    // Determinism: a second build folds the SAME graph/provenance content.
    let gts2 = build();
    assert_eq!(
        folded_graph_ntriples(&gts2, GRAPH_PROVENANCE),
        folded,
        "the graph/provenance fold must be byte-deterministic"
    );
}
