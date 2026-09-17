// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_validate::slice_peerage::SeamRecord;
use std::collections::BTreeSet;

const LANG: &str = "https://blackcatinformatics.ca/gmeow/slices/lang";
const LOGIC: &str = "https://blackcatinformatics.ca/gmeow/slices/logic";
const MATH: &str = "https://blackcatinformatics.ca/gmeow/slices/math";

fn set(values: &[&str]) -> BTreeSet<String> {
    values.iter().map(|v| (*v).to_string()).collect()
}

fn seam(iri: &str, label: &str, directions: &[(&str, &str)]) -> SeamRecord {
    SeamRecord {
        iri: iri.to_string(),
        name: label.to_string(),
        labels: vec![(label.to_string(), Some("x-gmeow-english".to_string()))],
        carrying_terms: set(&["logic:Foo"]),
        carrying_term_iris: set(&["https://blackcatinformatics.ca/logic/Foo"]),
        directions: directions
            .iter()
            .map(|(f, t)| ((*f).to_string(), (*t).to_string()))
            .collect(),
        owning_docs: set(&["TEST.md"]),
    }
}

/// An empty registry emits nothing (no-optionality: absence of data is absence of
/// output, never a placeholder record).
#[test]
fn empty_registry_yields_empty_turtle() {
    assert_eq!(grounding_seams_turtle(&[]), "");
}

/// DETERMINISM: the emitted bytes are a pure function of the seam SET, never of the
/// order the catalog yielded its records in — the emitter sorts seams by IRI and
/// assigns every `_:seamdir{i}` label from the globally sorted `(seam, from, to)` key
/// BEFORE emission. Catches a regression that emitted in iteration order (which would
/// churn `gmeow.gts` bytes on every run and break the cache/superset gates).
#[test]
fn emitted_bytes_are_independent_of_input_order() {
    let a = seam(
        "https://blackcatinformatics.ca/gmeow/seam/alpha",
        "Alpha seam",
        &[(LANG, LOGIC)],
    );
    let b = seam(
        "https://blackcatinformatics.ca/gmeow/seam/beta",
        "Beta seam",
        &[(MATH, LOGIC), (LANG, LOGIC)],
    );
    let forward = grounding_seams_turtle(&[a.clone(), b.clone()]);
    // Same input, run again: byte-identical (no hashed/interior-mutable state).
    assert_eq!(forward, grounding_seams_turtle(&[a.clone(), b.clone()]));
    // Reversed input: still byte-identical (the sort happens before emission).
    assert_eq!(
        forward,
        grounding_seams_turtle(&[b, a]),
        "input order must not affect the emitted byte sequence"
    );
    let alpha = forward.find("seam/alpha").expect("alpha emitted");
    let beta = forward.find("seam/beta").expect("beta emitted");
    assert!(
        alpha < beta,
        "seams must be emitted in IRI order:\n{forward}"
    );
}

/// ROUND-TRIP: the emitted body parses as valid Turtle and every authored field
/// survives into the N-Quads projection the carrier actually ingests — including a
/// seam with TWO direction legs, whose blank nodes must stay distinct and correctly
/// paired (the `correspondence-and-preservation` seam's real shape).
#[test]
fn emitted_turtle_round_trips_through_the_parser() {
    let mut two_legged = seam(
        "https://blackcatinformatics.ca/gmeow/seam/two-legged",
        "Two legged seam",
        &[(LANG, LOGIC), (MATH, LOGIC)],
    );
    two_legged.carrying_term_iris = set(&[
        "https://blackcatinformatics.ca/logic/Correspondence",
        "https://blackcatinformatics.ca/logic/preservationKind",
    ]);
    two_legged.owning_docs = set(&["LANG-TRANSLATION.md", "LOGIC-CORRESPONDENCE.md"]);

    let body = grounding_seams_turtle(&[two_legged]);
    let nq =
        turtle_to_nquads(body.as_bytes()).expect("the emitted seam registry must be valid Turtle");
    let text = String::from_utf8(nq).expect("utf8");

    for expected in [
        "<https://blackcatinformatics.ca/gmeow/Seam>",
        "\"Two legged seam\"",
        "<https://blackcatinformatics.ca/gmeow/seamCarryingTerm> <https://blackcatinformatics.ca/logic/Correspondence>",
        "<https://blackcatinformatics.ca/gmeow/seamCarryingTerm> <https://blackcatinformatics.ca/logic/preservationKind>",
        "<https://blackcatinformatics.ca/gmeow/seamOwningDoc> \"LANG-TRANSLATION.md\"",
        "<https://blackcatinformatics.ca/gmeow/seamOwningDoc> \"LOGIC-CORRESPONDENCE.md\"",
        "<https://blackcatinformatics.ca/gmeow/seamFromSlice> <https://blackcatinformatics.ca/gmeow/slices/lang>",
        "<https://blackcatinformatics.ca/gmeow/seamFromSlice> <https://blackcatinformatics.ca/gmeow/slices/math>",
        "<https://blackcatinformatics.ca/gmeow/seamToSlice> <https://blackcatinformatics.ca/gmeow/slices/logic>",
    ] {
        assert!(
            text.contains(expected),
            "the parsed registry must carry {expected}:\n{text}"
        );
    }
    assert_eq!(
        text.matches("<https://blackcatinformatics.ca/gmeow/seamDirection>")
            .count(),
        2,
        "both direction legs must survive as distinct blank nodes:\n{text}"
    );
    // The two legs are distinct blank nodes with distinct `from` slices.
    assert_eq!(
        text.matches("<https://blackcatinformatics.ca/gmeow/seamFromSlice>")
            .count(),
        2,
        "each leg keeps its own gmeow:seamFromSlice:\n{text}"
    );
}

/// A label's language tag survives emission (the authored seams are all
/// `@x-gmeow-english`), and an untagged label emits a plain literal.
#[test]
fn label_language_tags_are_preserved() {
    let mut untagged = seam(
        "https://blackcatinformatics.ca/gmeow/seam/untagged",
        "Untagged seam",
        &[(LANG, LOGIC)],
    );
    untagged.labels = vec![("Untagged seam".to_string(), None)];
    let tagged = seam(
        "https://blackcatinformatics.ca/gmeow/seam/tagged",
        "Tagged seam",
        &[(LANG, LOGIC)],
    );
    let body = grounding_seams_turtle(&[tagged, untagged]);
    assert!(
        body.contains("\"Tagged seam\"@x-gmeow-english"),
        "an authored language tag must survive:\n{body}"
    );
    assert!(
        body.contains("\"Untagged seam\" ;") || body.contains("\"Untagged seam\" ."),
        "an untagged label must emit a plain literal, never a fabricated tag:\n{body}"
    );
}

/// NON-VACUITY, over the REAL repository: the shipped `graph/grounding-seams` payload
/// built from the real slice catalog carries ALL NINE authored seams, each with its
/// real `rdfs:label`, its real owning design doc, and at least one direction leg.
/// This is the test that fails if the registry ever stops reaching the bundle — a
/// fixture-only suite would pass while `gmeow.gts` shipped nothing.
#[test]
fn the_real_registry_ships_all_nine_authored_seams() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root");
    let catalog = discover_slice_catalog(&root).expect("discover the slice catalog");
    let nq = build_grounding_seams(&catalog).expect("build graph/grounding-seams");
    let text = String::from_utf8(nq).expect("utf8");

    // (seam IRI local name, authored rdfs:label, an authored owning doc)
    const AUTHORED: [(&str, &str, &str); 9] = [
        ("denotation", "Denotation seam", "LANG-MEANING.md"),
        (
            "compilation",
            "Compilation seam",
            "MATHEMATICS-EXPRESSIONS.md",
        ),
        (
            "laws-and-boundaries",
            "Laws and boundaries seam",
            "MATHEMATICS-ANALYSIS-AND-GEOMETRY.md",
        ),
        (
            "correspondence-and-preservation",
            "Correspondence and preservation seam",
            "LOGIC-CORRESPONDENCE.md",
        ),
        ("rendering", "Rendering seam", "MATHEMATICS-EXPRESSIONS.md"),
        (
            "quantity",
            "Quantity seam",
            "MATHEMATICS-MEASURE-AND-DIMENSION.md",
        ),
        (
            "gmn-mathematical-plane",
            "GMN mathematical-plane seam",
            "LANG-GMN.md",
        ),
        (
            "quantity-boundary",
            "Quantity boundary seam",
            "LOGIC-CORRESPONDENCE.md",
        ),
        (
            "gmn-logical-plane-verification",
            "GMN logical-plane verification seam",
            "LANG-GMN.md",
        ),
    ];
    for (local, label, doc) in AUTHORED {
        let iri = format!("https://blackcatinformatics.ca/gmeow/seam/{local}");
        assert!(
            text.contains(&format!("<{iri}> <{RDF_TYPE}> <{GMEOW_NS}Seam>")),
            "the shipped registry must type <{iri}> as gmeow:Seam:\n{text}"
        );
        assert!(
            text.contains(&format!("\"{label}\"")),
            "the shipped registry must carry seam \"{label}\"'s authored rdfs:label"
        );
        assert!(
            text.contains(&format!("<{iri}> <{GMEOW_NS}seamOwningDoc> \"{doc}\"")),
            "seam \"{label}\" must carry its authored owning doc {doc}"
        );
        assert!(
            text.contains(&format!("<{iri}> <{GMEOW_NS}seamDirection>")),
            "seam \"{label}\" must carry at least one direction leg"
        );
    }
    // The registry is CLOSED at nine: an added or dropped seam is a governance change
    // that must be made deliberately, not discovered by drift.
    let seam_types = text
        .matches(&format!("<{RDF_TYPE}> <{GMEOW_NS}Seam>"))
        .count();
    assert_eq!(
        seam_types, 9,
        "the authored registry is the CLOSED set of nine sanctioned seams; got {seam_types}"
    );
    // Every leg is fully paired: as many seamToSlice as seamFromSlice assertions.
    assert_eq!(
        text.matches(&format!("<{GMEOW_NS}seamFromSlice>")).count(),
        text.matches(&format!("<{GMEOW_NS}seamToSlice>")).count(),
        "every direction leg must carry BOTH a from-slice and a to-slice:\n{text}"
    );
}

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
