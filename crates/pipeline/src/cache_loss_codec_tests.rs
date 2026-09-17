// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Actual pipeline cache codecs over small, entirely synthetic correspondence IR.

use super::*;
use gmeow_logic_compile::ir::{
    AxisEvidence, Correspondence, CorrespondenceCaveat, CorrespondenceRelation, Determinacy,
    FiniteNumericLiteral, Formula, LogicProgram, MorphismClass, MorphismKind, PreservationKind,
    RecoveryCaseIr, Term, UnitInterval,
};
use gmeow_logic_compile::projections::correspondence::{
    CorrespondenceProgram, project_correspondence_dataset,
};
use purrdf::{PipelineBundle, RdfLiteral, RdfTextDirection};

const GRAPH: &str = "https://example.org/codec-graph";
const DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";

fn directional(text: &str) -> RdfLiteral {
    let mut literal = RdfLiteral::language_tagged(text, "ar");
    literal.direction = Some(RdfTextDirection::Rtl);
    literal
}

fn cell(name: &str, losses: Vec<RdfLiteral>) -> Correspondence {
    let iri = format!("https://example.org/{name}");
    let coordinate = || UnitInterval::new(RdfLiteral::typed("0.1250", DECIMAL)).unwrap();
    Correspondence::new(
        &iri,
        CorrespondenceRelation::Subsumes,
        MorphismClass::LossyLens,
        MorphismKind::InstitutionMorphism,
        false,
        Some(Determinacy::Crisp),
        Some(format!("{iri}/get")),
        Some(format!("{iri}/put")),
        vec![],
        Some(coordinate()),
        Some(coordinate()),
        Some(FiniteNumericLiteral::new(RdfLiteral::typed("-12.500", DECIMAL)).unwrap()),
        Some(coordinate()),
        Some(format!("{iri}/observer")),
        Some(PreservationKind::SoundUnder),
    )
    .unwrap()
    .with_caveats(vec![CorrespondenceCaveat {
        iri: format!("{iri}/caveat"),
        comments: vec![
            directional("تحذير"),
            RdfLiteral::typed("0001", "https://example.org/noteCode"),
        ],
    }])
    .unwrap()
    .with_loss_evidence(losses)
    .unwrap()
    .with_axis_evidence(
        AxisEvidence::new(
            vec![format!("{iri}/source")],
            Some(format!("{iri}/scale")),
            Some(format!("{iri}/model")),
        )
        .unwrap(),
    )
    .with_endpoints(format!("{iri}/source-term"), format!("{iri}/target-term"))
    .unwrap()
    .as_grounding()
    .with_recovery_cases(vec![
        RecoveryCaseIr::new(
            format!("{iri}/case"),
            Formula::atom(
                Term::iri(format!("{iri}/relation")).unwrap(),
                vec![
                    Term::iri(format!("{iri}/subject")).unwrap(),
                    Term::rdf_literal(directional("شاهد")).unwrap(),
                ],
            )
            .unwrap(),
        )
        .unwrap(),
    ])
    .unwrap()
}

/// Empty evidence precedes and follows populated evidence; every following field
/// has meaningful content, so an omitted length cannot masquerade as another field.
fn mixed_program() -> CorrespondenceProgram {
    CorrespondenceProgram::new(
        vec![
            cell("a-empty", vec![]),
            cell(
                "b-evidence",
                vec![
                    RdfLiteral::simple("The view omits timestamps."),
                    RdfLiteral::language_tagged("The view omits timestamps.", "en"),
                    RdfLiteral::language_tagged("La vue omet les horodatages.", "fr"),
                    directional("تحذير"),
                    RdfLiteral::typed("0007", "https://example.org/lossCode"),
                ],
            ),
            cell("c-empty", vec![]),
        ],
        PreservationKind::SoundUnder,
    )
}

fn handles(program: CorrespondenceProgram) -> [PipelineHandle; 2] {
    let logic = LogicProgram::new(
        vec![],
        vec![],
        vec![],
        Some("synthetic-codec-source".into()),
    )
    .with_correspondences(program.correspondences.clone())
    .unwrap();
    [
        PipelineHandle::Correspondence(Arc::new(program)),
        PipelineHandle::Logic(Arc::new(logic)),
    ]
}

fn assert_same(expected: &PipelineHandle, restored: &PipelineHandle) {
    match (expected, restored) {
        (PipelineHandle::Correspondence(expected), PipelineHandle::Correspondence(restored)) => {
            assert_eq!(restored, expected);
            assert_eq!(restored.content_key(), expected.content_key());
        }
        (PipelineHandle::Logic(expected), PipelineHandle::Logic(restored)) => {
            assert_eq!(restored, expected);
            assert_eq!(restored.canonical_key(), expected.canonical_key());
        }
        _ => panic!("restored handle must retain the selected arm"),
    }
    assert_eq!(
        handle_payload_digest(restored),
        handle_payload_digest(expected)
    );
}

fn product(handle: PipelineHandle) -> StageProduct {
    let projection = match &handle {
        PipelineHandle::Correspondence(program) => project_correspondence_dataset(program).unwrap(),
        PipelineHandle::Logic(program) => {
            gmeow_logic_compile::projections::rdf::project_canonical_rdf12_dataset(program)
                .unwrap()
                .dataset
        }
        _ => panic!("only the two selected binary codecs enter this fixture"),
    };
    let dataset = crate::stages::carrier::rooted_in_graph(&projection, GRAPH).unwrap();
    let mut bundle = PipelineBundle::new(
        dataset,
        RdfLookaside::default(),
        Arc::new(ContentStore::new()),
        DatasetProvenance::new(),
    );
    let pin = bundle.graph_digest(GRAPH);
    bundle.pin_handle(GRAPH, handle, pin).unwrap();
    StageProduct::from_bundle("stage-loss-codec", Arc::new(bundle))
}

#[test]
fn binary_handle_codecs_preserve_mixed_loss_vectors_and_following_fields() {
    for handle in handles(mixed_program()) {
        let bytes = encode_handle(&handle).unwrap();
        let restored = rebuild_handle(handle_arm_tag(&handle), Some(&bytes)).unwrap();
        assert_same(&handle, &restored);
        assert_eq!(encode_handle(&restored).unwrap(), bytes);
    }
}

#[test]
fn persistent_actions_preserve_loss_vectors_in_both_binary_handle_arms() {
    for handle in handles(mixed_program()) {
        let expected = handle.clone();
        let product = product(handle);
        let selection = ReceiptOutputSelection {
            graphs: vec![GRAPH.into()],
            blob_representations: vec![],
            logical_artifacts: vec![],
            handles: vec![GRAPH.into()],
            default_graph: default_graph_commitment(&product).unwrap(),
            provenance: provenance_commitment(&product).unwrap(),
            content_store: content_store_commitment(&product).unwrap(),
        };
        let dir = tempfile::tempdir().unwrap();
        let cache = PipelineCache::open(dir.path()).unwrap();
        let context = StageKeyContext::new("stage-loss-codec", "fixed-loss-vector", vec![], vec![])
            .with_dimension("handle-arm", handle_arm_tag(&expected));
        let receipt = cache
            .put(&context, "stable", "persistent", &selection, &product)
            .unwrap();
        let hit = cache.get(&context).unwrap().unwrap();
        assert_eq!(hit.receipt, receipt);
        assert_eq!(hit.product.digest, product.digest);
        let restored = &hit.product.bundle().handle(GRAPH).unwrap().payload;
        assert_same(&expected, restored);
        assert_eq!(
            hit.product.bundle().graph_digest(GRAPH),
            product.bundle().graph_digest(GRAPH)
        );
    }
}

#[test]
fn binary_handle_decoders_keep_caveats_nonempty_and_reject_invalid_loss_metadata() {
    let mut empty_caveat = mixed_program();
    empty_caveat.correspondences[0].caveats[0].comments.clear();
    for handle in handles(empty_caveat) {
        let bytes = encode_handle(&handle).unwrap();
        let error = rebuild_handle(handle_arm_tag(&handle), Some(&bytes)).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("caveat requires at least one comment"),
            "{error}"
        );
    }
    let mut invalid_loss = mixed_program();
    let directional_loss = invalid_loss.correspondences[1]
        .loss_evidence
        .iter_mut()
        .find(|literal| literal.direction.is_some())
        .unwrap();
    directional_loss.language = None;
    for handle in handles(invalid_loss) {
        let bytes = encode_handle(&handle).unwrap();
        assert!(rebuild_handle(handle_arm_tag(&handle), Some(&bytes)).is_err());
    }
}
