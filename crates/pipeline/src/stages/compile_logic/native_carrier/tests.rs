// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! GMEOW assembly contracts over small native projections, without corpus inputs.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use gmeow_logic_compile::ir::{LogicProgram, PreservationKind};
use gmeow_logic_compile::loss_ledger::LossLedger;
use gmeow_logic_compile::projections::rdf::project_canonical_rdf12_dataset;
use gmeow_logic_compile::projections::report::{ProjectionReportRow, ReportHeader};
use purrdf::{BlankScope, RdfDataset, RdfDatasetBuilder, RdfLocation, RdfTerm};

use super::super::{
    CARRIER_GRAPHS, GRAPH_CORRESPONDENCE, GRAPH_LOGIC, GRAPH_RELATIONAL_CORE, build_logic_bundle,
    lower_program_with_formulas, project_correspondence_dataset, project_relational_core_dataset,
};
use super::{GRAPH_DIAGNOSTICS, assemble, compose};
use crate::bundle::{
    CompiledLogicPublication, LogicReportInputs, PipelineHandle, bundle_artifacts,
};
use crate::stages::diag_render::RenderedDiagnostics;

const RELATION: &str = "urn:compile-carrier:relation";
const STANDPOINT: &str = "https://blackcatinformatics.ca/logic/standpoint";
const PROVENANCE: &str = "https://blackcatinformatics.ca/logic/provenance";

fn source(graph: &str, context: &str, attach_location: bool) -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    let graph_id = builder.intern_iri(graph);
    let subject = builder.intern_blank("member", BlankScope::DEFAULT);
    let relation = builder.intern_iri(RELATION);
    let object = builder.intern_iri("urn:compile-carrier:object");
    let claim = builder.intern_blank("claim", BlankScope::DEFAULT);
    let statement = builder.intern_triple(subject, relation, object);
    let standpoint = builder.intern_iri(STANDPOINT);
    let provenance = builder.intern_iri(PROVENANCE);
    let context = builder.intern_iri(context);
    let row = builder.push_quad_with_handle(subject, relation, object, Some(graph_id));
    if attach_location {
        builder.attach_location(row, RdfLocation::file("synthetic-projection.ttl"));
    }
    builder.push_reifier_in_graph(claim, statement, Some(graph_id));
    builder.push_annotation_in_graph(claim, standpoint, context, Some(graph_id));
    builder.push_annotation_in_graph(claim, provenance, graph_id, Some(graph_id));
    let declaration = builder.intern_iri(&format!("{graph}/empty"));
    builder.declare_named_graph(declaration);
    builder.freeze().expect("freeze synthetic projection")
}

fn graph_names(dataset: &RdfDataset) -> BTreeSet<String> {
    dataset
        .owned_named_graphs()
        .map(|graph| match graph {
            RdfTerm::Iri(iri) => iri,
            other => panic!("unexpected carrier graph {other:?}"),
        })
        .collect()
}

#[test]
fn composite_shares_inputs_and_routes_every_rdf_layer_in_one_materialization() {
    let projections = std::array::from_fn(|index| {
        source(
            &format!("urn:old-projection:{index}"),
            &format!("urn:context:{index}"),
            false,
        )
    });
    let diagnostics = source(GRAPH_DIAGNOSTICS, "urn:context:diagnostics", false);
    let view = compose(projections.clone(), Arc::clone(&diagnostics)).unwrap();
    for (retained, original) in view
        .sources()
        .iter()
        .zip(projections.iter().chain(std::iter::once(&diagnostics)))
    {
        assert!(Arc::ptr_eq(
            retained.dataset().expect("native source"),
            original
        ));
    }
    assert_eq!(view.stats().retained_sources, 5);
    assert_eq!(view.stats().work.copied_rows, 0);
    assert_eq!(view.stats().work.materializations, 0);
    let dataset = view
        .materialize()
        .expect("materialize selected compile carrier");
    assert_eq!(view.stats().work.materializations, 1);
    assert_eq!(view.stats().work.copied_rows, dataset.rdf_row_count());
    assert_eq!(
        view.stats().work.freezes,
        2,
        "destination dictionary plus final RDF dataset"
    );

    let expected_graphs: BTreeSet<_> = CARRIER_GRAPHS
        .into_iter()
        .map(str::to_owned)
        .chain([
            GRAPH_DIAGNOSTICS.to_owned(),
            format!("{GRAPH_DIAGNOSTICS}/empty"),
        ])
        .collect();
    assert_eq!(
        graph_names(&dataset),
        expected_graphs,
        "old projection graphs and declarations disappear"
    );
    let mut subjects = Vec::new();
    for (index, graph) in CARRIER_GRAPHS
        .into_iter()
        .chain([GRAPH_DIAGNOSTICS])
        .enumerate()
    {
        let selected = dataset.project_named_graph(graph);
        let ordinary: Vec<_> = selected.owned_quads().collect();
        let reifiers: Vec<_> = selected.owned_reifiers().collect();
        let annotations: Vec<_> = selected.owned_annotations().collect();
        assert_eq!(ordinary.len(), 1, "ordinary projection in {graph}");
        assert_eq!(reifiers.len(), 1, "statement binding in {graph}");
        assert_eq!(
            annotations.len(),
            2,
            "statement context and provenance in {graph}"
        );
        assert_eq!(reifiers[0].statement.subject, ordinary[0].subject);
        assert_eq!(reifiers[0].statement.predicate, ordinary[0].predicate);
        assert_eq!(reifiers[0].statement.object, ordinary[0].object);
        assert!(
            annotations
                .iter()
                .all(|row| row.reifier == reifiers[0].reifier)
        );
        let context = if index == 3 {
            "urn:context:diagnostics".to_owned()
        } else {
            format!("urn:context:{index}")
        };
        let annotation = annotations
            .iter()
            .find(|row| row.predicate == STANDPOINT)
            .unwrap();
        assert_eq!(annotation.object, RdfTerm::iri(context));
        let former_graph = if index == 3 {
            GRAPH_DIAGNOSTICS.to_owned()
        } else {
            format!("urn:old-projection:{index}")
        };
        let provenance = annotations
            .iter()
            .find(|row| row.predicate == PROVENANCE)
            .unwrap();
        assert_eq!(
            provenance.object,
            RdfTerm::iri(former_graph),
            "replacing graph ownership must retain the source graph's provenance reference"
        );
        subjects.push(ordinary[0].subject.clone());
    }
    for (index, subject) in subjects.iter().enumerate() {
        assert!(
            subjects[..index].iter().all(|other| other != subject),
            "separate projection sources must never alias same-label blanks"
        );
    }
}

#[test]
fn empty_projections_replace_source_declarations_and_preserve_diagnostic_declarations() {
    let empty = |graph: &str| {
        let mut builder = RdfDatasetBuilder::new();
        let graph = builder.intern_iri(graph);
        builder.declare_named_graph(graph);
        builder.freeze().unwrap()
    };
    let projections = std::array::from_fn(|index| empty(&format!("urn:old-empty:{index}")));
    let diagnostics = empty(GRAPH_DIAGNOSTICS);
    let dataset = assemble(projections, diagnostics).unwrap();
    assert_eq!(dataset.rdf_row_count(), 0);
    assert_eq!(
        graph_names(&dataset),
        CARRIER_GRAPHS
            .into_iter()
            .chain([GRAPH_DIAGNOSTICS])
            .map(str::to_owned)
            .collect()
    );
}

#[test]
fn every_producer_input_refuses_unrepresented_physical_row_locations() {
    for attached_input in 0..4 {
        let projections = std::array::from_fn(|index| {
            source(
                &format!("urn:old-projection:{index}"),
                &format!("urn:context:{index}"),
                attached_input == index,
            )
        });
        let diagnostics = source(
            GRAPH_DIAGNOSTICS,
            "urn:context:diagnostics",
            attached_input == 3,
        );
        let error = assemble(projections, diagnostics).unwrap_err().to_string();
        let owner = CARRIER_GRAPHS
            .get(attached_input)
            .copied()
            .unwrap_or(GRAPH_DIAGNOSTICS);
        assert!(error.contains(owner), "{error}");
        assert!(error.contains("physical row locations"), "{error}");
    }
}

#[test]
fn assembled_bundle_retains_full_native_handles_loss_artifacts_and_diagnostic_locations() {
    let program = Arc::new(LogicProgram::new(
        vec![],
        vec![],
        vec![],
        Some("urn:compile-source".into()),
    ));
    let canonical = project_canonical_rdf12_dataset(&program).unwrap().dataset;
    let mut relational = lower_program_with_formulas(&program);
    relational.push_residue("synthetic structural obligation remains in the native carrier");
    let expected_relational = relational.clone();
    let relational_projection = project_relational_core_dataset(&relational).unwrap();
    let correspondence = super::super::test_fixtures::synthetic_affine_program();
    let expected_correspondence = correspondence.clone();
    let correspondence_projection = project_correspondence_dataset(&correspondence).unwrap();

    let mut loss = LossLedger::new();
    loss.record_projection_drops(
        "synthetic-target",
        PreservationKind::SoundUnder,
        &["target omits a source capability".into()],
        &["source capability retained as loss evidence".into()],
    );
    let report = LogicReportInputs {
        header: ReportHeader::of_program(&program),
        base_correspondence_count: correspondence.correspondences.len(),
        base_lawful_uplift_count: 0,
        projections: vec![ProjectionReportRow {
            target: "synthetic-target".into(),
            is_rdf: true,
            preservation: PreservationKind::SoundUnder,
            complexity: "linear".into(),
        }],
        loss,
    };
    let expected_report = crate::handle_identity::typed_digest(&report);
    let mut findings = gmeow_errors::Report::new("logic-compile");
    let mut finding = gmeow_errors::Finding::new(
        gmeow_errors::Severity::Note,
        "compile.synthetic",
        "retain source attribution in diagnostic RDF",
    );
    finding.locations.push(gmeow_errors::model::Location::new(
        Some("synthetic-projection.ttl".into()),
        Some(7),
        Some(11),
        None,
    ));
    findings.add_finding(finding);
    let diagnostics = gmeow_errors::render::to_gmeow_dataset(&findings).unwrap();
    let expected_diagnostics = purrdf::canonicalize(&diagnostics).nquads;
    assert!(expected_diagnostics.contains("synthetic-projection.ttl"));
    let artifacts = BTreeMap::from([(
        "pipeline/synthetic-proof.json".into(),
        b"{\"source\":\"synthetic\"}".to_vec(),
    )]);
    let bundle = build_logic_bundle(
        CompiledLogicPublication {
            program: Arc::clone(&program),
            report,
        },
        canonical,
        relational,
        relational_projection,
        correspondence,
        correspondence_projection,
        RenderedDiagnostics {
            artifacts: artifacts.clone(),
            dataset: diagnostics,
            report: Arc::new(findings.normalized()),
        },
    )
    .expect("admit native producer outputs");
    assert_eq!(bundle_artifacts(&bundle), artifacts);
    for graph in CARRIER_GRAPHS {
        assert_eq!(
            bundle.handle(graph).unwrap().content_digest,
            bundle.graph_digest(graph)
        );
    }
    let PipelineHandle::CompiledLogic(publication) = &bundle.handle(GRAPH_LOGIC).unwrap().payload
    else {
        panic!("the full native compiler publication must survive");
    };
    assert!(Arc::ptr_eq(&publication.program, &program));
    assert_eq!(
        crate::handle_identity::typed_digest(&publication.report),
        expected_report
    );
    let PipelineHandle::RelationalCore(carried_relational) =
        &bundle.handle(GRAPH_RELATIONAL_CORE).unwrap().payload
    else {
        panic!("native relational program missing");
    };
    assert_eq!(carried_relational.as_ref(), &expected_relational);
    let PipelineHandle::Correspondence(carried_correspondence) =
        &bundle.handle(GRAPH_CORRESPONDENCE).unwrap().payload
    else {
        panic!("native correspondence program missing");
    };
    assert_eq!(carried_correspondence.as_ref(), &expected_correspondence);
    let diagnostics = bundle.dataset().project_named_graph(GRAPH_DIAGNOSTICS);
    let restored =
        crate::stages::carrier::rooted_in_graph(&diagnostics, GRAPH_DIAGNOSTICS).unwrap();
    assert_eq!(purrdf::canonicalize(&restored).nquads, expected_diagnostics);
}
