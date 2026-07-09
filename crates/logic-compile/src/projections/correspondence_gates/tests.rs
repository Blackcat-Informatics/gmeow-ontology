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
        None,
    )
    .expect("well-formed correspondence")
}

/// A correspondence carrying an explicit per-correspondence preservation judgment — for the
/// preservation-consistency gate (the only gate that reads `logic:preservationKind`).
#[allow(clippy::too_many_arguments)]
fn corr_pres(
    iri: &str,
    relation: CorrespondenceRelation,
    class: MorphismClass,
    kind: MorphismKind,
    preservation: PreservationKind,
) -> Correspondence {
    Correspondence::new(
        iri.to_owned(),
        relation,
        class,
        kind,
        false,
        None,
        Some(format!("{iri}#get")),
        None,
        Vec::new(),
        None,
        None,
        None,
        None,
        None,
        Some(preservation),
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

/// Assign the SAME executed lens-law verdict to every correspondence in `prog` — the
/// injected verdict map the (execution-free) gates read. The real executor
/// (`gmeow_logic::correspondence_exec::program_verdicts`) is tested against the engine; here
/// we test the GATE LOGIC given a verdict, so we inject it directly.
fn verdicts_all(prog: &CorrespondenceProgram, v: DischargeVerdict) -> CorrespondenceVerdicts {
    prog.correspondences
        .iter()
        .map(|c| (c.iri.clone(), v))
        .collect()
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
    let report = evaluate_gates(
        &prog,
        &[],
        &verdicts_all(&prog, DischargeVerdict::ObligationDischarged),
    );
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
    let report = evaluate_gates(
        &prog,
        &[],
        &verdicts_all(&prog, DischargeVerdict::ObligationDischarged),
    );
    let r = &report.per_correspondence[0];
    assert_eq!(r.round_trip, GateVerdict::Pass);
    assert_eq!(r.mnemomorphism, GateVerdict::Pass);
}

#[test]
fn executed_verdict_decides_round_trip_and_mnemomorphism_not_the_leg_iri() {
    // The gates are execution-free: they read the EXECUTED section-law verdict, never a
    // syntactic leg compare. The content-addressed put-IRI mint (`<get>/put#sha8`) is
    // therefore irrelevant to the verdict — an iso with a genuine inverse discharges (PASS),
    // while the SAME cell with a refuted (ObligationViolated) executed verdict REDs both the
    // round-trip and mnemomorphism gates. The body→verdict discharge itself is proved against
    // the engine in `gmeow_logic::correspondence_exec` (wrong_put_body_is_violated).
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
    // The put IRI IS the content-addressed mint — but the gate never reads it.
    let put_iri = prog.correspondences[0]
        .put_leg
        .clone()
        .expect("iso derives a put leg");
    assert_eq!(
        put_iri,
        crate::projections::put_derivation::derived_put_iri(&get, &prog.correspondences[0]),
        "the put IRI is the content-addressed mint (so an IRI tautology would pass)"
    );

    // A discharged executed verdict passes.
    let ok = evaluate_gates(
        &prog,
        &[],
        &verdicts_all(&prog, DischargeVerdict::ObligationDischarged),
    );
    assert_eq!(ok.per_correspondence[0].round_trip, GateVerdict::Pass);

    // A refuted executed verdict REDs the round-trip AND witness-recovery gates.
    let red = evaluate_gates(
        &prog,
        &[],
        &verdicts_all(&prog, DischargeVerdict::ObligationViolated),
    );
    let r = &red.per_correspondence[0];
    assert!(
        r.round_trip.is_red(),
        "refuted verdict REDs round-trip: {:?}",
        r.round_trip
    );
    assert!(
        r.mnemomorphism.is_red(),
        "refuted verdict REDs witness recovery: {:?}",
        r.mnemomorphism
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
    let prog = program(vec![c]);
    let report = evaluate_gates(
        &prog,
        &[],
        &verdicts_all(&prog, DischargeVerdict::ObligationUnknown),
    );
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
    let prog = program(vec![c]);
    let report = evaluate_gates(
        &prog,
        &[],
        &verdicts_all(&prog, DischargeVerdict::ObligationUnknown),
    );
    let r = &report.per_correspondence[0];
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
    // The executed section-law discharge over the liar's own get/wrong-put legs is refuted.
    let prog = program(vec![c]);
    let report = evaluate_gates(
        &prog,
        &[],
        &verdicts_all(&prog, DischargeVerdict::ObligationViolated),
    );
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
    let prog = program(vec![c]);
    let report = evaluate_gates(
        &prog,
        &[],
        &verdicts_all(&prog, DischargeVerdict::ObligationUnknown),
    );
    let r = &report.per_correspondence[0];
    assert!(r.mnemomorphism.is_red(), "{:?}", r.mnemomorphism);
}

#[test]
fn composition_law_status_overclaim_is_red() {
    // Both parts are lossy lenses whose authored law status is ObligationUnknown (the honest
    // unverified verdict). The composite is also a lossy lens (so the rung-class check passes
    // — not stronger than the join) but claims a DISCHARGED SectionLaw. A composite may not
    // discharge a law its parts leave unverified → law-status RED even though the rung is
    // fine. (Built UN-derived so the authored statuses are exactly what the gate sees; the
    // composite's own law gate is irrelevant — the COMPOSITION is the violation under test.)
    let mk = |name: &str, verdict, law| {
        corr(
            &format!("{GMEOW}ex/{name}"),
            CorrespondenceRelation::Overlaps,
            MorphismClass::LossyLens,
            MorphismKind::InstitutionMorphism,
            false,
            Some(&format!("{GMEOW}ex/{name}Get")),
            None,
            vec![LawClaimIr {
                law,
                verdict,
                condition: None,
            }],
        )
    };
    let left = mk(
        "lsLeft",
        DischargeVerdict::ObligationUnknown,
        CorrespondenceLaw::PutGet,
    );
    let right = mk(
        "lsRight",
        DischargeVerdict::ObligationUnknown,
        CorrespondenceLaw::PutGet,
    );
    let composite = mk(
        "lsComposite",
        DischargeVerdict::ObligationDischarged,
        CorrespondenceLaw::SectionLaw,
    );
    let comps = vec![(
        format!("{GMEOW}ex/lsLeft"),
        format!("{GMEOW}ex/lsRight"),
        Some(format!("{GMEOW}ex/lsComposite")),
    )];
    let prog = program(vec![left, right, composite]);
    let report = evaluate_gates(
        &prog,
        &comps,
        &verdicts_all(&prog, DischargeVerdict::ObligationDischarged),
    );
    let comp = &report.per_composition[0];
    assert_eq!(
        comp.composed_class, "LossyLens",
        "rung join is not stronger"
    );
    assert_eq!(comp.composed_law_status, "ObligationUnknown");
    assert!(
        comp.composition.is_red(),
        "law-status overclaim must RED: {:?}",
        comp.composition
    );
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
    let report = evaluate_gates(
        &prog,
        &comps,
        &verdicts_all(&prog, DischargeVerdict::ObligationDischarged),
    );
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
    let report = evaluate_gates(
        &prog,
        &comps,
        &verdicts_all(&prog, DischargeVerdict::ObligationDischarged),
    );
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
    let report = evaluate_gates(
        &prog,
        &[],
        &verdicts_all(&prog, DischargeVerdict::ObligationDischarged),
    );
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
    let lift = liftability(&evaluate_gates(
        &prog,
        &[],
        &verdicts_all(&prog, DischargeVerdict::ObligationDischarged),
    ));
    assert_eq!(
        lift,
        LiftabilityLedger {
            lawful: 1,
            total: 2
        }
    );
}

#[test]
fn exact_preservation_on_non_injective_rung_is_preservation_red() {
    // A LossyLens (many-to-one, non-invertible get) declaring ExactPreservation overclaims
    // its own preservation judgment: an exact round-trip is impossible in the lossy
    // direction. The preservation-consistency gate REDs it, and assert_gates reds the build.
    let c = corr_pres(
        &format!("{GMEOW}ex/exactLossy"),
        CorrespondenceRelation::Overlaps,
        MorphismClass::LossyLens,
        MorphismKind::InstitutionMorphism,
        PreservationKind::Exact,
    );
    let prog = program(vec![c]);
    let report = evaluate_gates(
        &prog,
        &[],
        &verdicts_all(&prog, DischargeVerdict::ObligationUnknown),
    );
    let r = &report.per_correspondence[0];
    assert!(r.preservation.is_red(), "{:?}", r.preservation);
    let err = assert_gates(&report).unwrap_err();
    assert!(err.0.contains("Preservation"), "{}", err.0);
}

#[test]
fn sound_under_lossy_lens_passes_preservation_gate() {
    // The corrSzsToVerdict shape: a LossyLens declaring SoundUnderApproximation is the
    // HONEST preservation judgment for a non-injective get, so the gate passes.
    let c = corr_pres(
        &format!("{GMEOW}ex/szsToVerdict"),
        CorrespondenceRelation::Subsumes,
        MorphismClass::LossyLens,
        MorphismKind::InstitutionMorphism,
        PreservationKind::SoundUnder,
    );
    let prog = program(vec![c]);
    let report = evaluate_gates(
        &prog,
        &[],
        &verdicts_all(&prog, DischargeVerdict::ObligationUnknown),
    );
    let r = &report.per_correspondence[0];
    assert_eq!(r.preservation, GateVerdict::Pass, "{:?}", r.preservation);
    assert!(assert_gates(&report).is_ok());
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
    let prog = derived(vec![c]);
    let report = evaluate_gates(
        &prog,
        &[],
        &verdicts_all(&prog, DischargeVerdict::ObligationDischarged),
    );
    let json = serde_json::to_string(&report).expect("serialize");
    assert!(json.contains("\"status\":\"pass\""), "{json}");
    assert!(json.contains("\"per_correspondence\""), "{json}");
    assert!(json.contains("\"per_composition\""), "{json}");
}
