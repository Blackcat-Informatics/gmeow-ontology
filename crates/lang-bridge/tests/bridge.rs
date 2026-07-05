// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Tests for the decidable round-trip and exactness helpers that read facts off the
//! carried `logic:Correspondence`.

use gmeow_lang_bridge::{exact_round_trip_holds, is_exact_correspondence};
use gmeow_logic_compile::ir::{
    Correspondence, CorrespondenceLaw, CorrespondenceRelation, DischargeVerdict, LawClaimIr,
    LegPath, MorphismClass, MorphismKind,
};

fn step(p: &str) -> LegPath {
    LegPath::Step(p.to_owned())
}

fn inv(inner: LegPath) -> LegPath {
    LegPath::Inverse(Box::new(inner))
}

/// Build a minimal correspondence carrying the given rung and law claims; the axes and
/// legs are left unset because the exactness helper reads only rung + claims.
fn correspondence(class: MorphismClass, claims: Vec<LawClaimIr>) -> Correspondence {
    Correspondence::new(
        "https://blackcatinformatics.ca/lang/corr/test",
        CorrespondenceRelation::Equiv,
        class,
        MorphismKind::InstitutionMorphism,
        false,
        None,
        None,
        None,
        claims,
        None,
        None,
        None,
        None,
        None,
    )
    .expect("valid correspondence")
}

fn claim(law: CorrespondenceLaw, verdict: DischargeVerdict) -> LawClaimIr {
    LawClaimIr {
        law,
        verdict,
        condition: None,
    }
}

#[test]
fn step_and_its_inverse_round_trip() {
    // put == get.invert(): a single step's lawful put leg is its structural inverse.
    let get = step("p");
    let put = inv(step("p"));
    assert!(exact_round_trip_holds(&get, &put));
}

#[test]
fn mismatched_put_does_not_round_trip() {
    let get = step("p");
    let put = step("q");
    assert!(!exact_round_trip_holds(&get, &put));
}

#[test]
fn seq_inverts_to_reversed_inverted_branches() {
    // reverse(a / b) = ^b / ^a — the lawful put for a sequential get.
    let get = LegPath::Seq(vec![step("a"), step("b")]);
    let put = LegPath::Seq(vec![inv(step("b")), inv(step("a"))]);
    assert!(exact_round_trip_holds(&get, &put));

    // A put that keeps the source order (^a / ^b) is not the inverse.
    let wrong = LegPath::Seq(vec![inv(step("a")), inv(step("b"))]);
    assert!(!exact_round_trip_holds(&get, &wrong));
}

#[test]
fn isomorphism_with_discharged_claim_is_exact() {
    let c = correspondence(
        MorphismClass::Isomorphism,
        vec![claim(
            CorrespondenceLaw::GetPut,
            DischargeVerdict::ObligationDischarged,
        )],
    );
    assert!(is_exact_correspondence(&c));
}

#[test]
fn lossy_lens_is_not_exact() {
    // A discharged claim cannot rescue a non-injective rung.
    let c = correspondence(
        MorphismClass::LossyLens,
        vec![claim(
            CorrespondenceLaw::GetPut,
            DischargeVerdict::ObligationDischarged,
        )],
    );
    assert!(!is_exact_correspondence(&c));
}

#[test]
fn injective_rung_with_only_unknown_claims_is_not_exact() {
    // An injective rung whose laws are all merely carried-forward is a claim, not a
    // discharge.
    let c = correspondence(
        MorphismClass::WellBehavedLens,
        vec![
            claim(
                CorrespondenceLaw::GetPut,
                DischargeVerdict::ObligationUnknown,
            ),
            claim(
                CorrespondenceLaw::PutGet,
                DischargeVerdict::ObligationUnknown,
            ),
        ],
    );
    assert!(!is_exact_correspondence(&c));
}
