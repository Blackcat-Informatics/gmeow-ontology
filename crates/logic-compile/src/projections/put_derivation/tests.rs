// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::ir::{
    CorrespondenceRelation, Determinacy, MorphismClass, MorphismKind, PreservationKind,
};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

/// A mnemomorphic correspondence on an injective rung, authored with a `get` leg and NO
/// `put` leg — the canonical input the derivation lawfully completes.
fn mnemomorphic_get_only(class: MorphismClass) -> Correspondence {
    Correspondence::new(
        format!("{GMEOW}example/recoverableCorrespondence"),
        CorrespondenceRelation::Equiv,
        class,
        MorphismKind::InstitutionMorphism,
        true,
        Some(Determinacy::Crisp),
        Some(format!("{GMEOW}example/getLeg")),
        None,
        Vec::new(),
        Some(1.0),
        None,
        None,
        None,
        None,
    )
    .expect("well-formed mnemomorphic get-only correspondence")
}

#[test]
fn recoverable_cell_derives_a_lawful_section_put() {
    for class in [
        MorphismClass::Isomorphism,
        MorphismClass::SectionRetraction,
        MorphismClass::WellBehavedLens,
    ] {
        let c = mnemomorphic_get_only(class);
        let get_leg = c.get_leg.clone().unwrap();
        let derived = derive_put(&c).expect("derivation succeeds");
        let PutDerivation::Derived(dp) = derived else {
            panic!("expected a lawful recovery for {class:?}");
        };
        assert!(dp.mnemomorphic_recovery, "{class:?} is a witness recovery");
        assert_eq!(dp.preservation, PreservationKind::CompleteOver);
        assert_eq!(dp.section_claim.law, CorrespondenceLaw::SectionLaw);
        assert_eq!(
            dp.section_claim.verdict,
            DischargeVerdict::ObligationDischarged
        );
        assert!(dp.residue.is_empty(), "a recovery flags no residue");
        // The mint is the content-addressed inverse-along-witness IRI.
        assert_eq!(dp.put_leg, derived_put_iri(&get_leg, &c));
        assert!(dp.put_leg.starts_with(&format!("{get_leg}/put#")));
    }
}

#[test]
fn mint_recomputes_identically_from_the_stored_cell() {
    // The Round-trip-gate hinge: the mint depends ONLY on the get-side identity, which is
    // invariant under the derivation. Folding the derived put + section claim back in must
    // NOT change the recomputed mint.
    let c = mnemomorphic_get_only(MorphismClass::Isomorphism);
    let get_leg = c.get_leg.clone().unwrap();
    let mint_before = derived_put_iri(&get_leg, &c);

    let program = CorrespondenceProgram::new(vec![c], Vec::new(), PreservationKind::CompleteOver);
    let (rebuilt, outcomes) = program.with_derived_puts().expect("derive puts");
    assert_eq!(outcomes.len(), 1);

    let stored = &rebuilt.correspondences[0];
    assert_eq!(stored.put_leg.as_deref(), Some(mint_before.as_str()));
    // Recomputing from the STORED (put-bearing, section-claim-folded) cell yields the
    // same mint — the gate's `put ∘ get = id` reduces to this string compare.
    assert_eq!(derived_put_iri(&get_leg, stored), mint_before);
    assert!(stored
        .law_claims
        .iter()
        .any(|lc| lc.law == CorrespondenceLaw::SectionLaw));
}

#[test]
fn amnesic_co_authored_claim_mints_with_claim_validation_only() {
    // mnemomorphic=false on a non-injective rung, but the author declared a put-direction
    // law status: a co-authored put-with-claim → minted-with-claim, ValidationOnly.
    let c = Correspondence::new(
        format!("{GMEOW}example/amnesicCorrespondence"),
        CorrespondenceRelation::Overlaps,
        MorphismClass::LossyLens,
        MorphismKind::InstitutionMorphism,
        false,
        None,
        Some(format!("{GMEOW}example/lossyGetLeg")),
        None,
        vec![LawClaimIr {
            law: CorrespondenceLaw::PutGet,
            verdict: DischargeVerdict::ObligationUnknown,
            condition: None,
        }],
        Some(0.6),
        None,
        None,
        None,
        None,
    )
    .expect("well-formed amnesic correspondence");

    let PutDerivation::Derived(dp) = derive_put(&c).expect("derivation succeeds") else {
        panic!("a co-authored claim mints a put-with-claim, not Unsupported");
    };
    assert!(
        !dp.mnemomorphic_recovery,
        "minted-with-claim, not a recovery"
    );
    assert_eq!(dp.preservation, PreservationKind::ValidationOnly);
    assert_eq!(dp.section_claim.law, CorrespondenceLaw::PutGet);
    assert_eq!(
        dp.section_claim.verdict,
        DischargeVerdict::ObligationUnknown
    );
    assert!(
        !dp.residue.is_empty(),
        "mint-with-claim discloses its residue"
    );
}

#[test]
fn no_witness_no_claim_is_the_unsupported_floor() {
    // mnemomorphic=false, non-injective rung, NO law claims: the honest legalization
    // floor — the up-lift is carried and flagged, never minted.
    let c = Correspondence::new(
        format!("{GMEOW}example/unsupportedCorrespondence"),
        CorrespondenceRelation::RelatedMatch,
        MorphismClass::Prism,
        MorphismKind::InstitutionMorphism,
        false,
        None,
        Some(format!("{GMEOW}example/prismGetLeg")),
        None,
        Vec::new(),
        None,
        None,
        None,
        None,
        None,
    )
    .expect("well-formed unsupported correspondence");

    match derive_put(&c).expect("derivation succeeds") {
        PutDerivation::Unsupported { residue } => {
            assert!(!residue.is_empty(), "the floor still flags its residue")
        }
        PutDerivation::Derived(_) => panic!("no witness and no claim must be Unsupported"),
    }
}

#[test]
fn unsupported_cell_keeps_its_put_less_form_through_with_derived_puts() {
    let c = Correspondence::new(
        format!("{GMEOW}example/unsupportedCorrespondence"),
        CorrespondenceRelation::RelatedMatch,
        MorphismClass::Prism,
        MorphismKind::InstitutionMorphism,
        false,
        None,
        Some(format!("{GMEOW}example/prismGetLeg")),
        None,
        Vec::new(),
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    let program = CorrespondenceProgram::new(vec![c], Vec::new(), PreservationKind::SoundUnder);
    let (rebuilt, outcomes) = program.with_derived_puts().expect("derive puts");
    assert_eq!(rebuilt.correspondences[0].put_leg, None, "no put minted");
    assert!(matches!(
        outcomes[0].derivation,
        PutDerivation::Unsupported { .. }
    ));
}

#[test]
fn authored_put_leg_is_never_re_derived() {
    // The §14 affine triangle carries its own authored put leg — with_derived_puts leaves
    // it untouched (derive_put would hard-fail on a put-bearing cell).
    let program = super::super::correspondence::affine_triangle_worked_example();
    let original = program.correspondences[0].put_leg.clone();
    assert!(original.is_some());
    let (rebuilt, outcomes) = program.with_derived_puts().expect("derive puts");
    assert_eq!(rebuilt.correspondences[0].put_leg, original);
    assert!(
        outcomes.is_empty(),
        "an authored-put cell yields no derivation"
    );
}

#[test]
fn get_only_with_no_get_leg_is_a_hard_fail() {
    // A correspondence with neither a get leg nor a put leg: there is no view for a
    // witness to travel in. with_derived_puts skips it (get_leg None), but a direct
    // derive_put is a hard error (no-optionality).
    let c = Correspondence::new(
        format!("{GMEOW}example/legless"),
        CorrespondenceRelation::Equiv,
        MorphismClass::Isomorphism,
        MorphismKind::InstitutionMorphism,
        true,
        None,
        None,
        None,
        Vec::new(),
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    assert!(derive_put(&c).is_err(), "no get leg is a hard fail");
}

#[test]
fn mnemomorphic_on_non_injective_rung_is_unsupported_not_minted_with_claim() {
    // A `mnemomorphic` flag on a NON-injective rung is incoherent: the witness cannot honour
    // the rung. `derive_put` must NOT mint a put for it (that would disagree with the
    // Mnemomorphism gate, which REDs the declaration) — it falls through to the Unsupported
    // floor. The gate stays the single authority on the bad-witness coherence rule.
    let c = mnemomorphic_get_only(MorphismClass::LossyLens);
    assert!(c.mnemomorphic && c.law_claims.is_empty());
    match derive_put(&c).expect("derivation is total") {
        PutDerivation::Unsupported { residue } => {
            assert!(!residue.is_empty(), "the unsupported floor flags a residue");
        }
        PutDerivation::Derived(dp) => {
            panic!("a mnemomorphic-on-non-injective cell must NOT mint a put: {dp:?}")
        }
    }
}
