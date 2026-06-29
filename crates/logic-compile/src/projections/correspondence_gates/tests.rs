// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::ir::{
    CorrespondenceLaw, CorrespondenceRelation, DischargeCondition, DischargeVerdict, LawClaimIr,
    MorphismKind, PreservationKind,
};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

#[allow(clippy::too_many_arguments)]
fn corr(
    iri: &str,
    relation: CorrespondenceRelation,
    class: MorphismClass,
    kind: MorphismKind,
    mnemomorphic: bool,
    get_leg: Option<&str>,
    put_leg: Option<String>,
    law_claims: Vec<LawClaimIr>,
) -> Correspondence {
    Correspondence::new(
        iri.to_owned(),
        relation,
        class,
        kind,
        mnemomorphic,
        None,
        get_leg.map(str::to_owned),
        put_leg,
        law_claims,
        None,
        None,
        None,
        None,
        None,
    )
    .expect("well-formed correspondence")
}

fn program(correspondences: Vec<Correspondence>) -> CorrespondenceProgram {
    CorrespondenceProgram::new(correspondences, Vec::new(), PreservationKind::SoundUnder)
}

/// Build a derived program (puts minted) the way compile_program will.
fn derived(correspondences: Vec<Correspondence>) -> CorrespondenceProgram {
    program(correspondences)
        .with_derived_puts()
        .expect("derive puts")
        .0
}

#[test]
fn iso_with_derived_put_passes_round_trip_and_law() {
    let get = format!("{GMEOW}ex/getIso");
    let c = corr(
        &format!("{GMEOW}ex/iso"),
        CorrespondenceRelation::Equiv,
        MorphismClass::Isomorphism,
        MorphismKind::InstitutionMorphism,
        true,
        Some(&get),
        None,
        Vec::new(),
    );
    let prog = derived(vec![c]);
    let report = evaluate_gates(&prog, &[]);
    let r = &report.per_correspondence[0];
    assert_eq!(r.round_trip, GateVerdict::Pass, "{:?}", r.round_trip);
    assert_eq!(r.law, GateVerdict::Pass);
    assert_eq!(r.overclaim, GateVerdict::Pass);
    assert_eq!(r.mnemomorphism, GateVerdict::Pass);
    assert!(assert_gates(&report).is_ok());
}

#[test]
fn section_retraction_passes_round_trip() {
    let get = format!("{GMEOW}ex/getSection");
    let c = corr(
        &format!("{GMEOW}ex/section"),
        CorrespondenceRelation::Subsumes,
        MorphismClass::SectionRetraction,
        MorphismKind::InstitutionMorphism,
        true,
        Some(&get),
        None,
        Vec::new(),
    );
    let prog = derived(vec![c]);
    let r = &evaluate_gates(&prog, &[]).per_correspondence[0];
    assert_eq!(r.round_trip, GateVerdict::Pass);
    assert_eq!(r.mnemomorphism, GateVerdict::Pass);
}

#[test]
fn bridge_view_declaring_equiv_is_overclaim_red() {
    let c = corr(
        &format!("{GMEOW}ex/bridge"),
        CorrespondenceRelation::Equiv,
        MorphismClass::BridgeView,
        MorphismKind::CommitmentShiftingBridge,
        false,
        Some(&format!("{GMEOW}ex/getBridge")),
        None,
        Vec::new(),
    );
    let report = evaluate_gates(&program(vec![c]), &[]);
    assert!(report.per_correspondence[0].overclaim.is_red());
    let err = assert_gates(&report).unwrap_err();
    assert!(err.0.contains("Overclaim"), "{}", err.0);
}

#[test]
fn caveated_overlap_claiming_equiv_is_overclaim_red() {
    // relation=Equiv on an affine (co-projection) rung: the honest relation is Overlaps,
    // so claiming equivalence overclaims the lowered legs.
    let c = corr(
        &format!("{GMEOW}ex/affineEquiv"),
        CorrespondenceRelation::Equiv,
        MorphismClass::AffineCorrespondence,
        MorphismKind::InstitutionMorphism,
        false,
        Some(&format!("{GMEOW}ex/getAffine")),
        None,
        Vec::new(),
    );
    let r = &evaluate_gates(&program(vec![c]), &[]).per_correspondence[0];
    assert!(r.overclaim.is_red(), "{:?}", r.overclaim);
}

#[test]
fn discharged_section_law_without_a_correct_put_is_law_red() {
    // A hand-authored SectionLaw ObligationDischarged whose put leg is NOT the derived
    // inverse: the Law gate refuses it (must degrade to ObligationUnknown).
    let c = corr(
        &format!("{GMEOW}ex/liar"),
        CorrespondenceRelation::Subsumes,
        MorphismClass::SectionRetraction,
        MorphismKind::InstitutionMorphism,
        true,
        Some(&format!("{GMEOW}ex/getLiar")),
        Some(format!("{GMEOW}ex/wrongPut")),
        vec![LawClaimIr {
            law: CorrespondenceLaw::SectionLaw,
            verdict: DischargeVerdict::ObligationDischarged,
            condition: Some(DischargeCondition::DischargeFiniteClosure),
        }],
    );
    let report = evaluate_gates(&program(vec![c]), &[]);
    let r = &report.per_correspondence[0];
    assert!(r.law.is_red(), "law: {:?}", r.law);
    assert!(r.round_trip.is_red(), "round_trip: {:?}", r.round_trip);
    assert!(assert_gates(&report).is_err());
}

#[test]
fn mnemomorphic_on_non_injective_rung_is_mnemomorphism_red() {
    let c = corr(
        &format!("{GMEOW}ex/badWitness"),
        CorrespondenceRelation::Overlaps,
        MorphismClass::LossyLens,
        MorphismKind::InstitutionMorphism,
        true,
        Some(&format!("{GMEOW}ex/getLossy")),
        None,
        Vec::new(),
    );
    let r = &evaluate_gates(&program(vec![c]), &[]).per_correspondence[0];
    assert!(r.mnemomorphism.is_red(), "{:?}", r.mnemomorphism);
}

#[test]
fn composition_weakens_passes() {
    let iso = corr(
        &format!("{GMEOW}ex/cIso"),
        CorrespondenceRelation::Equiv,
        MorphismClass::Isomorphism,
        MorphismKind::InstitutionMorphism,
        true,
        Some(&format!("{GMEOW}ex/getCIso")),
        None,
        Vec::new(),
    );
    let lossy = corr(
        &format!("{GMEOW}ex/cLossy"),
        CorrespondenceRelation::Overlaps,
        MorphismClass::LossyLens,
        MorphismKind::InstitutionMorphism,
        false,
        Some(&format!("{GMEOW}ex/getCLossy")),
        None,
        vec![LawClaimIr {
            law: CorrespondenceLaw::PutGet,
            verdict: DischargeVerdict::ObligationUnknown,
            condition: None,
        }],
    );
    let composite = corr(
        &format!("{GMEOW}ex/cComposite"),
        CorrespondenceRelation::Overlaps,
        MorphismClass::LossyLens,
        MorphismKind::InstitutionMorphism,
        false,
        Some(&format!("{GMEOW}ex/getCComposite")),
        None,
        vec![LawClaimIr {
            law: CorrespondenceLaw::PutGet,
            verdict: DischargeVerdict::ObligationUnknown,
            condition: None,
        }],
    );
    let prog = derived(vec![iso, lossy, composite]);
    let comps = vec![(
        format!("{GMEOW}ex/cIso"),
        format!("{GMEOW}ex/cLossy"),
        Some(format!("{GMEOW}ex/cComposite")),
    )];
    let report = evaluate_gates(&prog, &comps);
    assert_eq!(report.per_composition.len(), 1);
    assert_eq!(report.per_composition[0].composed_class, "LossyLens");
    assert_eq!(report.per_composition[0].composition, GateVerdict::Pass);
}

#[test]
fn composition_strengthening_is_red() {
    // lossy ∘ affine → join = AffineCorrespondence (weaker); a WellBehavedLens composite
    // is STRONGER than the join, so the gate REDs.
    let lossy = corr(
        &format!("{GMEOW}ex/sLossy"),
        CorrespondenceRelation::Overlaps,
        MorphismClass::LossyLens,
        MorphismKind::InstitutionMorphism,
        false,
        Some(&format!("{GMEOW}ex/getSLossy")),
        None,
        vec![LawClaimIr {
            law: CorrespondenceLaw::PutGet,
            verdict: DischargeVerdict::ObligationUnknown,
            condition: None,
        }],
    );
    let affine = corr(
        &format!("{GMEOW}ex/sAffine"),
        CorrespondenceRelation::Overlaps,
        MorphismClass::AffineCorrespondence,
        MorphismKind::InstitutionMorphism,
        false,
        Some(&format!("{GMEOW}ex/getSAffine")),
        None,
        vec![LawClaimIr {
            law: CorrespondenceLaw::PutGet,
            verdict: DischargeVerdict::ObligationUnknown,
            condition: None,
        }],
    );
    let composite = corr(
        &format!("{GMEOW}ex/sComposite"),
        CorrespondenceRelation::Equiv,
        MorphismClass::WellBehavedLens,
        MorphismKind::InstitutionMorphism,
        true,
        Some(&format!("{GMEOW}ex/getSComposite")),
        None,
        Vec::new(),
    );
    let prog = derived(vec![lossy, affine, composite]);
    let comps = vec![(
        format!("{GMEOW}ex/sLossy"),
        format!("{GMEOW}ex/sAffine"),
        Some(format!("{GMEOW}ex/sComposite")),
    )];
    let report = evaluate_gates(&prog, &comps);
    assert_eq!(
        report.per_composition[0].composed_class,
        "AffineCorrespondence"
    );
    assert!(report.per_composition[0].composition.is_red());
    assert!(assert_gates(&report).is_err());
}

#[test]
fn amnesic_mint_with_claim_is_not_lawful_uplift() {
    // mnemomorphic=false, non-injective, co-authored PutGet/Unknown → minted-with-claim:
    // Round-trip NotApplicable, Mnemomorphism NotApplicable, so NOT counted lawful.
    let c = corr(
        &format!("{GMEOW}ex/amnesic"),
        CorrespondenceRelation::Overlaps,
        MorphismClass::LossyLens,
        MorphismKind::InstitutionMorphism,
        false,
        Some(&format!("{GMEOW}ex/getAmnesic")),
        None,
        vec![LawClaimIr {
            law: CorrespondenceLaw::PutGet,
            verdict: DischargeVerdict::ObligationUnknown,
            condition: None,
        }],
    );
    let prog = derived(vec![c]);
    let report = evaluate_gates(&prog, &[]);
    let r = &report.per_correspondence[0];
    assert!(matches!(r.round_trip, GateVerdict::NotApplicable { .. }));
    assert!(matches!(r.mnemomorphism, GateVerdict::NotApplicable { .. }));
    assert!(
        assert_gates(&report).is_ok(),
        "minted-with-claim is honest, not RED"
    );
    let lift = liftability(&report);
    assert_eq!(
        lift,
        LiftabilityLedger {
            lawful: 0,
            total: 1
        }
    );
}

#[test]
fn liftability_counts_only_recoverable_cells() {
    let iso = corr(
        &format!("{GMEOW}ex/lIso"),
        CorrespondenceRelation::Equiv,
        MorphismClass::Isomorphism,
        MorphismKind::InstitutionMorphism,
        true,
        Some(&format!("{GMEOW}ex/getLIso")),
        None,
        Vec::new(),
    );
    let amnesic = corr(
        &format!("{GMEOW}ex/lAmnesic"),
        CorrespondenceRelation::Overlaps,
        MorphismClass::LossyLens,
        MorphismKind::InstitutionMorphism,
        false,
        Some(&format!("{GMEOW}ex/getLAmnesic")),
        None,
        vec![LawClaimIr {
            law: CorrespondenceLaw::PutGet,
            verdict: DischargeVerdict::ObligationUnknown,
            condition: None,
        }],
    );
    let prog = derived(vec![iso, amnesic]);
    let lift = liftability(&evaluate_gates(&prog, &[]));
    assert_eq!(
        lift,
        LiftabilityLedger {
            lawful: 1,
            total: 2
        }
    );
}

/// Canonical JSON serialization is stable (golden-shape check).
#[test]
fn gate_report_serializes_to_tagged_json() {
    let get = format!("{GMEOW}ex/getJson");
    let c = corr(
        &format!("{GMEOW}ex/json"),
        CorrespondenceRelation::Equiv,
        MorphismClass::Isomorphism,
        MorphismKind::InstitutionMorphism,
        true,
        Some(&get),
        None,
        Vec::new(),
    );
    let report = evaluate_gates(&derived(vec![c]), &[]);
    let json = serde_json::to_string(&report).expect("serialize");
    assert!(json.contains("\"status\":\"pass\""), "{json}");
    assert!(json.contains("\"per_correspondence\""), "{json}");
    assert!(json.contains("\"per_composition\""), "{json}");
}
