// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::frontend::{OwnerDisposition, OwnerFamily, PreparedLogicSource};
use crate::ir::{CorrespondenceComposition, LogicProgram};
use crate::projections::correspondence_gates::{
    ExecutedCorrespondenceLaws, assert_gates, evaluate_gates,
};

const PREFIX: &str =
    "@prefix ex: <https://example.org/> . @prefix logic: <https://blackcatinformatics.ca/logic/> .";
const DECLARATION: &str = "ex:chain a logic:CorrespondenceComposition ; logic:compositionFirst ex:first ; logic:compositionSecond ex:second ; logic:compositionResult ex:result ; logic:compositionAxisRule ex:weightRule ; logic:confidenceIndependenceEvidence ex:review ; logic:probabilityIndependenceEvidence ex:modelReview .";

fn dataset(declaration: &str) -> std::sync::Arc<purrdf::RdfDataset> {
    let mut source = format!("{PREFIX}\n{declaration}\n");
    for (name, from, to) in [
        ("first", "A", "B"),
        ("second", "B", "C"),
        ("result", "A", "C"),
    ] {
        source.push_str(&format!("ex:{name} a logic:Correspondence ; logic:correspondenceRelation logic:Subsumes ; logic:morphismClass logic:LossyLens ; logic:morphismKind logic:InstitutionMorphism ; logic:sourceEndpoint ex:{from} ; logic:targetEndpoint ex:{to} .\n"));
    }
    purrdf::parse_dataset(source.as_bytes(), "text/turtle", None).unwrap()
}

fn typed_program() -> CorrespondenceProgram {
    let source = dataset(DECLARATION);
    let theory = PreparedLogicSource::new(&source)
        .unwrap()
        .into_compiled(None)
        .unwrap();
    assert!(
        theory.diagnostics().is_empty(),
        "{:?}",
        theory.diagnostics()
    );
    CorrespondenceProgram::new(
        theory.program().correspondences.clone(),
        PreservationKind::SoundUnder,
    )
    .with_compositions(theory.program().correspondence_compositions.clone())
}

fn report(
    program: &CorrespondenceProgram,
) -> super::super::correspondence_gates::CorrespondenceGateReport {
    let verdicts = program
        .correspondences
        .iter()
        .map(|c| {
            (
                c.iri.clone(),
                ExecutedCorrespondenceLaws::section_only(
                    crate::ir::DischargeVerdict::ObligationUnknown,
                ),
            )
        })
        .collect();
    evaluate_gates(program, &[], &verdicts)
}

#[test]
fn composition_source_ownership_and_all_common_logic_round_trips() {
    let source = dataset(DECLARATION);
    let theory = PreparedLogicSource::new(&source)
        .unwrap()
        .into_compiled(None)
        .unwrap();
    let owner = theory
        .owner_lowerings()
        .iter()
        .find(|owner| owner.family == OwnerFamily::CorrespondenceComposition)
        .unwrap();
    assert_eq!(owner.disposition, OwnerDisposition::Emitted { index: 0 });
    assert_eq!(
        theory.source().dataset().resolve(owner.source.term),
        purrdf::TermRef::Iri("https://example.org/chain")
    );
    let expected = &theory.program().correspondence_compositions;
    // Generic source axioms cannot conceal a lost typed declaration here.
    let program = LogicProgram::new(vec![], vec![], vec![], None)
        .with_correspondences(theory.program().correspondences.clone())
        .unwrap()
        .with_correspondence_compositions(expected.clone());
    let cp = typed_program();
    let projected = project_correspondence(&cp);
    let backing =
        purrdf::parse_dataset(projected.as_bytes(), "application/n-triples", None).unwrap();
    assert_eq!(parse_correspondence(&backing).unwrap(), cp);
    let cached: LogicProgram =
        serde_json::from_slice(&serde_json::to_vec(&program).unwrap()).unwrap();
    assert_eq!(cached, program);
    for (name, (restored, _)) in [
        (
            "CLIF",
            crate::clif::parse_clif_str(
                &crate::clif::project_clif(&program).unwrap().content,
                None,
            )
            .unwrap(),
        ),
        (
            "CGIF",
            crate::cgif::parse_cgif_str(
                &crate::cgif::project_cgif(&program).unwrap().content,
                None,
            )
            .unwrap(),
        ),
        (
            "XCL",
            crate::xcl::parse_xcl_str(&crate::xcl::project_xcl(&program).unwrap().content, None)
                .unwrap(),
        ),
    ] {
        assert_eq!(&restored.correspondence_compositions, expected, "{name}");
    }
}

#[test]
fn malformed_composition_keeps_its_rejected_source_owner() {
    for declaration in [
        "ex:chain a logic:CorrespondenceComposition .",
        "ex:chain logic:compositionFirst ex:first ; logic:compositionSecond ex:second ; logic:compositionResult ex:result .",
        "ex:chain a logic:CorrespondenceComposition ; logic:compositionFirst ex:first, ex:second ; logic:compositionSecond ex:second ; logic:compositionResult ex:result .",
        "ex:chain a logic:CorrespondenceComposition ; logic:compositionFirst 7 ; logic:compositionSecond ex:second ; logic:compositionResult ex:result .",
        "[] a logic:CorrespondenceComposition ; logic:compositionFirst ex:first ; logic:compositionSecond ex:second ; logic:compositionResult ex:result .",
        "logic:correspondence-program logic:hasComposition ex:missing .",
        "logic:correspondence-program logic:hasComposition 7 .",
    ] {
        let source = dataset(declaration);
        let theory = PreparedLogicSource::new(&source)
            .unwrap()
            .into_compiled(None)
            .unwrap();
        assert!(
            theory.program().correspondence_compositions.is_empty(),
            "{declaration}"
        );
        assert_eq!(theory.program().correspondences.len(), 3);
        let owner = theory
            .owner_lowerings()
            .iter()
            .find(|owner| owner.family == OwnerFamily::CorrespondenceComposition)
            .unwrap();
        assert_eq!(
            owner.disposition,
            OwnerDisposition::Rejected,
            "{declaration}"
        );
        assert!(
            owner
                .diagnostics
                .iter()
                .any(|&i| theory.diagnostics()[i].code == "MALFORMED_CORRESPONDENCE_COMPOSITION")
        );
    }
}

#[test]
fn authored_composition_cannot_be_erased_by_empty_supplemental_selection() {
    let original = typed_program();
    let (derived, _) = original.clone().with_derived_puts().unwrap();
    assert_eq!(derived.compositions, original.compositions);
    let gates = report(&derived);
    assert_gates(&gates).unwrap();
    assert_eq!(gates.per_composition.len(), 1);
    assert_eq!(
        gates.per_composition[0].declaration.as_deref(),
        Some("https://example.org/chain")
    );
    assert_eq!(
        gates.per_composition[0].result.as_deref(),
        Some("https://example.org/result")
    );
    let mut missing = derived.clone();
    missing.correspondences.clear();
    assert!(assert_gates(&report(&missing)).is_err());
    let mut duplicated = derived.clone();
    duplicated
        .compositions
        .push(derived.compositions[0].clone());
    assert!(assert_gates(&report(&duplicated)).is_err());
}

#[test]
fn authored_composition_checks_endpoints_and_unspecified_standpoint() {
    let original = typed_program();
    for field in ["middle", "outer", "missing", "standpoint"] {
        let mut changed = original.clone();
        let first = changed
            .correspondences
            .iter_mut()
            .find(|c| c.iri.ends_with("first"))
            .unwrap();
        match field {
            "middle" => first.target_endpoint = Some("https://example.org/other".into()),
            "outer" => first.source_endpoint = Some("https://example.org/other".into()),
            "missing" => first.source_endpoint = None,
            "standpoint" => first.according_to = Some("https://example.org/observer".into()),
            _ => unreachable!(),
        }
        assert!(assert_gates(&report(&changed)).is_err(), "{field}");
    }
    let mut scoped = original;
    for member in &mut scoped.correspondences {
        member.according_to = Some("https://example.org/observer".into());
    }
    assert_gates(&report(&scoped)).unwrap();
}

#[test]
fn composition_keys_bind_order_result_and_source_identity() {
    let original = typed_program();
    for field in 0..4 {
        let mut changed = original.clone();
        let composition = &mut changed.compositions[0];
        let target = match field {
            0 => &mut composition.iri,
            1 => &mut composition.first,
            2 => &mut composition.second,
            _ => &mut composition.composite,
        };
        target.push_str("/changed");
        assert_ne!(changed.content_key(), original.content_key());
        let base = LogicProgram::new(vec![], vec![], vec![], None);
        assert_ne!(
            base.clone()
                .with_correspondence_compositions(changed.compositions)
                .canonical_key(),
            base.with_correspondence_compositions(original.compositions.clone())
                .canonical_key()
        );
    }
    let second = CorrespondenceComposition::new(
        "https://example.org/z".into(),
        original.compositions[0].first.clone(),
        original.compositions[0].second.clone(),
        original.compositions[0].composite.clone(),
    )
    .unwrap();
    assert_eq!(
        original
            .clone()
            .with_compositions(vec![second.clone(), original.compositions[0].clone()])
            .content_key(),
        original
            .clone()
            .with_compositions(vec![original.compositions[0].clone(), second])
            .content_key()
    );
}

#[test]
fn composition_cannot_select_one_of_two_definitions_for_a_member_identity() {
    for member in ["first", "second", "result"] {
        for reverse in [false, true] {
            let mut program = typed_program();
            let mut conflicting = program
                .correspondences
                .iter()
                .find(|c| c.iri.ends_with(member))
                .unwrap()
                .clone();
            conflicting.according_to = Some("https://example.org/otherObserver".into());
            program.correspondences.push(conflicting);
            if reverse {
                program.correspondences.reverse();
            }
            let gates = report(&program);
            let error = assert_gates(&gates).unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("ambiguous correspondence identity"),
                "{error}"
            );
        }
    }
}
