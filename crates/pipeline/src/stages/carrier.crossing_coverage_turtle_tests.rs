// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_validate::slice_peerage::CrossingCoverage;
use purrdf::slice::NamedNode;

fn nn(iri: &str) -> NamedNode {
    NamedNode::new_unchecked(iri)
}

fn crossing(from: &str, to: &str, seam: &str, term: &str) -> CrossingCoverage {
    CrossingCoverage {
        from_slice: from.to_string(),
        to_slice: to.to_string(),
        seam_iri: seam.to_string(),
        term: nn(term),
    }
}

const LANG: &str = "https://blackcatinformatics.ca/gmeow/slices/lang";
const LOGIC: &str = "https://blackcatinformatics.ca/gmeow/slices/logic";
const SEAM: &str = "https://blackcatinformatics.ca/gmeow/seam/test-seam";
const TERM: &str = "https://blackcatinformatics.ca/logic/Foo";

/// No covered crossings → no output (no-optionality: absence of data is
/// absence of output, never an empty placeholder record).
#[test]
fn empty_input_yields_empty_turtle() {
    assert_eq!(crossing_coverage_turtle(&[]), "");
}

/// One covered crossing emits exactly one `gmeow:crossingCoverage` record
/// with the right `coveredEdgeFrom`/`coveredEdgeTo`/`coveringSeam`/`coveredTerm`
/// full-IRI triples, and the record is a valid Turtle predicate-object list
/// (a stable `_:cov0` blank-node subject).
#[test]
fn one_covered_crossing_emits_the_expected_record() {
    let body = crossing_coverage_turtle(&[crossing(LANG, LOGIC, SEAM, TERM)]);
    assert!(body.contains("_:cov0"));
    assert!(body.contains("a <https://blackcatinformatics.ca/gmeow/crossingCoverage> ;"));
    assert!(body.contains(&format!(
        "<https://blackcatinformatics.ca/gmeow/coveredEdgeFrom> <{LANG}> ;"
    )));
    assert!(body.contains(&format!(
        "<https://blackcatinformatics.ca/gmeow/coveredEdgeTo> <{LOGIC}> ;"
    )));
    assert!(body.contains(&format!(
        "<https://blackcatinformatics.ca/gmeow/coveringSeam> <{SEAM}> ;"
    )));
    assert!(body.contains(&format!(
        "<https://blackcatinformatics.ca/gmeow/coveredTerm> <{TERM}> ."
    )));
}

/// Records are sorted by `(from, to, seam, term)` BEFORE blank-node index
/// assignment, so the emitted bytes are independent of the input `Vec`'s
/// order — feeding the two crossings in reverse-sorted order still assigns
/// `_:cov0` to the lexically-first record.
#[test]
fn records_are_sorted_before_index_assignment() {
    let first = crossing(LANG, LOGIC, SEAM, TERM);
    let second = crossing(
        "https://blackcatinformatics.ca/gmeow/slices/math",
        LOGIC,
        SEAM,
        TERM,
    );
    let forward = crossing_coverage_turtle(&[first.clone(), second.clone()]);
    let reversed = crossing_coverage_turtle(&[second, first]);
    assert_eq!(
        forward, reversed,
        "input order must not affect the emitted byte sequence"
    );
    let cov0_pos = forward.find("_:cov0").unwrap();
    let lang_pos = forward.find(LANG).unwrap();
    assert!(
        lang_pos > cov0_pos,
        "the lexically-first from_slice ({LANG}) must sort into _:cov0"
    );
}

/// An exact duplicate `(from, to, seam, term)` record is deduplicated — the
/// classifier can never actually emit one (each `(edge, term)` join is
/// unique in `PeerageClassification::crossings`), but the helper does not
/// trust that upstream invariant silently.
#[test]
fn duplicate_records_are_deduplicated() {
    let body = crossing_coverage_turtle(&[
        crossing(LANG, LOGIC, SEAM, TERM),
        crossing(LANG, LOGIC, SEAM, TERM),
    ]);
    assert_eq!(body.matches("_:cov").count(), 1, "{body}");
}

/// The emitted snippet, concatenated onto a realistic `graph/slice-analysis`
/// Turtle body (with its own `@prefix` block and a `_:dep0` record), still
/// parses as ONE valid Turtle document — the full-IRI snippet never depends
/// on the preceding body's prefixes, and blank-node labels never collide.
#[test]
fn concatenated_onto_a_prefixed_body_still_parses() {
    let existing_body = format!(
        "@prefix gmeow: <{GMEOW_NS}> .\n\
             @prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .\n\
             @prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .\n\
             \n\
             _:dep0\n\
             \x20   a <{GMEOW_NS}computedSliceDependency> ;\n\
             \x20   <{GMEOW_NS}dependencyFromSlice> <{LANG}> ;\n\
             \x20   <{GMEOW_NS}dependencyToSlice> <{LOGIC}> .\n\
             \n"
    );
    let mut combined = existing_body;
    combined.push_str(&crossing_coverage_turtle(&[crossing(
        LANG, LOGIC, SEAM, TERM,
    )]));

    let nq = turtle_to_nquads(combined.as_bytes())
        .expect("the concatenated body must still parse as valid Turtle");
    let text = String::from_utf8(nq).expect("utf8");
    assert!(
        text.contains("crossingCoverage"),
        "the coverage record must survive the parse+N-Quads round-trip: {text}"
    );
    assert!(
        text.contains("computedSliceDependency"),
        "the pre-existing dependency record must also survive: {text}"
    );
}
