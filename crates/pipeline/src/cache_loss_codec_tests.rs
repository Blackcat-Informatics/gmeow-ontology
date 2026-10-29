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
    let pin = bundle.graph_digest(GRAPH).unwrap();
    bundle.pin_handle(GRAPH, handle, pin).unwrap();
    StageProduct::from_bundle("stage-loss-codec", Arc::new(bundle))
        .expect("the stage carrier digests")
}

#[test]
fn binary_handle_codecs_preserve_mixed_loss_vectors_and_following_fields() {
    use gmeow_logic_compile::ir::{
        CorrespondenceComposition, CorrespondenceLaw, DischargeCondition, DischargeVerdict,
        LawClaimIr, LegPath, TransactionProgramIr,
    };
    use gmeow_logic_compile::projections::{
        correspondence::NativeCorrespondenceSource,
        grounding_catalog::{GroundingSourceIdentity, LiteralProjectionSupport, PropertyPremise},
        profile_program::{
            ProjectionBindingProgram, ProjectionInputCheck, ProjectionProfilePayload,
            ProjectionProfileProgram,
        },
        selected_templates::SelectedTemplate,
    };

    // Complete original packets pin the native wire and actual producer identities.
    let check_reference = |case: &str, program: &CorrespondenceProgram| {
        let reference = include_str!("cache/fixtures/correspondence-cbor-v1.txt")
            .lines()
            .map(|line| line.split('|').collect::<Vec<_>>())
            .find(|fields| fields[0] == case)
            .expect("every complete packet has an original reference");
        assert_eq!(reference.len(), 6);
        let mut bytes = Vec::new();
        program.write_cbor(&mut bytes).unwrap();
        assert_eq!(
            purrdf_hash::hex::encode(&bytes),
            reference[1],
            "complete original packet: {case}"
        );
        let raw = purrdf_hash::blake3::hash(&bytes);
        assert_eq!(
            purrdf_hash::hex::encode(raw.as_bytes()),
            reference[2],
            "original raw identity: {case}"
        );
        let handle = PipelineHandle::Correspondence(Arc::new(program.clone()));
        assert_eq!(
            handle_payload_digest(&handle),
            reference[3],
            "actual payload identity: {case}"
        );
        let commitment = crate::handle_identity::handle_commitment(
            "urn:correspondence:capture:graph",
            "synthetic-correspondence-graph-pin",
            &handle,
        );
        assert_eq!(
            commitment.identity, reference[4],
            "actual commitment arm: {case}"
        );
        assert_eq!(
            commitment.digest, reference[5],
            "actual commitment identity: {case}"
        );
    };
    check_reference(
        "empty",
        &CorrespondenceProgram::new(vec![], PreservationKind::Exact),
    );
    check_reference("mixed-actual", &mixed_program());

    let mut left_literal = RdfLiteral::language_tagged("left λ\0𝄞", "en");
    left_literal.direction = Some(RdfTextDirection::Ltr);
    let leaf = Formula::Atom {
        relation: Term::Iri("urn:correspondence:relation".into()),
        args: vec![
            Term::Var("x".into()),
            Term::Iri("urn:correspondence:constant".into()),
            Term::Literal(RdfLiteral::simple("plain λ\0𝄞")),
            Term::Literal(left_literal.clone()),
            Term::Literal(directional("يمين")),
            Term::Literal(RdfLiteral::typed("0007", "urn:correspondence:datatype")),
            Term::SequenceMarker("remaining".into()),
            Term::App {
                symbol: "urn:correspondence:outer".into(),
                args: vec![Term::App {
                    symbol: "urn:correspondence:inner".into(),
                    args: vec![Term::Var("y".into())],
                }],
            },
        ],
    };
    let transform = Formula::Forall {
        vars: vec!["x".into(), "y".into()],
        body: Box::new(Formula::Exists {
            vars: vec!["z".into()],
            body: Box::new(Formula::Iff(
                Box::new(Formula::Not(Box::new(Formula::Or(vec![
                    leaf.clone(),
                    Formula::Not(Box::new(leaf.clone())),
                ])))),
                Box::new(Formula::Implies(
                    Box::new(Formula::And(vec![leaf.clone(), leaf.clone()])),
                    Box::new(leaf),
                )),
            )),
        }),
    };
    let classes = [
        MorphismClass::Isomorphism,
        MorphismClass::SectionRetraction,
        MorphismClass::WellBehavedLens,
        MorphismClass::LossyLens,
        MorphismClass::Prism,
        MorphismClass::AffineCorrespondence,
        MorphismClass::BridgeView,
    ];
    let relations = [
        CorrespondenceRelation::Equiv,
        CorrespondenceRelation::Subsumes,
        CorrespondenceRelation::SubsumedBy,
        CorrespondenceRelation::Overlaps,
        CorrespondenceRelation::RelatedMatch,
        CorrespondenceRelation::Disjoint,
    ];
    let preservations = [
        PreservationKind::Exact,
        PreservationKind::SoundUnder,
        PreservationKind::CompleteOver,
        PreservationKind::ValidationOnly,
        PreservationKind::InconsistencyPreserving,
        PreservationKind::InconsistencyReflecting,
        PreservationKind::Unsupported,
    ];
    let mut correspondences: Vec<_> = classes
        .into_iter()
        .enumerate()
        .map(|(index, class)| {
            let mut value = cell(&format!("capture-{index}"), vec![left_literal.clone()]);
            value.relation = relations[index % relations.len()];
            value.morphism_class = class;
            value.morphism_kind = if index % 2 == 0 {
                MorphismKind::InstitutionMorphism
            } else {
                MorphismKind::CommitmentShiftingBridge
            };
            value.mnemomorphic = index % 2 == 0;
            value.determinacy = Some(if index % 2 == 0 {
                Determinacy::Crisp
            } else {
                Determinacy::Vague
            });
            value.preservation = Some(preservations[index]);
            value.grounding = index % 2 != 0;
            value
        })
        .collect();
    // Direct fields preserve serialized absence without invoking semantic admission.
    let minimal = &mut correspondences[0];
    minimal.determinacy = None;
    minimal.get_leg = None;
    minimal.put_leg = None;
    minimal.confidence = None;
    minimal.evidence_strength = None;
    minimal.weight = None;
    minimal.probability = None;
    minimal.according_to = None;
    minimal.preservation = None;
    minimal.source_endpoint = None;
    minimal.target_endpoint = None;
    minimal.axis_evidence = AxisEvidence::default();
    minimal.caveats.clear();
    minimal.loss_evidence.clear();
    minimal.recovery_cases.clear();
    correspondences[1].recovery_cases.push(RecoveryCaseIr {
        iri: "urn:correspondence:full-transform".into(),
        transform,
    });
    correspondences[1].law_claims = [
        (
            CorrespondenceLaw::GetPut,
            DischargeVerdict::ObligationDischarged,
            Some(DischargeCondition::DischargeCertifiedFragment),
        ),
        (
            CorrespondenceLaw::PutGet,
            DischargeVerdict::ObligationUnknown,
            None,
        ),
        (
            CorrespondenceLaw::PutPut,
            DischargeVerdict::ObligationViolated,
            Some(DischargeCondition::DischargeFiniteClosure),
        ),
        (
            CorrespondenceLaw::SectionLaw,
            DischargeVerdict::ObligationUnknown,
            Some(DischargeCondition::DischargeSyntacticReachability),
        ),
        (
            CorrespondenceLaw::GetPut,
            DischargeVerdict::ObligationDischarged,
            Some(DischargeCondition::DischargeConservativeExtension),
        ),
        (
            CorrespondenceLaw::PutGet,
            DischargeVerdict::ObligationUnknown,
            Some(DischargeCondition::DischargeBoundedCorpus),
        ),
    ]
    .into_iter()
    .map(|(law, verdict, condition)| LawClaimIr {
        law,
        verdict,
        condition,
    })
    .collect();
    // Collection literals retain explicit stored datatype; term literals use
    // their existing effective-datatype tuple. Neither capture normalizes them.
    correspondences[1].caveats[0].comments.push(RdfLiteral {
        lexical_form: "stored metadata λ\0".into(),
        datatype: Some("urn:correspondence:stored-datatype".into()),
        language: Some("en".into()),
        direction: Some(RdfTextDirection::Ltr),
    });
    let ownership = GroundingSourceIdentity {
        owner: "urn:correspondence:owner".into(),
        path: "synthetic/λ.ttl".into(),
        digest: "synthetic-source-digest".into(),
    };
    let selected = SelectedTemplate {
        profile: "synthetic-profile".into(),
        template_index: usize::MAX,
        subject_variable: "subject".into(),
        object_variable: "object".into(),
        support: LiteralProjectionSupport {
            alias: "urn:correspondence:alias".into(),
            correspondence: "urn:correspondence:cell".into(),
            correspondence_key: "synthetic-cell-key".into(),
            source_property: "urn:correspondence:source-property".into(),
            target_property: "urn:correspondence:target-property".into(),
            owner: ownership.owner.clone(),
            premises: vec![PropertyPremise {
                subject: "urn:correspondence:premise-subject".into(),
                predicate: "urn:correspondence:premise-predicate".into(),
                object: "urn:correspondence:premise-object".into(),
            }],
        },
    };
    let rich = CorrespondenceProgram {
        correspondences,
        compositions: vec![
            CorrespondenceComposition {
                iri: "urn:correspondence:composition-full".into(),
                first: "urn:correspondence:first".into(),
                second: "urn:correspondence:second".into(),
                composite: "urn:correspondence:composite".into(),
                axis_rules: vec![
                    "urn:correspondence:axis-a".into(),
                    "urn:correspondence:axis-b".into(),
                ],
                confidence_independence: Some("urn:correspondence:confidence".into()),
                probability_independence: Some("urn:correspondence:probability".into()),
            },
            CorrespondenceComposition {
                iri: "urn:correspondence:composition-empty".into(),
                first: "urn:correspondence:first".into(),
                second: "urn:correspondence:second".into(),
                composite: "urn:correspondence:composite".into(),
                axis_rules: vec![],
                confidence_independence: None,
                probability_independence: None,
            },
        ],
        preservation: PreservationKind::CompleteOver,
        leg_programs: vec![TransactionProgramIr {
            iri: "urn:correspondence:leg".into(),
            body: LegPath::Seq(vec![
                LegPath::Step("urn:correspondence:step".into()),
                LegPath::Inverse(Box::new(LegPath::Alt(vec![
                    LegPath::Step("urn:correspondence:alternative-a".into()),
                    LegPath::Step("urn:correspondence:alternative-b".into()),
                ]))),
            ]),
        }],
        native_sources: vec![
            NativeCorrespondenceSource {
                alias: selected.support.alias.clone(),
                correspondence: selected.support.correspondence.clone(),
                predicate: "urn:correspondence:alignment".into(),
                sssom_file: "synthetic-mapping-set".into(),
                ownership: Some(ownership),
            },
            NativeCorrespondenceSource {
                alias: "urn:correspondence:unowned-alias".into(),
                correspondence: "urn:correspondence:unowned-cell".into(),
                predicate: "urn:correspondence:other-alignment".into(),
                sssom_file: "synthetic-other-set".into(),
                ownership: None,
            },
        ],
        projection_profiles: vec![ProjectionProfileProgram {
            id: "urn:correspondence:projection-profile".into(),
            payload: ProjectionProfilePayload {
                catalog_id: "urn:correspondence:catalog".into(),
                profile: selected.profile.clone(),
                forward: "synthetic forward λ\0".into(),
                checks: vec![ProjectionInputCheck {
                    id: "urn:correspondence:input-check".into(),
                    binding_iri: "urn:correspondence:binding".into(),
                    binding_key: "synthetic-binding-key".into(),
                    binding_program: "urn:correspondence:binding-program".into(),
                    source_mapping: "urn:correspondence:source-mapping".into(),
                    query: "synthetic source check".into(),
                    selected: selected.clone(),
                }],
            },
        }],
        projection_bindings: vec![ProjectionBindingProgram {
            id: "urn:correspondence:binding-program".into(),
            correspondence: "urn:correspondence:binding".into(),
            binding_key: "synthetic-binding-key".into(),
            source_mapping: "urn:correspondence:source-mapping".into(),
            profile: selected.profile.clone(),
            order: usize::MAX,
            source_pattern: "synthetic source pattern".into(),
            template: "synthetic template".into(),
            selected: vec![selected],
            lossy_drops: vec!["first λ\0".into(), "second".into()],
        }],
    };
    check_reference("rich", &rich);
    let mut reordered = rich.clone();
    reordered.correspondences.reverse();
    reordered.compositions.reverse();
    reordered.native_sources.reverse();
    reordered.projection_bindings[0].lossy_drops.reverse();
    check_reference("rich-reordered", &reordered);
    let mut duplicated = rich.clone();
    duplicated
        .correspondences
        .push(rich.correspondences[1].clone());
    duplicated.leg_programs.push(rich.leg_programs[0].clone());
    duplicated
        .projection_profiles
        .push(rich.projection_profiles[0].clone());
    check_reference("rich-duplicated", &duplicated);
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
