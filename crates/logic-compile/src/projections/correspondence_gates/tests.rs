// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::ir::{
    CorrespondenceLaw, CorrespondenceRelation, DischargeCondition, DischargeVerdict, LawClaimIr,
    LegPath, MorphismKind, PreservationKind, TransactionProgramIr,
};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

/// A deterministic single-step body for a leg IRI — distinct legs get distinct bodies, so
/// the round-trip gate composes real (non-trivially-equal) paths.
fn body_for(leg_iri: &str) -> LegPath {
    LegPath::Step(format!("{leg_iri}#p"))
}

/// Register a [`body_for`] body for every get/put leg IRI the correspondences reference, so
/// the leg registry resolves. A derived put then registers the *inverse* of its get body
/// (see `with_derived_puts`); an AUTHORED put (e.g. the liar) keeps the distinct body here,
/// which the round-trip gate compares against `get.invert()` (and REDs when they differ).
fn legs_for(correspondences: &[Correspondence]) -> Vec<TransactionProgramIr> {
    let mut legs = Vec::new();
    for c in correspondences {
        if let Some(g) = c.get_leg.as_deref() {
            legs.push(TransactionProgramIr {
                iri: g.to_owned(),
                body: body_for(g),
            });
        }
        if let Some(p) = c.put_leg.as_deref() {
            legs.push(TransactionProgramIr {
                iri: p.to_owned(),
                body: body_for(p),
            });
        }
    }
    legs
}

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
    let legs = legs_for(&correspondences);
    CorrespondenceProgram::new(correspondences, Vec::new(), PreservationKind::SoundUnder)
        .with_leg_programs(legs)
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
fn correct_put_iri_but_wrong_body_fails_the_real_round_trip() {
    // THE acceptance test for un-faking the gate. We derive an iso's put leg the lawful way
    // (so its IRI is exactly the content-addressed mint `<get>/put#sha8`), then corrupt ONLY
    // the put leg's BODY in the registry, keeping that correct IRI. The OLD gate compared
    // `put_leg == derived_put_iri(get, c)` — a pure IRI-string match — so it would PASS this.
    // The REAL gate composes the leg BODIES and REDs, because the body is no longer the
    // inverse of get. This test is *impossible to write* against the old gate (legs had no
    // bodies) — which is itself the proof the old gate verified nothing.
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
    // Lawful derived put passes (control).
    assert_eq!(
        evaluate_gates(&prog, &[]).per_correspondence[0].round_trip,
        GateVerdict::Pass
    );

    // The derived put IRI IS the content-addressed mint — the OLD string compare would pass.
    let put_iri = prog.correspondences[0]
        .put_leg
        .clone()
        .expect("iso derives a put leg");
    assert_eq!(
        put_iri,
        crate::projections::put_derivation::derived_put_iri(&get, &prog.correspondences[0]),
        "the put IRI is the content-addressed mint (so the IRI tautology would pass)"
    );

    // Corrupt ONLY the body behind that (correct) IRI.
    let mut legs = prog.leg_programs.clone();
    for leg in &mut legs {
        if leg.iri == put_iri {
            leg.body = LegPath::Inverse(Box::new(LegPath::Step(format!("{GMEOW}ex/WRONG"))));
        }
    }
    let corrupted = CorrespondenceProgram::new(
        prog.correspondences.clone(),
        prog.caveats.clone(),
        prog.preservation,
    )
    .with_leg_programs(legs);

    let r = &evaluate_gates(&corrupted, &[]).per_correspondence[0];
    assert!(
        r.round_trip.is_red(),
        "a wrong put BODY (correct IRI) must RED the round-trip: {:?}",
        r.round_trip
    );
    assert!(
        r.mnemomorphism.is_red(),
        "a wrong put BODY must fail witness recovery: {:?}",
        r.mnemomorphism
    );
    // The put IRI never changed — proving it is the BODY, not the IRI, that decides the law.
    assert_eq!(
        corrupted.correspondences[0].put_leg.as_deref(),
        Some(put_iri.as_str())
    );
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
