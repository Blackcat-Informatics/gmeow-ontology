// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::collections::BTreeMap;

use gmeow_logic_compile::ir::{
    Correspondence, CorrespondenceRelation, Determinacy, MorphismClass, MorphismKind,
    PreservationKind,
};
use gmeow_logic_compile::projections::correspondence::CorrespondenceProgram;
use gmeow_logic_compile::projections::correspondence_gates::evaluate_gates;

use super::*;
use crate::up_projection::{AuditReport, FileBaseline};

// --------------------------------------------------------------------------- //
// classify_term — the evidence → shape policy (isolated, no gates)
// --------------------------------------------------------------------------- //

#[test]
fn verifiable_round_trip_is_an_injective_section_with_two_real_legs() {
    let shape = classify_term(&LiftEvidence::VerifiableRoundTrip {
        direct: "http://schema.org/name".to_owned(),
        inverse: "http://schema.org/name".to_owned(),
    })
    .expect("a verifiable round-trip mints a correspondence");
    assert_eq!(shape.class, MorphismClass::SectionRetraction);
    assert_eq!(shape.relation, CorrespondenceRelation::Equiv);
    assert!(!shape.mnemomorphic, "the audit never fabricates a witness");
    assert!(
        shape.legs.is_some(),
        "a proved candidate registers real legs"
    );
}

#[test]
fn clean_without_a_round_trip_is_an_injective_but_unproved_lens() {
    let shape = classify_term(&LiftEvidence::CleanAsserted).expect("clean mints a correspondence");
    // WellBehavedLens is injective but makes NO full round-trip claim → not proved, claimed.
    assert_eq!(shape.class, MorphismClass::WellBehavedLens);
    assert!(shape.legs.is_none(), "no verifiable inverse leg");
}

#[test]
fn claimed_lift_is_a_lossy_overlap_under_an_unknown_obligation() {
    let shape = classify_term(&LiftEvidence::ClaimedLift).expect("a claim mints a correspondence");
    assert_eq!(shape.relation, CorrespondenceRelation::Overlaps);
    assert_eq!(shape.class, MorphismClass::AffineCorrespondence);
    assert_eq!(shape.laws.len(), 1, "carries one honest GetPut claim");
}

#[test]
fn unsupported_buckets_mint_no_correspondence() {
    assert!(classify_term(&LiftEvidence::Unsupported).is_none());
}

// --------------------------------------------------------------------------- //
// ledger_from_audit — the four-tier partition over a synthetic audit
// --------------------------------------------------------------------------- //

fn audit_of(terms: &[(&str, &str)]) -> AuditReport {
    let per_term: BTreeMap<String, String> = terms
        .iter()
        .map(|(t, b)| ((*t).to_owned(), (*b).to_owned()))
        .collect();
    AuditReport {
        files: vec![FileBaseline {
            name: "fixture".to_owned(),
            per_term,
            per_vocab: BTreeMap::new(),
        }],
        gaps: Vec::new(),
        sssom_total: 0,
        struct_total: 0,
    }
}

fn qmap(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

#[test]
fn the_four_tiers_partition_every_audited_term() {
    let audit = audit_of(&[
        ("schema:name", "clean"),  // verifiable round-trip  → proved
        ("ex:bad", "clean"),       // direct != inverse      → red_excluded
        ("schema:about", "clean"), // no edoal path          → claimed
        ("odrl:permission", "liftable-with-claim"), // claim        → claimed
        ("x:gap", "GAP"),          // non-liftable           → unsupported
        ("y:narrow", "down-only"), // non-liftable           → unsupported
        ("z:mint", "hard-mint"),   // non-liftable           → unsupported
    ]);
    let direct = qmap(&[
        ("schema:name", "http://schema.org/name"),
        ("ex:bad", "http://example.org/p"),
    ]);
    let inverse = qmap(&[
        ("schema:name", "http://schema.org/name"),
        ("ex:bad", "http://example.org/q"), // a DIFFERENT real predicate — does not invert
    ]);

    let ledger = ledger_from_audit(&audit, &direct, &inverse).expect("ledger computes");

    assert_eq!(
        ledger.totals.proved, 1,
        "only the matching round-trip is proved"
    );
    assert_eq!(
        ledger.totals.claimed, 2,
        "clean-no-edoal + claim are claimed"
    );
    assert_eq!(
        ledger.totals.red_excluded, 1,
        "the non-inverting reverse rule is gate-excluded"
    );
    assert_eq!(ledger.totals.unsupported, 3, "GAP/down-only/hard-mint");
    // The strict partition: every audited term lands in exactly one tier.
    assert_eq!(ledger.total(), 7);
    assert_eq!(
        ledger.totals.proved
            + ledger.totals.claimed
            + ledger.totals.red_excluded
            + ledger.totals.unsupported,
        7
    );
    // The headline numerator is proved + claimed.
    assert_eq!(ledger.liftable(), 3);
}

#[test]
fn a_non_inverting_reverse_rule_is_not_lawful_even_when_the_bucket_is_clean() {
    // Criterion #3, the load-bearing negative test: a `clean`-bucketed term whose authored
    // reverse rule is a DIFFERENT real predicate (not a corrupted body) must drop out of
    // `proved` via the round-trip gate — never counted lawful on the bucket's say-so.
    let audit = audit_of(&[("ex:bad", "clean")]);
    let direct = qmap(&[("ex:bad", "http://example.org/forward")]);
    let inverse = qmap(&[("ex:bad", "http://example.org/notTheInverse")]);

    let ledger = ledger_from_audit(&audit, &direct, &inverse).expect("ledger computes");
    assert_eq!(
        ledger.totals.proved, 0,
        "a non-inverting clean term is NOT proved"
    );
    assert_eq!(
        ledger.totals.red_excluded, 1,
        "it is gate-excluded, surfaced"
    );
    assert_eq!(
        ledger.liftable(),
        0,
        "and excluded from the liftable headline"
    );
}

#[test]
fn a_matching_reverse_rule_is_proved_lawful() {
    let audit = audit_of(&[("schema:name", "clean")]);
    let direct = qmap(&[("schema:name", "http://schema.org/name")]);
    let inverse = qmap(&[("schema:name", "http://schema.org/name")]);
    let ledger = ledger_from_audit(&audit, &direct, &inverse).expect("ledger computes");
    assert_eq!(ledger.totals.proved, 1);
    assert_eq!(ledger.totals.red_excluded, 0);
    assert_eq!(ledger.liftable(), 1);
}

// --------------------------------------------------------------------------- //
// overclaim gate — a claim-strength term asserting equivalence REDs
// --------------------------------------------------------------------------- //

#[test]
fn an_equivalence_overclaim_on_a_lossy_rung_reds() {
    // A closeMatch-style term (lossy / co-projection) that asserts an exactMatch-strength
    // equivalence (logic:Equiv on an AffineCorrespondence rung) must RED via the overclaim
    // gate — the gate adds verification the bucket cannot.
    let corr = Correspondence::new(
        "https://blackcatinformatics.ca/logic/up-projection-audit/overclaim".to_owned(),
        CorrespondenceRelation::Equiv,
        MorphismClass::AffineCorrespondence,
        MorphismKind::InstitutionMorphism,
        false,
        Some(Determinacy::Vague),
        None,
        None,
        Vec::new(),
        None,
        None,
        None,
        None,
        None,
    )
    .expect("well-formed");
    let program = CorrespondenceProgram::new(vec![corr], Vec::new(), PreservationKind::SoundUnder);
    let report = evaluate_gates(&program, &[]);
    assert!(
        report.per_correspondence[0].overclaim.is_red(),
        "equivalence on a non-injective rung must RED"
    );
}
