// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::collections::{BTreeMap, BTreeSet};

use gmeow_logic_compile::ir::{
    Correspondence, CorrespondenceRelation, Determinacy, MorphismClass, MorphismKind,
    PreservationKind,
};
use gmeow_logic_compile::projections::correspondence::CorrespondenceProgram;
use gmeow_logic_compile::projections::correspondence_gates::evaluate_gates;

use super::*;
use crate::up_projection_corpus::{AuditReport, FileBaseline};

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
        None,
    )
    .expect("well-formed");
    let program = CorrespondenceProgram::new(vec![corr], Vec::new(), PreservationKind::SoundUnder);
    let verdicts = gmeow_logic::correspondence_exec::program_verdicts(&program);
    let report = evaluate_gates(&program, &[], &verdicts);
    assert!(
        report.per_correspondence[0].overclaim.is_red(),
        "equivalence on a non-injective rung must RED"
    );
}

// --------------------------------------------------------------------------- //
// candidate_lifts — cross-layer ambiguity guard (feedback #4)
// --------------------------------------------------------------------------- //

fn edoal_set(pairs: &[(&str, &[&str])]) -> BTreeMap<String, BTreeSet<String>> {
    pairs
        .iter()
        .map(|(target, gmeows)| {
            (
                (*target).to_owned(),
                gmeows
                    .iter()
                    .map(|g| (*g).to_owned())
                    .collect::<BTreeSet<_>>(),
            )
        })
        .collect()
}

/// A single-atom, generalizing (`<=`) structural cell: `toPredicate <target>` keyed on a real
/// `edoalSource <gmeow>` so `structural_pairs` records it in the *generalizing* claim layer.
fn generalizing_struct_cell(
    cell: &str,
    source_gmeow: &str,
    target: &str,
    confidence: &str,
) -> String {
    format!(
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         gmeow:{cell} a gmeow:ProjectionMapping ;\n\
           gmeow:hasMappingPattern [\n\
             gmeow:edoalSource <{source_gmeow}> ] ;\n\
           gmeow:hasBinding [ gmeow:toPredicate <{target}> ; \
                              gmeow:relation \"<=\" ; \
                              gmeow:confidence \"{confidence}\" ] .\n"
    )
}

#[test]
fn a_direct_ambiguous_term_is_not_recovered_as_inverse_or_claim() {
    // GUARD (feedback #4, case 1): a target with MULTIPLE direct candidates is dropped as
    // ambiguous residue. The retired executor tracked such a target in a shared `ambiguous` set
    // so no lower-priority layer could recover it. The refactor's inverse/claim loops only checked
    // `direct.contains_key` — which is FALSE for a dropped-ambiguous target — so the SAME term
    // could re-enter as an inverse or a claim lift. It must stay honest residue at every layer.
    let target = "https://schema.org/ambiguous";
    // Direct EDOAL: TWO distinct gmeow candidates → ambiguous, target NOT inserted into `direct`.
    let direct_edoal = edoal_set(&[(
        target,
        &[
            "https://blackcatinformatics.ca/gmeow/one",
            "https://blackcatinformatics.ca/gmeow/two",
        ],
    )]);
    // Inverse EDOAL: a SINGLE clean candidate on the SAME target — must NOT be recovered.
    let inverse_edoal = edoal_set(&[(target, &["https://blackcatinformatics.ca/gmeow/inv"])]);
    // A closeMatch SSSOM claim on the SAME target — must NOT be recovered either.
    let sssom = concat!(
        "#curie_map:\n",
        "#  gmeow: https://blackcatinformatics.ca/gmeow/\n",
        "#  schema: https://schema.org/\n",
        "#  skos: http://www.w3.org/2004/02/skos/core#\n",
        "subject_id\tpredicate_id\tobject_id\tconfidence\n",
        "gmeow:claimant\tskos:closeMatch\tschema:ambiguous\t0.8\n",
    );

    let (direct, inverse, claim, ambiguous_dropped) =
        candidate_lifts(&[sssom.to_owned()], &[], &direct_edoal, &inverse_edoal)
            .expect("candidate_lifts resolves");

    assert!(
        !direct.contains_key(target),
        "the multi-candidate direct target is dropped, never a lift"
    );
    assert!(
        !inverse.contains_key(target),
        "a direct-ambiguous target must NOT be recovered as an inverse lift"
    );
    assert!(
        !claim.contains_key(target),
        "a direct-ambiguous target must NOT be recovered as a claim lift"
    );
    // Counted exactly ONCE (at the direct layer); the blocked inverse/claim never re-count it.
    assert_eq!(
        ambiguous_dropped, 1,
        "the ambiguous target is counted once, not double-counted across layers"
    );
}

#[test]
fn conflicting_generalizing_and_closematch_targets_are_ambiguous_not_first_layer_wins() {
    // GUARD (feedback #4, case 2): the claim step must consider the UNION of the generalizing
    // structural and SSSOM closeMatch candidates per target. A target with ONE candidate in EACH
    // layer pointing at DIFFERENT gmeow targets is a cross-layer conflict → ambiguous. The buggy
    // per-layer `claim.contains_key` short-circuit let the first layer win instead.
    let target = "https://schema.org/conflict";
    // Generalizing structural: target → gmeow:general (confidence-bearing `<=` cell).
    let ttl = generalizing_struct_cell(
        "genCell",
        "https://blackcatinformatics.ca/gmeow/general",
        target,
        "0.9",
    );
    // SSSOM closeMatch: SAME target → a DIFFERENT gmeow:close.
    let sssom = concat!(
        "#curie_map:\n",
        "#  gmeow: https://blackcatinformatics.ca/gmeow/\n",
        "#  schema: https://schema.org/\n",
        "#  skos: http://www.w3.org/2004/02/skos/core#\n",
        "subject_id\tpredicate_id\tobject_id\tconfidence\n",
        "gmeow:close\tskos:closeMatch\tschema:conflict\t0.8\n",
    );

    let (_direct, _inverse, claim, ambiguous_dropped) = candidate_lifts(
        &[sssom.to_owned()],
        &[ttl],
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("candidate_lifts resolves");

    assert!(
        !claim.contains_key(target),
        "a cross-layer generalizing-vs-closeMatch conflict must NOT resolve to a claim lift"
    );
    assert_eq!(
        ambiguous_dropped, 1,
        "the cross-layer conflict is counted as one ambiguous drop"
    );
}

#[test]
fn a_single_agreeing_claim_candidate_still_lifts() {
    // NON-REGRESSION: a target with the SAME gmeow across the generalizing and closeMatch layers
    // (or in just one layer) is NOT a conflict and must still resolve to a claim lift, so the
    // union guard tightens ONLY the genuinely ambiguous case, not the honest single-candidate one.
    let target = "https://schema.org/agree";
    let ttl = generalizing_struct_cell(
        "agreeCell",
        "https://blackcatinformatics.ca/gmeow/agree",
        target,
        "0.9",
    );
    let sssom = concat!(
        "#curie_map:\n",
        "#  gmeow: https://blackcatinformatics.ca/gmeow/\n",
        "#  schema: https://schema.org/\n",
        "#  skos: http://www.w3.org/2004/02/skos/core#\n",
        "subject_id\tpredicate_id\tobject_id\tconfidence\n",
        "gmeow:agree\tskos:closeMatch\tschema:agree\t0.8\n",
    );

    let (_direct, _inverse, claim, ambiguous_dropped) = candidate_lifts(
        &[sssom.to_owned()],
        &[ttl],
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("candidate_lifts resolves");

    assert_eq!(
        claim.get(target).map(|c| c.gmeow.as_str()),
        Some("https://blackcatinformatics.ca/gmeow/agree"),
        "an agreeing single-target claim across both layers still lifts"
    );
    assert_eq!(ambiguous_dropped, 0, "no ambiguity when both layers agree");
}

#[test]
fn a_generalizing_winner_survives_a_self_ambiguous_closematch_layer() {
    // NON-REGRESSION (layer priority): when the generalizing layer has a single unique winner
    // but the closeMatch layer is internally ambiguous (>1 distinct candidate) for the SAME
    // target, the generalizing winner must still lift — a self-ambiguous lower-priority layer is
    // NOT a cross-layer conflict, and dropping the clean generalizing pick would be a real
    // coverage regression (this mirrors the corpus's `prov:Activity` / `sosa:Observation` shape).
    let target = "https://schema.org/priority";
    let ttl = generalizing_struct_cell(
        "priorityCell",
        "https://blackcatinformatics.ca/gmeow/generalWinner",
        target,
        "0.9",
    );
    // TWO closeMatch rows for the SAME target → the closeMatch layer is self-ambiguous.
    let sssom = concat!(
        "#curie_map:\n",
        "#  gmeow: https://blackcatinformatics.ca/gmeow/\n",
        "#  schema: https://schema.org/\n",
        "#  skos: http://www.w3.org/2004/02/skos/core#\n",
        "subject_id\tpredicate_id\tobject_id\tconfidence\n",
        "gmeow:closeA\tskos:closeMatch\tschema:priority\t0.8\n",
        "gmeow:closeB\tskos:closeMatch\tschema:priority\t0.7\n",
    );

    let (_direct, _inverse, claim, ambiguous_dropped) = candidate_lifts(
        &[sssom.to_owned()],
        &[ttl],
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("candidate_lifts resolves");

    assert_eq!(
        claim.get(target).map(|c| c.gmeow.as_str()),
        Some("https://blackcatinformatics.ca/gmeow/generalWinner"),
        "the clean generalizing winner survives a self-ambiguous closeMatch layer"
    );
    assert_eq!(
        ambiguous_dropped, 0,
        "a self-ambiguous lower-priority layer is not a cross-layer conflict"
    );
}

// --------------------------------------------------------------------------- //
// discharged rename collision — two discharged `=` cells claiming the same ext
// --------------------------------------------------------------------------- //

/// A mnemomorphic (`=`) `toClass` retype cell keyed on `edoalSource <source_gmeow>` and lifting the
/// external class `<target>`. A `toClass` binding orients as `Direct` without needing an anchor/atom
/// list, so this is the minimal shape that exercises the discharged-rename promotion path.
fn mnemomorphic_class_cell(cell: &str, source_gmeow: &str, target: &str) -> String {
    format!(
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         gmeow:{cell} a gmeow:ProjectionMapping ;\n\
           gmeow:hasMappingPattern [\n\
             gmeow:edoalSource <{source_gmeow}> ] ;\n\
           gmeow:hasBinding [ gmeow:toClass <{target}> ; \
                              gmeow:relation \"=\" ; \
                              gmeow:mnemomorphic \"true\" ] .\n"
    )
}

#[test]
fn two_discharged_cells_claiming_the_same_ext_hard_fail_not_last_wins() {
    // Discharged-rename invariant: promotion streams into `rules.insert(ext, …)`
    // in input order, so TWO discharged mnemomorphic `=` cells resolving to the SAME external `ext`
    // but DIFFERENT gmeow targets would silently last-wins overwrite one lawful rename with the
    // other — a nondeterministic soundness hole. It must be a HARD FAIL naming the colliding ext.
    let ext = "http://rdfs.org/sioc/ns#Post"; // a real projection-namespace (sioc) external class
    let cell_a = "https://blackcatinformatics.ca/gmeow/cellA";
    let cell_b = "https://blackcatinformatics.ca/gmeow/cellB";
    let ttl_a =
        mnemomorphic_class_cell("cellA", "https://blackcatinformatics.ca/gmeow/Message", ext);
    let ttl_b = mnemomorphic_class_cell(
        "cellB",
        "https://blackcatinformatics.ca/gmeow/Container",
        ext,
    );
    // BOTH cells are discharged (Deliverable A authorized), so neither trips the missing-verdict
    // hard fail — the ONLY thing that can stop the ambiguity is the collision guard.
    let discharged: BTreeSet<String> = [cell_a.to_owned(), cell_b.to_owned()].into_iter().collect();

    let err = gate_verified_lift_program(&[], &[ttl_a, ttl_b], &discharged)
        .expect_err("two discharged cells claiming the same ext must hard-fail, never last-wins");
    let rendered = err.to_string();
    assert!(
        rendered.contains(ext),
        "the collision error must name the colliding external term, got: {rendered}"
    );
    assert!(
        rendered.contains(cell_a) && rendered.contains(cell_b),
        "the collision error must name BOTH offending discharged cells, got: {rendered}"
    );
}

#[test]
fn an_idempotent_duplicate_discharged_rename_is_not_a_collision() {
    // The precise sound condition is "a collision that would CHANGE the resulting rule": a second
    // discharged rename for the same `ext` that resolves to the SAME gmeow target AND orientation is
    // a byte-identical no-op and must be tolerated (not spuriously hard-failed).
    let ext = "http://rdfs.org/sioc/ns#Post";
    let cell_a = "https://blackcatinformatics.ca/gmeow/dupA";
    let cell_b = "https://blackcatinformatics.ca/gmeow/dupB";
    let gmeow = "https://blackcatinformatics.ca/gmeow/Message";
    let ttl_a = mnemomorphic_class_cell("dupA", gmeow, ext);
    let ttl_b = mnemomorphic_class_cell("dupB", gmeow, ext);
    let discharged: BTreeSet<String> = [cell_a.to_owned(), cell_b.to_owned()].into_iter().collect();

    let program = gate_verified_lift_program(&[], &[ttl_a, ttl_b], &discharged)
        .expect("an idempotent duplicate rename resolves without a false collision");
    let rule = program
        .rules
        .get(ext)
        .expect("the single lawful rename is promoted once");
    assert_eq!(rule.gmeow, gmeow);
    assert_eq!(rule.orientation, Orientation::Direct);
    assert_eq!(rule.kind, LiftKind::Fact);
}
