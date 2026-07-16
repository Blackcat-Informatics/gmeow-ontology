// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::ir::{Formula, PreservationKind, RecoveryCaseIr, Term};

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

/// Recovery evidence has one canonical RDF home under its correspondence.  Its complete
/// quantified transform survives project → parse → project without also becoming a
/// top-level formula assertion.
#[test]
fn recovery_case_formula_round_trips_byte_identically() {
    use crate::ir::{CorrespondenceRelation, MorphismClass, MorphismKind};

    let atom = |relation: &str, object: &str| {
        Formula::atom(
            Term::iri(relation).expect("relation IRI"),
            vec![
                Term::var("source").expect("source variable"),
                Term::iri(object).expect("endpoint IRI"),
            ],
        )
        .expect("binary atom")
    };
    let transform = Formula::Forall {
        vars: vec!["source".to_owned()],
        body: Box::new(Formula::Implies(
            Box::new(atom(
                "https://example.org/sourceKind",
                "https://example.org/Language",
            )),
            Box::new(atom(
                "https://example.org/viewKind",
                "https://example.org/SignSystem",
            )),
        )),
    };
    let case = RecoveryCaseIr::new("https://example.org/recovery/language", transform)
        .expect("valid recovery case");
    let correspondence = Correspondence::new(
        "https://example.org/correspondence/language",
        CorrespondenceRelation::SubsumedBy,
        MorphismClass::SectionRetraction,
        MorphismKind::InstitutionMorphism,
        true,
        None,
        Some("https://example.org/get".to_owned()),
        Some("https://example.org/put".to_owned()),
        vec![],
        None,
        None,
        None,
        None,
        None,
        Some(PreservationKind::Exact),
    )
    .expect("valid correspondence")
    .with_recovery_cases(vec![case])
    .expect("unique recovery case");
    let program = CorrespondenceProgram::new(vec![correspondence], vec![], PreservationKind::Exact);

    let nt = project_correspondence(&program);
    assert!(
        nt.contains("RecoveryCase"),
        "case type must be projected:\n{nt}"
    );
    assert!(
        nt.contains("recoveryTransform"),
        "formula ownership edge must be projected:\n{nt}"
    );
    let dataset = parse_nt(&nt);
    let re_derived = parse_correspondence(&dataset).expect("re-derive recovery case");
    assert_eq!(program.content_key(), re_derived.content_key());
    assert_eq!(project_correspondence(&re_derived), nt);

    let duplicate_transform = format!(
        "{nt}<https://example.org/recovery/language> <{}> <https://example.org/recovery/second-transform> .\n",
        p_recovery_transform()
    );
    let duplicate_dataset = parse_nt(&duplicate_transform);
    let error = parse_correspondence(&duplicate_dataset)
        .expect_err("a recovery case with two transforms must hard-fail");
    assert!(error.message().contains("exactly one"), "{error}");

    let untyped = nt
        .lines()
        .filter(|line| {
            !(line.starts_with("<https://example.org/recovery/language> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type>")
                && line.contains("RecoveryCase"))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let untyped_dataset = parse_nt(&untyped);
    let error = parse_correspondence(&untyped_dataset)
        .expect_err("an untyped recovery case must hard-fail");
    assert!(error.message().contains("not typed"), "{error}");
}

/// [`CorrespondenceOwnership`] must not over-capture: a resource whose IRI merely shares a
/// recovery case's IRI as a *string* prefix — but is not a genuine `recoveryTransform`
/// descendant minted by [`project_correspondence`] — is NEVER treated as correspondence-owned.
/// Regression for the dialect-writer gap where an "exact-or-slash-prefix" ownership test
/// classified any slash-suffixed sibling of a recovery-case IRI as owned, silently dropping its
/// axioms from the generated CGIF/CLIF/XCL dialects. The case's genuine structural children
/// (the case node itself and its `/transform` formula tree) are still correctly owned.
#[test]
fn correspondence_ownership_does_not_over_capture_unrelated_slash_children() {
    use crate::ir::{CorrespondenceRelation, MorphismClass, MorphismKind};

    let case_iri = "https://example.org/recovery/case1".to_owned();
    let transform = Formula::atom(
        Term::iri("https://example.org/holds").expect("relation IRI"),
        vec![Term::iri("https://example.org/a").expect("endpoint IRI")],
    )
    .expect("ground atom");
    let case = RecoveryCaseIr::new(case_iri.clone(), transform).expect("valid recovery case");
    let correspondence = Correspondence::new(
        "https://example.org/correspondence/case1owner",
        CorrespondenceRelation::SubsumedBy,
        MorphismClass::SectionRetraction,
        MorphismKind::InstitutionMorphism,
        true,
        None,
        None,
        None,
        vec![],
        None,
        None,
        None,
        None,
        None,
        Some(PreservationKind::Exact),
    )
    .expect("valid correspondence")
    .with_recovery_cases(vec![case])
    .expect("unique recovery case");

    let ownership = CorrespondenceOwnership::build(std::slice::from_ref(&correspondence));

    // Genuine structural children ARE owned.
    assert!(
        ownership.owns(&case_iri),
        "the recovery case node itself must be owned"
    );
    assert!(
        ownership.owns(&format!("{case_iri}/transform")),
        "the mint root of the recoveryTransform formula tree must be owned"
    );
    assert!(
        ownership.owns(&format!("{case_iri}/transform/body")),
        "a genuine descendant under the transform mint root must be owned"
    );

    // An unrelated resource that shares the case IRI as a string prefix but is NOT a
    // `/transform` descendant must NOT be swept up.
    assert!(
        !ownership.owns(&format!("{case_iri}extra")),
        "a sibling IRI sharing the case IRI as a bare string prefix must not be owned"
    );
    assert!(
        !ownership.owns(&format!("{case_iri}/unrelated-sibling")),
        "a slash-suffixed sibling that is not a recoveryTransform descendant must not be owned"
    );
    assert!(
        !ownership.owns(&format!("{case_iri}/../other")),
        "a path-traversal-style sibling must not be owned"
    );
}

/// End-to-end regression: a program carrying a recovery case PLUS an unrelated axiom whose
/// subject is slash-suffixed under the case IRI must project both the case's own recovery
/// evidence AND retain the unrelated axiom as a flat axiom in the meta channel — the writer
/// must not silently drop it as "correspondence-owned".
#[test]
fn writer_meta_channel_retains_axiom_sharing_case_iri_as_slash_prefix() {
    use crate::ir::{
        ContextualScope, CorrespondenceRelation, LogicAxiom, LogicProgram, MorphismClass,
        MorphismKind,
    };

    let case_iri = "https://example.org/recovery/case1".to_owned();
    let transform = Formula::atom(
        Term::iri("https://example.org/holds").expect("relation IRI"),
        vec![Term::iri("https://example.org/a").expect("endpoint IRI")],
    )
    .expect("ground atom");
    let case = RecoveryCaseIr::new(case_iri.clone(), transform).expect("valid recovery case");
    let correspondence = Correspondence::new(
        "https://example.org/correspondence/case1owner",
        CorrespondenceRelation::SubsumedBy,
        MorphismClass::SectionRetraction,
        MorphismKind::InstitutionMorphism,
        true,
        None,
        None,
        None,
        vec![],
        None,
        None,
        None,
        None,
        None,
        Some(PreservationKind::Exact),
    )
    .expect("valid correspondence")
    .with_recovery_cases(vec![case])
    .expect("unique recovery case");

    // A domain resource that is NOT part of the recovery-case structure, but happens to be
    // named as a slash-child of the case IRI (a plausible naming collision in a large corpus).
    let sibling_subject = format!("{case_iri}/unrelated-sibling");
    let sibling_axiom = LogicAxiom::new(
        sibling_subject.clone(),
        "https://example.org/marker".to_owned(),
        "true".to_owned(),
        true,
        false,
        ContextualScope::default(),
    )
    .expect("valid axiom");

    let program = LogicProgram::new(vec![sibling_axiom], vec![], vec![], None)
        .with_correspondences(vec![correspondence])
        .expect("unique correspondence");

    let cgif = crate::cgif::project_cgif(&program)
        .expect("project_cgif")
        .content;
    assert!(
        cgif.contains(&sibling_subject),
        "the unrelated sibling axiom must survive into the CGIF meta channel, not be dropped \
         as correspondence-owned:\n{cgif}"
    );

    let clif = crate::clif::project_clif(&program)
        .expect("project_clif")
        .content;
    assert!(
        clif.contains(&sibling_subject),
        "the unrelated sibling axiom must survive into the CLIF meta channel, not be dropped \
         as correspondence-owned:\n{clif}"
    );

    let xcl = crate::xcl::project_xcl(&program)
        .expect("project_xcl")
        .content;
    assert!(
        xcl.contains(&sibling_subject),
        "the unrelated sibling axiom must survive into the XCL meta channel, not be dropped \
         as correspondence-owned:\n{xcl}"
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

/// Standpoint indexing has one canonical RDF spelling across the ontology and the
/// correspondence carrier: the declared `gmeow:accordingTo` annotation property.
#[test]
fn standpoint_index_round_trips_through_canonical_gmeow_property() {
    use crate::ir::{CorrespondenceRelation, MorphismClass, MorphismKind};

    let standpoint = "https://blackcatinformatics.ca/gmeow/example/curatorStandpoint".to_owned();
    let correspondence = Correspondence::new(
        "https://blackcatinformatics.ca/gmeow/example/standpointIndexedCorrespondence",
        CorrespondenceRelation::RelatedMatch,
        MorphismClass::AffineCorrespondence,
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
        Some(standpoint.clone()),
        None,
    )
    .expect("standpoint-indexed correspondence");
    let program = CorrespondenceProgram::new(vec![correspondence], vec![], PreservationKind::Exact);
    let nt = project_correspondence(&program);

    assert!(
        nt.contains(&format!("<{GMEOW_NAMESPACE}accordingTo>")),
        "the carrier must emit the declared gmeow:accordingTo property:\n{nt}"
    );
    assert!(
        !nt.contains(&format!("<{LOGIC_NAMESPACE}accordingTo>")),
        "the undeclared legacy logic:accordingTo spelling must not be emitted:\n{nt}"
    );

    let dataset = parse_nt(&nt);
    let re_derived = parse_correspondence(&dataset).expect("re-derive standpoint index");
    assert_eq!(re_derived.correspondences[0].according_to, Some(standpoint));
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
