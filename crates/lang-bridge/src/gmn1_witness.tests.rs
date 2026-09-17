// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use purrdf::{RdfLiteral, RdfQuad, RdfTerm};

use super::*;
use crate::gmn1_codec::GmnDictionary;

use gmeow_ns::GMEOW_NS;

fn empty_dict() -> GmnDictionary {
    GmnDictionary::default()
}

fn iri(local: &str) -> RdfTerm {
    RdfTerm::Iri(format!("{GMEOW_NS}{local}"))
}

/// A multi-claim model with (a) two ground IRI claims, (b) a claim whose blank subject
/// is closed within itself, and (c) two claims that SHARE a blank (an n-ary-reification
/// idiom: a reifier blank referenced by two distinct subjects).
fn multi_claim_model() -> Gmn0Model {
    let quads = vec![
        // Ground claim `gate1`.
        RdfQuad::new(iri("gate1"), format!("{GMEOW_NS}hasState"), iri("open")),
        // Ground claim `gate2`.
        RdfQuad::new(iri("gate2"), format!("{GMEOW_NS}hasState"), iri("closed")),
        // Blank-closed claim: `_:selfClosed` names only ground objects, referenced by
        // no other subject.
        RdfQuad::new(
            RdfTerm::blank_node("selfClosed"),
            format!("{GMEOW_NS}hasState"),
            iri("open"),
        ),
        // Two claims that SHARE the blank `_:shared`: `subjA` and `subjB` both point at
        // it, so neither is blank-closed.
        RdfQuad::new(
            iri("subjA"),
            format!("{GMEOW_NS}relatesTo"),
            RdfTerm::blank_node("shared"),
        ),
        RdfQuad::new(
            iri("subjB"),
            format!("{GMEOW_NS}relatesTo"),
            RdfTerm::blank_node("shared"),
        ),
        // The shared blank is itself a subject with a ground object.
        RdfQuad::new(
            RdfTerm::blank_node("shared"),
            format!("{GMEOW_NS}hasState"),
            iri("open"),
        ),
    ];
    model_from_quads(&quads)
}

#[test]
fn per_claim_equality_holds_on_a_multi_claim_blank_bearing_model() {
    let model = multi_claim_model();
    let dict = empty_dict();
    // Sanity: the model genuinely carries a blank shared across subjects.
    assert!(
        model
            .quads
            .iter()
            .filter(|q| matches!(&q.object, RdfTerm::BlankNode(l) if l == "shared"))
            .count()
            >= 2,
        "fixture must exercise a blank shared across subjects"
    );
    per_claim_round_trip_check(&model, &dict)
        .expect("every claim round-trips, including the blank-bearing and blank-shared claims");
    round_trip_with_claims_check(&model, &dict)
        .expect("combined witness preserves whole-model blank identity and claim partitions");
}

#[test]
fn corrupted_single_claim_reds_only_that_claim() {
    // Two hand-built canonical N-Quads partitions differing in EXACTLY one subject's
    // object. This drives the partition/compare primitive directly (no real codec
    // mismatch is forcible — the codec round-trips), isolating the localization.
    let original = "\
<https://blackcatinformatics.ca/gmeow/gate1> <https://blackcatinformatics.ca/gmeow/hasState> <https://blackcatinformatics.ca/gmeow/open> .
<https://blackcatinformatics.ca/gmeow/gate2> <https://blackcatinformatics.ca/gmeow/hasState> <https://blackcatinformatics.ca/gmeow/closed> .
";
    // gate2's object is perturbed; gate1 is byte-identical.
    let reconstructed = "\
<https://blackcatinformatics.ca/gmeow/gate1> <https://blackcatinformatics.ca/gmeow/hasState> <https://blackcatinformatics.ca/gmeow/open> .
<https://blackcatinformatics.ca/gmeow/gate2> <https://blackcatinformatics.ca/gmeow/hasState> <https://blackcatinformatics.ca/gmeow/ajar> .
";
    let error = compare_claim_partitions(original, reconstructed)
        .expect_err("a perturbed claim must red the witness");
    assert_eq!(
        error,
        Gmn1Error::PerClaimMismatch {
            subject: "<https://blackcatinformatics.ca/gmeow/gate2>".to_owned(),
        },
        "the mismatch must NAME the offending canonical subject, and only it"
    );
    assert_eq!(
        error.failure_class(),
        "https://blackcatinformatics.ca/lang/GmnNonDecodableGrammar",
        "a per-claim mismatch reuses the whole-model round-trip class"
    );
}

#[test]
fn missing_claim_subject_is_named() {
    // A subject present on one side only is a key-set mismatch that names it.
    let original = "\
<https://blackcatinformatics.ca/gmeow/gate1> <https://blackcatinformatics.ca/gmeow/hasState> <https://blackcatinformatics.ca/gmeow/open> .
<https://blackcatinformatics.ca/gmeow/gate2> <https://blackcatinformatics.ca/gmeow/hasState> <https://blackcatinformatics.ca/gmeow/closed> .
";
    let reconstructed = "\
<https://blackcatinformatics.ca/gmeow/gate1> <https://blackcatinformatics.ca/gmeow/hasState> <https://blackcatinformatics.ca/gmeow/open> .
";
    let error = compare_claim_partitions(original, reconstructed)
        .expect_err("a dropped claim must red the witness");
    assert_eq!(
        error,
        Gmn1Error::PerClaimMismatch {
            subject: "<https://blackcatinformatics.ca/gmeow/gate2>".to_owned(),
        }
    );
}

#[test]
fn partition_reads_iri_blank_and_triple_term_subjects() {
    let nquads = "\
<https://blackcatinformatics.ca/gmeow/gate1> <https://blackcatinformatics.ca/gmeow/hasState> <https://blackcatinformatics.ca/gmeow/open> .
_:c14n0 <https://blackcatinformatics.ca/gmeow/hasState> <https://blackcatinformatics.ca/gmeow/open> .
<<( <https://blackcatinformatics.ca/gmeow/s> <https://blackcatinformatics.ca/gmeow/p> <https://blackcatinformatics.ca/gmeow/o> )>> <https://blackcatinformatics.ca/gmeow/note> <https://blackcatinformatics.ca/gmeow/n1> .
";
    let partition = partition_by_subject(nquads);
    assert_eq!(partition.len(), 3, "three distinct canonical subjects");
    assert!(partition.contains_key("<https://blackcatinformatics.ca/gmeow/gate1>"));
    assert!(partition.contains_key("_:c14n0"));
    assert!(partition.contains_key(
            "<<( <https://blackcatinformatics.ca/gmeow/s> <https://blackcatinformatics.ca/gmeow/p> <https://blackcatinformatics.ca/gmeow/o> )>>"
        ));
}

#[test]
fn idempotence_holds_on_a_by_reference_literal_document() {
    // A language-tagged literal rides BY REFERENCE (an `r_<hash>` token + a refs-table
    // payload), exercising the reference table in the idempotence comparison.
    let quads = vec![RdfQuad::new(
        iri("gate1"),
        format!("{GMEOW_NS}label"),
        RdfTerm::Literal(RdfLiteral::language_tagged("porte", "fr")),
    )];
    let model = model_from_quads(&quads);
    let dict = empty_dict();

    let doc = gmn1_write(&model, &dict).expect("by-reference document writes");
    assert!(
        doc.text.contains("r_"),
        "the langString must ride by reference (r_<hash> token): {}",
        doc.text
    );
    idempotence_check(&doc, &dict).expect("a writer-produced document re-encodes identically");
    // And the whole witness (which includes the idempotence leg) is green.
    per_claim_round_trip_check(&model, &dict)
        .expect("the by-reference model passes the full witness");
    round_trip_with_claims_check(&model, &dict)
        .expect("combined witness retains the reference-table idempotence check");
}

#[test]
fn standalone_checks_ground_and_blank_closed_but_skips_shared() {
    let model = multi_claim_model();
    let dict = empty_dict();
    let report =
        per_claim_standalone_check(&model, &dict).expect("no blank-closed claim mis-inverts");

    // The ground claims and the self-closed blank claim are checked standalone.
    assert!(
        report
            .checked
            .contains(&"<https://blackcatinformatics.ca/gmeow/gate1>".to_owned()),
        "a ground claim round-trips standalone: {report:?}"
    );
    assert!(
        report.checked.contains(&"_:selfClosed".to_owned()),
        "a blank-closed claim round-trips standalone: {report:?}"
    );

    // The two subjects sharing `_:shared`, and the shared blank's own claim, are
    // SKIPPED — never falsely failed.
    for shared in [
        "<https://blackcatinformatics.ca/gmeow/subjA>",
        "<https://blackcatinformatics.ca/gmeow/subjB>",
        "_:shared",
    ] {
        assert!(
            report.skipped.contains(&shared.to_owned()),
            "a blank-shared claim must be SKIPPED, not checked: {shared} in {report:?}"
        );
        assert!(
            !report.checked.contains(&shared.to_owned()),
            "a blank-shared claim must NOT be checked standalone: {shared} in {report:?}"
        );
    }
}

#[test]
fn partition_is_insensitive_to_input_line_order() {
    // Two byte-permutations of the same claim's lines partition and digest identically,
    // because the partition sorts each claim's lines into canonical order.
    let forward = partition_by_subject("_:x <p> <o1> .\n_:x <p> <o2> .\n");
    let reversed = partition_by_subject("_:x <p> <o2> .\n_:x <p> <o1> .\n");
    assert_eq!(forward, reversed, "partition is insensitive to line order");
    let (subject, lines) = forward.iter().next().expect("one claim");
    assert_eq!(subject, "_:x");
    assert_eq!(claim_digest(lines), claim_digest(&reversed[subject]));
}
