// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::ir::PreservationKind;

const SKOS_RELATED_MATCH: &str = "http://www.w3.org/2004/02/skos/core#relatedMatch";
const SKOS_EXACT_MATCH: &str = "http://www.w3.org/2004/02/skos/core#exactMatch";
const OWL_EQUIVALENT_CLASS: &str = "http://www.w3.org/2002/07/owl#equivalentClass";

fn parse_nt(nt: &str) -> std::sync::Arc<purrdf::RdfDataset> {
    purrdf::parse_dataset(nt.as_bytes(), "application/n-triples", None)
        .expect("parse projection N-Triples")
}

/// The §14 worked example projects to a `skos:relatedMatch` alignment surface and
/// NEVER to `skos:exactMatch` / `owl:equivalentClass` — and it carries a loss-ledger
/// row declaring its under-approximation polarity.
#[test]
fn affine_triangle_projects_related_match_never_equivalence() {
    let program = affine_triangle_worked_example();
    let nt = project_correspondence(&program);

    // Check the alignment PREDICATE position (`<...>` as a predicate IRI), not a bare
    // substring — the loss-ledger prose names the forbidden predicates as disclosure.
    assert!(
        nt.contains(&format!("<{SKOS_RELATED_MATCH}>")),
        "the affine overlap MUST surface a skos:relatedMatch edge:\n{nt}"
    );
    assert!(
        !nt.contains(&format!("<{SKOS_EXACT_MATCH}>")),
        "a caveated affine overlap MUST NOT surface a skos:exactMatch edge:\n{nt}"
    );
    assert!(
        !nt.contains(&format!("<{OWL_EQUIVALENT_CLASS}>")),
        "a caveated affine overlap MUST NOT surface an owl:equivalentClass edge:\n{nt}"
    );
    // The loss-ledger row: the lane declares its preservation polarity, never silent.
    assert!(
        nt.contains(&PreservationKind::SoundUnder.iri()),
        "the lane MUST declare its SoundUnderApproximation preservation polarity:\n{nt}"
    );
    assert!(
        nt.contains("lossyDrop"),
        "the lane MUST carry an explicit loss-ledger row:\n{nt}"
    );
    // The §14 axes are present.
    assert!(nt.contains("AffineCorrespondence"), "morphism class");
    assert!(nt.contains("Overlaps"), "relation");
    assert!(nt.contains("Vague"), "determinacy");
    assert!(nt.contains("0.72"), "confidence");
    assert!(nt.contains("not equivalent"), "the caveat text");
}

/// `xsd:decimal` lexical space does not allow scientific notation. Values whose
/// shortest Rust `f64` display would use an exponent are expanded before projection.
#[test]
fn decimal_projection_expands_scientific_notation() {
    use crate::ir::{CorrespondenceRelation, MorphismClass, MorphismKind};

    let correspondence = Correspondence::new(
        "https://blackcatinformatics.ca/gmeow/example/decimalProjection",
        CorrespondenceRelation::RelatedMatch,
        MorphismClass::BridgeView,
        MorphismKind::CommitmentShiftingBridge,
        false,
        None,
        None,
        None,
        vec![],
        Some(1e-5),
        None,
        Some(1e20),
        None,
        None,
        None,
    )
    .expect("valid finite decimal correspondence");
    let program =
        CorrespondenceProgram::new(vec![correspondence], vec![], PreservationKind::SoundUnder);
    let nt = project_correspondence(&program);

    assert!(
        nt.contains(&format!("\"0.00001\"^^<{XSD_DECIMAL}>")),
        "small decimal must be fixed-form xsd:decimal:\n{nt}"
    );
    assert!(
        nt.contains(&format!("\"100000000000000000000\"^^<{XSD_DECIMAL}>")),
        "large decimal must be fixed-form xsd:decimal:\n{nt}"
    );
    let decimal_lexicals: Vec<&str> = nt
        .lines()
        .filter(|line| line.contains(XSD_DECIMAL))
        .filter_map(|line| line.split('"').nth(1))
        .collect();
    assert!(
        decimal_lexicals
            .iter()
            .all(|lexical| !lexical.contains('e') && !lexical.contains('E')),
        "xsd:decimal lexicals must not use exponent notation: {decimal_lexicals:?}"
    );

    let dataset = parse_nt(&nt);
    let re_derived = parse_correspondence(&dataset).expect("re-derive fixed decimals");
    let got = &re_derived.correspondences[0];
    assert_eq!(got.confidence, Some(1e-5));
    assert_eq!(got.weight, Some(1e20));
}

/// The overclaim gate REJECTS an attempt to emit a class equivalence for the §14
/// affine/overlaps correspondence (a BUILD FAILURE), but ALLOWS the relation-sound
/// related-match surface.
#[test]
fn overclaim_gate_rejects_equivalence_for_affine_overlap() {
    let program = affine_triangle_worked_example();
    let correspondence = &program.correspondences[0];

    // Asking for equivalence is an overclaim → hard error.
    let err = assert_no_overclaim_correspondence(correspondence, true)
        .expect_err("emitting equivalence for a caveated affine overlap must HARD-fail");
    assert!(
        err.0.contains("Overclaim"),
        "the error must name the overclaim: {err}"
    );

    // NOT asking for equivalence is fine (the related-match surface).
    assert_no_overclaim_correspondence(correspondence, false)
        .expect("the related-match surface is not an overclaim");
}

/// A genuine satisfaction-preserving `Equiv` MAY emit equivalence (the gate is not a
/// blanket ban — it is relation-sound).
#[test]
fn overclaim_gate_allows_equivalence_for_true_equiv() {
    use crate::ir::{CorrespondenceRelation, MorphismClass, MorphismKind};
    let equiv = Correspondence::new(
        "https://blackcatinformatics.ca/gmeow/example/equiv",
        CorrespondenceRelation::Equiv,
        MorphismClass::Isomorphism,
        MorphismKind::InstitutionMorphism,
        false,
        None,
        None,
        None,
        vec![],
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .expect("valid");
    assert_no_overclaim_correspondence(&equiv, true)
        .expect("a true satisfaction-preserving equivalence MAY emit equivalence");
}

/// A commitment-shifting bridge over an `Equiv` relation is STILL refused equivalence
/// (the loss ledger refuses owl:equivalentClass for a by-reference bridge).
#[test]
fn overclaim_gate_refuses_equivalence_for_commitment_shifting_bridge() {
    use crate::ir::{CorrespondenceRelation, MorphismClass, MorphismKind};
    let bridge = Correspondence::new(
        "https://blackcatinformatics.ca/gmeow/example/bridge",
        CorrespondenceRelation::Equiv,
        MorphismClass::BridgeView,
        MorphismKind::CommitmentShiftingBridge,
        false,
        None,
        None,
        None,
        vec![],
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .expect("valid");
    assert_no_overclaim_correspondence(&bridge, true)
        .expect_err("a commitment-shifting bridge must be refused equivalence");
}

/// Round-trip: project → parse re-derives a content-key-equal program (the cache-hit
/// identity the typed handle relies on).
#[test]
fn projection_round_trips_to_equal_content_key() {
    let program = affine_triangle_worked_example();
    let nt = project_correspondence(&program);
    let dataset = parse_nt(&nt);
    let re_derived = parse_correspondence(&dataset).expect("re-derive correspondence program");
    assert_eq!(
        program.content_key(),
        re_derived.content_key(),
        "the cache re-derivation yields a content-key-equal correspondence program"
    );
    // And re-projecting the re-derived program is byte-identical (idempotent).
    assert_eq!(
        project_correspondence(&re_derived),
        nt,
        "re-projecting the re-derived program is byte-identical"
    );
}

/// Fidelity oracle for the dogfooded affine cell: the hand-authored
/// `slices/grounding/logic/examples/affine-correspondence.ttl` re-derives (via
/// `parse_correspondence`, the cache-hit inverse the production lane now uses) to the
/// EXACT same [`CorrespondenceProgram`] as the `affine_triangle_worked_example` Rust
/// literal — so its `project_correspondence` is byte-identical and `graph/correspondence`
/// keeps byte-parity now that the stage reads the authored TTL instead of the literal.
#[test]
fn authored_affine_cell_matches_worked_example_oracle() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../slices/grounding/logic/examples/affine-correspondence.ttl");
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read authored affine cell {path:?}: {e}"));
    let dataset = purrdf::parse_dataset(source.as_bytes(), "text/turtle", None)
        .expect("parse authored affine correspondence cell");
    let authored = parse_correspondence(&dataset).expect("re-derive authored affine program");

    let oracle = affine_triangle_worked_example();

    // (a) Field-for-field program identity (both types derive PartialEq).
    assert_eq!(
        authored, oracle,
        "the authored affine cell must re-derive to the worked-example program"
    );

    // (b) Byte-parity of the backing projection: the authored program projects to the
    // SAME `graph/correspondence` N-Triples as the former hardcoded literal.
    assert_eq!(
        project_correspondence(&authored),
        project_correspondence(&oracle),
        "the authored affine cell must project byte-identically to graph/correspondence"
    );

    // (c) Round-trip: projecting the worked example and re-parsing its N-Triples yields
    // the same program (the cache-hit inverse), which the authored cell also equals.
    let reparsed = parse_correspondence(&parse_nt(&project_correspondence(&oracle)))
        .expect("re-derive the projected worked example");
    assert_eq!(
        reparsed, oracle,
        "project → parse must round-trip the worked-example program"
    );
    assert_eq!(
        authored, reparsed,
        "the authored cell and the projected round-trip must be the same program"
    );
}

/// The projection is deterministic (sorted, byte-stable across runs).
#[test]
fn projection_is_byte_deterministic() {
    let a = project_correspondence(&affine_triangle_worked_example());
    let b = project_correspondence(&affine_triangle_worked_example());
    assert_eq!(a, b, "the projection must be byte-deterministic");
    // Sorted: every non-empty line ends with " ." and the body is sorted.
    let lines: Vec<&str> = a.lines().filter(|l| !l.is_empty()).collect();
    let mut sorted = lines.clone();
    sorted.sort();
    assert_eq!(lines, sorted, "the projection lines must be sorted");
    assert!(
        lines.iter().all(|l| l.ends_with(" .")),
        "every triple line ends with ' .'"
    );
}
