// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! GMEOW cell-owned loss evidence; every input below is a tiny synthetic program.

use super::*;
use crate::{frontend::PreparedLogicSource, ir::LogicProgram, loss_ledger::LossLedger};

const OWNER: &str = "https://example.org/cell";

fn source(extra: &str) -> std::sync::Arc<purrdf::RdfDataset> {
    purrdf::parse_dataset(
        format!(
            r#"
        @prefix ex: <https://example.org/> .
        @prefix logic: <https://blackcatinformatics.ca/logic/> .
        @prefix gm: <https://blackcatinformatics.ca/gmeow/> .
        @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
        logic:correspondence-program logic:hasPreservation logic:SoundUnderApproximation ;
            logic:hasCorrespondence ex:cell .
        ex:cell a logic:Correspondence ; logic:correspondenceRelation logic:Subsumes ;
            logic:morphismClass logic:LossyLens ; logic:morphismKind logic:InstitutionMorphism .
        {extra}
    "#
        )
        .as_bytes(),
        "text/turtle",
        None,
    )
    .unwrap()
}

fn cell(extra: &str) -> Correspondence {
    parse_correspondence(&source(extra))
        .unwrap()
        .correspondences
        .remove(0)
}

#[test]
fn compiler_records_only_each_cells_actual_losses_and_context() {
    let first = cell(
        r#"
        ex:cell logic:preservationKind logic:SoundUnderApproximation ;
            logic:lossyDrop "The view omits observation timestamps."@en ;
            gm:accordingTo ex:observer ; logic:evidenceSource ex:source .
    "#,
    );
    let mut second = cell(
        r#"
        ex:cell logic:preservationKind logic:ValidationOnly ;
            logic:lossyDrop "The validator does not compute the closure."@en .
    "#,
    );
    second.iri = "https://example.org/validator".to_owned();
    let program = LogicProgram::new(vec![], vec![], vec![], None)
        .with_correspondences(vec![first.clone(), second.clone()])
        .unwrap();
    let artifacts = crate::projections::compile_program(&program, |_| Default::default()).unwrap();
    let rows: Vec<_> = artifacts
        .logic_projections
        .iter()
        .filter(|row| row.target.starts_with("correspondence:"))
        .collect();
    assert_eq!(rows.len(), 2);
    for correspondence in [&first, &second] {
        let row = rows
            .iter()
            .find(|row| {
                row.target
                    .starts_with(&format!("correspondence:{}:", correspondence.iri))
            })
            .unwrap();
        assert_eq!(Some(row.preservation), correspondence.preservation);
        let note = &correspondence.loss_evidence[0].lexical_form;
        assert_eq!(
            artifacts.loss.projection_drops_for(&row.target),
            vec![format!("actual: {note}")]
        );
        assert_eq!(
            artifacts.loss.term_source_drops(&row.target),
            vec![(note.clone(), correspondence.iri.clone())]
        );
        assert!(!row.complexity.contains("graph-iso"));
        assert!(
            !artifacts
                .loss
                .projection_drops_for(&row.target)
                .iter()
                .any(|note| note.contains("Szs")
                    || note.contains("many-to-one")
                    || note.contains("correspondence:"))
        );
    }
    let witnesses = artifacts.loss.to_nodes();
    let witness = witnesses
        .iter()
        .find(|node| {
            node.observations
                .iter()
                .any(|observation| observation.message == first.loss_evidence[0].lexical_form)
        })
        .unwrap();
    assert!(
        witness
            .tags
            .contains(&"according-to:https://example.org/observer".to_owned())
    );
    assert!(
        witness
            .tags
            .contains(&"evidence-source:https://example.org/source".to_owned())
    );
}

#[test]
fn preservation_judgments_never_manufacture_loss_or_execution_claims() {
    for preservation in [
        PreservationKind::Exact,
        PreservationKind::SoundUnder,
        PreservationKind::CompleteOver,
        PreservationKind::ValidationOnly,
        PreservationKind::InconsistencyPreserving,
        PreservationKind::InconsistencyReflecting,
    ] {
        let correspondence = cell(&format!(
            "ex:cell logic:preservationKind <{}> .",
            preservation.iri()
        ));
        let mut loss = LossLedger::new();
        let row =
            crate::projections::correspondence_preservation_result(&correspondence, &mut loss)
                .unwrap()
                .unwrap();
        assert_eq!(row.preservation, preservation);
        assert!(loss.projection_drops_for(&row.target).is_empty());
        assert!(loss.to_nodes().is_empty());
    }
}

#[test]
fn invalid_loss_claims_fail_source_and_native_projection_admission() {
    for extra in [
        "ex:cell logic:preservationKind logic:ExactPreservation ; logic:lossyDrop \"lost content\" .",
        "ex:cell logic:preservationKind logic:Unsupported .",
        "ex:cell logic:lossyDrop \"lost content\" .",
        "ex:cell logic:preservationKind logic:SoundUnderApproximation ; logic:lossyDrop ex:notLiteral .",
        "ex:cell logic:preservationKind logic:SoundUnderApproximation ; logic:lossyDrop \"  \" .",
        "logic:correspondence-program logic:lossyDrop \"unowned content\" .",
    ] {
        assert!(parse_correspondence(&source(extra)).is_err(), "{extra}");
    }
    let valid = cell(
        "ex:cell logic:preservationKind logic:SoundUnderApproximation ; logic:lossyDrop \"lost content\" .",
    );
    for (preservation, losses) in [
        (Some(PreservationKind::Exact), valid.loss_evidence.clone()),
        (Some(PreservationKind::Unsupported), vec![]),
        (None, valid.loss_evidence.clone()),
    ] {
        let mut invalid = valid.clone();
        invalid.preservation = preservation;
        invalid.loss_evidence = losses;
        let mut loss = LossLedger::new();
        assert!(
            crate::projections::correspondence_preservation_result(&invalid, &mut loss).is_err()
        );
        assert!(
            loss.to_nodes().is_empty(),
            "identity-only notes must not authorize the claim"
        );
        assert!(
            project_correspondence_dataset(&CorrespondenceProgram::new(
                vec![invalid],
                PreservationKind::SoundUnder
            ))
            .is_err()
        );
    }
}

#[test]
fn source_loss_literals_survive_native_common_logic_and_cache_roundtrips() {
    let dataset = source(
        r#"
        ex:cell logic:preservationKind logic:SoundUnderApproximation ;
            logic:lossyDrop "same lexical"@en, "same lexical"@fr,
                "تحذير"@ar--rtl, "0001"^^ex:lossCode ;
            gm:accordingTo ex:observer ; logic:evidenceSource ex:source .
    "#,
    );
    let prepared = PreparedLogicSource::new(&dataset).unwrap();
    let compiled = prepared.compile_with_sources(None).unwrap();
    let expected = parse_correspondence(&dataset).unwrap();
    assert_eq!(compiled.program().correspondences, expected.correspondences);
    assert_eq!(expected.correspondences[0].loss_evidence.len(), 4);
    let native = project_correspondence_dataset(&expected).unwrap();
    assert_eq!(parse_correspondence(&native).unwrap(), expected);
    let cached: CorrespondenceProgram =
        serde_json::from_slice(&serde_json::to_vec(&expected).unwrap()).unwrap();
    assert_eq!(cached, expected);
    assert_eq!(cached.content_key(), expected.content_key());
    let program = LogicProgram::new(vec![], vec![], vec![], None)
        .with_correspondences(expected.correspondences.clone())
        .unwrap();
    for (name, restored) in [
        (
            "CLIF",
            crate::clif::parse_clif_str(
                &crate::clif::project_clif(&program).unwrap().content,
                None,
            )
            .unwrap()
            .0,
        ),
        (
            "CGIF",
            crate::cgif::parse_cgif_str(
                &crate::cgif::project_cgif(&program).unwrap().content,
                None,
            )
            .unwrap()
            .0,
        ),
        (
            "XCL",
            crate::xcl::parse_xcl_str(&crate::xcl::project_xcl(&program).unwrap().content, None)
                .unwrap()
                .0,
        ),
    ] {
        assert_eq!(restored.correspondences, expected.correspondences, "{name}");
    }
}

#[test]
fn loss_identity_binds_all_literal_components_and_owner_context() {
    let mut keys = std::collections::BTreeSet::new();
    let mut targets = std::collections::BTreeSet::new();
    for suffix in [
        "",
        "@en",
        "@fr",
        "@ar--rtl",
        "@ar--ltr",
        "^^<https://example.org/lossKind>",
    ] {
        let correspondence = cell(&format!(
            "ex:cell logic:preservationKind logic:SoundUnderApproximation ; logic:lossyDrop \"same lexical\"{suffix} ."
        ));
        assert!(keys.insert(correspondence.content_key()), "{suffix}");
        let row = crate::projections::correspondence_preservation_result(
            &correspondence,
            &mut LossLedger::new(),
        )
        .unwrap()
        .unwrap();
        assert!(targets.insert(row.target), "{suffix}");
    }
    let baseline = cell(
        "ex:cell logic:preservationKind logic:SoundUnderApproximation ; logic:lossyDrop \"a\", \"b\" .",
    );
    let reordered = cell(
        "ex:cell logic:preservationKind logic:SoundUnderApproximation ; logic:lossyDrop \"b\", \"a\" .",
    );
    assert_eq!(baseline, reordered);
    for according_to in [
        None,
        Some("https://example.org/a"),
        Some("https://example.org/b"),
    ] {
        let mut changed = baseline.clone();
        changed.according_to = according_to.map(str::to_owned);
        assert!(keys.insert(changed.content_key()));
    }
    let mut changed = baseline.clone();
    changed.axis_evidence =
        crate::ir::AxisEvidence::new(vec!["https://example.org/evidence".to_owned()], None, None)
            .unwrap();
    assert_ne!(changed.content_key(), baseline.content_key());
}

#[test]
fn malformed_loss_literal_metadata_cannot_enter_through_cache() {
    let valid = cell(
        "ex:cell logic:preservationKind logic:SoundUnderApproximation ; logic:lossyDrop \"تحذير\"@ar--rtl .",
    );
    let wire = serde_json::to_value(&valid).unwrap();
    for (slot, value) in [
        (3, serde_json::json!("unknown")),
        (2, serde_json::Value::Null),
        (1, serde_json::json!(XSD_BOOLEAN)),
    ] {
        let mut corrupt = wire.clone();
        corrupt["loss_evidence"][0][slot] = value;
        assert!(serde_json::from_value::<Correspondence>(corrupt).is_err());
    }
    let mut invalid = valid.loss_evidence.clone();
    invalid[0].language = None;
    assert!(valid.with_loss_evidence(invalid).is_err());
}

#[test]
fn malformed_loss_owner_is_rejected_without_removing_healthy_owners() {
    let dataset = source(
        r#"
        ex:cell logic:preservationKind logic:SoundUnderApproximation ; logic:lossyDrop ex:notLiteral .
        ex:healthy a logic:Correspondence ; logic:correspondenceRelation logic:Subsumes ;
            logic:morphismClass logic:LossyLens ; logic:morphismKind logic:InstitutionMorphism .
    "#,
    );
    let prepared = PreparedLogicSource::new(&dataset).unwrap();
    let compiled = prepared.compile_with_sources(None).unwrap();
    assert_eq!(compiled.program().correspondences.len(), 1);
    assert!(
        compiled.program().correspondences[0]
            .iri
            .ends_with("healthy")
    );
    assert!(compiled.owner_lowerings().iter().any(|owner| owner.family
        == crate::frontend::OwnerFamily::Correspondence
        && owner.disposition == crate::frontend::OwnerDisposition::Rejected
        && !owner.diagnostics.is_empty()));
    let (_, errors) = extract_correspondences(&dataset);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].0, OWNER);
}

#[test]
fn program_preservation_does_not_invent_an_affine_overlap() {
    let program = CorrespondenceProgram::new(vec![], PreservationKind::SoundUnder);
    let dataset = project_correspondence_dataset(&program).unwrap();
    assert!(dataset.term_id_by_iri(&p_lossy_drop()).is_none());
    assert_eq!(parse_correspondence(&dataset).unwrap(), program);
    let affine = affine_triangle_worked_example();
    let projected = project_correspondence_dataset(&affine).unwrap();
    assert_eq!(parse_correspondence(&projected).unwrap(), affine);
    let view = CorrespondenceView::from_dataset(&projected);
    assert_eq!(
        view.objects(&affine.correspondences[0].iri, &p_lossy_drop())
            .count(),
        1
    );
    assert_eq!(view.objects(&program_iri(), &p_lossy_drop()).count(), 0);
}
