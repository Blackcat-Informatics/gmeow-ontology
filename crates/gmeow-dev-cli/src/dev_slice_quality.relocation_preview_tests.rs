// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn a_full_move_off_a_green_corpus_is_always_payable() {
    // The live-corpus case: the source sits AT its ceiling and the destination is at
    // or under its own, so the credit the lowering raises exactly covers the raise
    // the destination must commit. This is why the `unpaid` column reads 0 on every
    // real preview — the preview says so in words rather than leaving a maintainer
    // to wonder whether the column is even wired up.
    assert_eq!(
        relocation_plan(4, 17, 17, 0, 0),
        RelocationPlan {
            credit: 4,
            demand: 4,
            unpaid: 0
        }
    );
    // The same with stale headroom at the source and existing headroom at the
    // destination: the credit is still clamped to what actually moves, and the
    // demand shrinks because the destination already had room.
    assert_eq!(
        relocation_plan(3, 39, 40, 0, 4),
        RelocationPlan {
            credit: 3,
            demand: 0,
            unpaid: 0
        }
    );
}

#[test]
fn unpaid_is_nonzero_only_when_the_destination_is_already_over_its_ceiling() {
    // The structural fact the preview's verdict line names: `demand` exceeds
    // `moving` — and therefore exceeds the clamped `credit` — exactly when the
    // destination's measured residue already sits above its committed ceiling. Here
    // the destination measures 5 against a ceiling of 2, so three units of the raise
    // are debt that predates the move and no relocation can pay for.
    assert_eq!(
        relocation_plan(1, 10, 10, 5, 2),
        RelocationPlan {
            credit: 1,
            demand: 4,
            unpaid: 3
        }
    );
}

#[test]
fn the_credit_clamp_is_load_bearing() {
    // A source carrying huge STALE headroom (ceiling 90 against a measured residue
    // of 10) lowers by a lot, but only ONE construct actually moves. Lowering dead
    // headroom surrenders no authoring, so the credit is clamped to the one unit
    // that moved — never the 81-unit paper drop.
    assert_eq!(relocation_plan(1, 10, 90, 0, 0).credit, 1);
}

fn construct(anchor: &str) -> gmeow_slice_quality::Construct {
    gmeow_slice_quality::Construct {
        key: anchor.to_owned(),
        grounded: false,
        is_bridge: false,
        witness: gmeow_slice_quality::Witness::Anchored(anchor.to_owned()),
    }
}

#[test]
fn case_a_a_term_absent_from_both_slices_is_not_unwitnessed() {
    // Neither slice's residue anchors "ex:ghost" at all — a relocation of it
    // genuinely moves nothing, and it must land in `absent`, never `unwitnessed`.
    let vocab = gmeow_slice_quality::counting::shacl_vocab();
    let to_residue: std::collections::BTreeMap<String, Vec<gmeow_slice_quality::Construct>> =
        std::collections::BTreeMap::from([(vocab.prefix.clone(), vec![construct("ex:real")])]);
    let terms = vec!["ex:ghost".to_owned()];
    let (unwitnessed, absent) =
        classify_unmoved_terms(&terms, &to_residue, std::slice::from_ref(&vocab));
    assert_eq!(unwitnessed, Vec::<&str>::new());
    assert_eq!(absent, vec!["ex:ghost"]);
}

#[test]
fn case_b_a_term_anchored_only_at_the_destination_is_unwitnessed_not_absent() {
    // This is the preview's own negative control: a maintainer runs the preview
    // with --from/--to swapped (or simply names a term that already lives at the
    // destination). "ex:emotion" DOES anchor real residue — just at `to_iri`, not
    // the requested `from_iri` — so there is no departure to pair with an arrival.
    // It must be reported as `unwitnessed: 1 of 1`, never folded into "nothing
    // would move".
    let vocab = gmeow_slice_quality::counting::shacl_vocab();
    let to_residue: std::collections::BTreeMap<String, Vec<gmeow_slice_quality::Construct>> =
        std::collections::BTreeMap::from([(vocab.prefix.clone(), vec![construct("ex:emotion")])]);
    let terms = vec!["ex:emotion".to_owned()];
    let (unwitnessed, absent) =
        classify_unmoved_terms(&terms, &to_residue, std::slice::from_ref(&vocab));
    assert_eq!(unwitnessed, vec!["ex:emotion"]);
    assert_eq!(absent, Vec::<&str>::new());
}

#[test]
fn a_mixed_request_splits_cleanly_between_both_classes() {
    let vocab = gmeow_slice_quality::counting::shacl_vocab();
    let to_residue: std::collections::BTreeMap<String, Vec<gmeow_slice_quality::Construct>> =
        std::collections::BTreeMap::from([(vocab.prefix.clone(), vec![construct("ex:emotion")])]);
    let terms = vec!["ex:ghost".to_owned(), "ex:emotion".to_owned()];
    let (unwitnessed, absent) =
        classify_unmoved_terms(&terms, &to_residue, std::slice::from_ref(&vocab));
    assert_eq!(unwitnessed, vec!["ex:emotion"]);
    assert_eq!(absent, vec!["ex:ghost"]);
}

#[test]
fn absent_terms_message_never_reads_as_a_universal_none() {
    // The mixed case this wording exists to get right: `absent` (1 term) is a
    // SUBSET of `total_requested` (2 terms) — the other one is `unwitnessed`,
    // printed separately. The old "NONE of the 1 of 2 requested term(s)" wording
    // read as a contradiction (it asserted both "NONE" and "1 of 2" for the same
    // count). The fixed wording must scope the sentence to the absent subset,
    // never assert a universal "NONE", and still carry every exact number/term.
    let msg = absent_terms_message(&["ex:ghost"], 2, "ex:from", "ex:to");
    assert!(
        !msg.contains("NONE"),
        "must not read as a universal claim over all requested terms: {msg:?}"
    );
    assert_eq!(
        msg,
        "# absent: 1 of 2 requested term(s) anchor no residue construct in ex:from or ex:to — those would move nothing: ex:ghost"
    );
}

#[test]
fn absent_terms_message_all_absent_still_reads_correctly() {
    // The degenerate case where every requested term is absent — `absent.len() ==
    // total_requested` — must still read correctly (this is the one case where
    // "NONE of the requested terms" would have been literally true, but the
    // scoped wording must not regress to a special-cased sentence for it).
    let msg = absent_terms_message(&["ex:ghost", "ex:phantom"], 2, "ex:from", "ex:to");
    assert_eq!(
        msg,
        "# absent: 2 of 2 requested term(s) anchor no residue construct in ex:from or ex:to — those would move nothing: ex:ghost, ex:phantom"
    );
}
