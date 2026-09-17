// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn ds_of(ttl: &str) -> std::sync::Arc<RdfDataset> {
    let full = format!(
        "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
             @prefix gufo: <https://w3id.org/gufo#> .\n\
             {ttl}"
    );
    purrdf::parse_dataset(full.as_bytes(), "text/turtle", None)
        .expect("test fixture parses as valid Turtle")
}

fn alignment_vocab(prefix: &str, ns: &str) -> ProjectionVocabulary {
    ProjectionVocabulary {
        prefix: prefix.to_owned(),
        namespaces: vec![ns.to_owned()],
        subsumed_by: LOGIC_NS.to_owned(),
        owner: LOGIC_NS.to_owned(),
        count_kind: CountKind::TypedAxiom,
        default_ceiling: 0,
        preservation: "SoundUnderApproximation".to_owned(),
        alignment_predicates: vec![
            "http://www.w3.org/2000/01/rdf-schema#subClassOf".to_owned(),
            "http://www.w3.org/2002/07/owl#equivalentClass".to_owned(),
        ],
        counted_predicates: Vec::new(),
    }
}

#[test]
fn grounded_shape_not_counted_in_residue() {
    // The grounding target is a real logic: axiom construct (logic:Formula is in
    // the logic: namespace), so the shape is grounded and subtracted.
    let ds = ds_of(
        r#"
            gmeow:S a sh:NodeShape ; logic:formalizes logic:disjointGoals .
            logic:disjointGoals a logic:Formula .
            "#,
    );
    assert_eq!(residue(&ds, &shacl_vocab()), 0);
}

#[test]
fn back_ref_to_non_axiom_target_still_counts() {
    // logic:formalizes points at a target typed owl:Class — a plain class
    // declaration, NOT a logic: axiom / owl:AllDisjointClasses. Under the tightened
    // typed-grounding contract this does NOT ground the shape, so it is counted.
    let ds = ds_of(
        r#"
            gmeow:S a sh:NodeShape ; logic:formalizes gmeow:Goal .
            gmeow:Goal a owl:Class .
            "#,
    );
    assert_eq!(residue(&ds, &shacl_vocab()), 1);
}

#[test]
fn back_ref_to_named_disjointness_axiom_grounds() {
    // A shape may formalize a named owl:AllDisjointClasses axiom (the one
    // non-logic:-namespaced grounding-target type) — grounded, not counted.
    let ds = ds_of(
        r#"
            gmeow:S a sh:NodeShape ; logic:formalizes gmeow:identityDisjointness .
            gmeow:identityDisjointness a owl:AllDisjointClasses .
            "#,
    );
    assert_eq!(residue(&ds, &shacl_vocab()), 0);
}

#[test]
fn ungrounded_shape_counted_in_residue() {
    let ds = ds_of(
        r#"
            gmeow:S a sh:NodeShape .
            "#,
    );
    assert_eq!(residue(&ds, &shacl_vocab()), 1);
}

#[test]
fn dangling_back_ref_still_counts() {
    // logic:formalizes points at logic:Nowhere, which never appears as a subject
    // of any triple in the dataset — a rubber-stamp, not a real grounding.
    let ds = ds_of(
        r#"
            gmeow:S a sh:NodeShape ; logic:formalizes logic:Nowhere .
            "#,
    );
    assert_eq!(residue(&ds, &shacl_vocab()), 1);
}

#[test]
fn declarative_owl_rdfs_axiom_not_counted_for_guarded_vocab() {
    let ds = ds_of(
        r#"
            gmeow:Widget a owl:Class ; rdfs:subClassOf gmeow:Thing .
            gmeow:Thing a owl:Class .
            "#,
    );
    let vocab = alignment_vocab("gufo", "https://w3id.org/gufo#");
    // Neither triple mentions the gufo namespace at all, so the gUFO-guarded
    // residue over this dataset is 0 — owl/rdfs declarative axioms are simply
    // outside the vocab's own namespace, never a `gufo`-kind construct.
    assert_eq!(residue(&ds, &vocab), 0);
}

#[test]
fn raw_external_bridge_now_counts_not_exempt() {
    let ds = ds_of(
        r#"
            gmeow:X rdfs:subClassOf gufo:Kind .
            gmeow:X gufo:mediates gmeow:Y .
            "#,
    );
    let vocab = alignment_vocab("gufo", "https://w3id.org/gufo#");
    // A raw rdfs:subClassOf to an external gufo object is NO LONGER an exempt
    // bridge (it is not a validated gmeow:TermEquivalence cell), so it counts; the
    // gufo:mediates triple counts too → residue 2.
    assert_eq!(residue(&ds, &vocab), 2);
}

#[test]
fn validated_correspondence_cell_is_exempt() {
    // A native RDF-1.2 grounding correspondence: the envelope rides the reifier, so
    // the only external-facing flat triple is the asserted match base triple, and it
    // is subtracted as a by-reference bridge on the owner surface.
    let ds = ds_of(
        r#"
            gmeow:MyKind skos:exactMatch gufo:Kind {|
                a logic:GroundingCorrespondence ;
                gmeow:sssomFile "grounding.sssom.tsv" ;
                gmeow:justification gmeow:ManualMappingCuration ;
                logic:sourceEndpoint gmeow:MyKind ;
                logic:targetEndpoint gufo:Kind ;
                logic:morphismClass logic:WellBehavedLens ;
                logic:morphismKind logic:InstitutionMorphism ;
                logic:preservationKind logic:SoundUnderApproximation
            |} .
            "#,
    );
    let vocab = alignment_vocab("gufo", "https://w3id.org/gufo#");
    assert_eq!(residue(&ds, &vocab), 0);
}

#[test]
fn validated_cell_on_non_owner_surface_still_counts() {
    // The SAME validated cell that is exempt on the owner surface counts when
    // measured on a non-owner surface — strict owner boundary (C1e). The single
    // asserted match base triple is the one external-facing flat triple.
    let ds = ds_of(
        r#"
            gmeow:MyKind skos:exactMatch gufo:Kind {|
                a logic:GroundingCorrespondence ;
                gmeow:sssomFile "grounding.sssom.tsv" ;
                gmeow:justification gmeow:ManualMappingCuration ;
                logic:sourceEndpoint gmeow:MyKind ;
                logic:targetEndpoint gufo:Kind ;
                logic:morphismClass logic:WellBehavedLens ;
                logic:morphismKind logic:InstitutionMorphism ;
                logic:preservationKind logic:SoundUnderApproximation
            |} .
            "#,
    );
    let vocab = alignment_vocab("gufo", "https://w3id.org/gufo#"); // owner = LOGIC_NS
    assert_eq!(residue_for_surface(&ds, &vocab, LOGIC_NS), 0); // on owner: exempt
    assert_eq!(
        residue_for_surface(
            &ds,
            &vocab,
            "https://blackcatinformatics.ca/gmeow/slices/kernel"
        ),
        1 // the match base triple counts off the owner surface
    );
}

#[test]
fn grounding_cell_without_justification_is_not_exempt() {
    // A native grounding cell missing its warrant (no gmeow:justification) is an
    // incomplete grounding correspondence; targeting a TYPED-axiom foundational
    // vocabulary, its match base triple stays in the residue.
    let ds = ds_of(
        r#"
            gmeow:MyKind skos:exactMatch gufo:Kind {|
                a logic:GroundingCorrespondence ;
                gmeow:sssomFile "grounding.sssom.tsv" ;
                logic:sourceEndpoint gmeow:MyKind ;
                logic:targetEndpoint gufo:Kind ;
                logic:morphismClass logic:WellBehavedLens ;
                logic:morphismKind logic:InstitutionMorphism ;
                logic:preservationKind logic:SoundUnderApproximation
            |} .
            "#,
    );
    let vocab = alignment_vocab("gufo", "https://w3id.org/gufo#");
    assert_eq!(residue(&ds, &vocab), 1);
}

#[test]
fn ordinary_alignment_to_typed_vocab_stays_in_residue() {
    // A bare native alignment cell (no grounding envelope) to a TYPED-axiom
    // foundational vocabulary is not a warranted grounding correspondence; its match
    // base triple counts, never opening the owner boundary.
    let ds = ds_of(
        r#"
            gmeow:MyKind skos:exactMatch gufo:Kind {|
                gmeow:sssomFile "ordinary.sssom.tsv" ;
                gmeow:justification gmeow:ManualMappingCuration
            |} .
            "#,
    );
    let vocab = alignment_vocab("gufo", "https://w3id.org/gufo#");
    assert_eq!(residue(&ds, &vocab), 1);
}

#[test]
fn structural_domain_alignment_cell_is_exempt() {
    // A domain slice aligning an external class into the gmeow taxonomy via a native
    // rdfs:subClassOf cell is a first-class correspondence record, not hand-authored
    // second-source rdfs — subtracted from the STRUCTURAL residue on any surface.
    let ds = ds_of(
        r#"
            gufo:Kind rdfs:subClassOf gmeow:MyKind {|
                gmeow:sssomFile "classes.sssom.tsv" ;
                gmeow:justification gmeow:ManualMappingCuration
            |} .
            "#,
    );
    let mut vocab = alignment_vocab("rdfs", "http://www.w3.org/2000/01/rdf-schema#");
    vocab.count_kind = CountKind::StructuralAxiom;
    vocab.counted_predicates = vec!["http://www.w3.org/2000/01/rdf-schema#subClassOf".to_owned()];
    assert_eq!(
        residue_for_surface(
            &ds,
            &vocab,
            "https://blackcatinformatics.ca/gmeow/slices/documents"
        ),
        0
    );
}

#[test]
fn single_binding_grounding_projection_target_is_exempt() {
    let ds = ds_of(
        r#"
            gmeow:mapKind a gmeow:ProjectionMapping, logic:GroundingCorrespondence ;
                gmeow:hasMappingPattern [ gmeow:anchor "s" ] ;
                gmeow:hasBinding [
                    gmeow:profile "gufo" ;
                    gmeow:relation "=" ;
                    gmeow:toClass gufo:Kind
                ] ;
                gmeow:justification gmeow:ManualMappingCuration ;
                logic:sourceEndpoint gmeow:MyKind ;
                logic:targetEndpoint gufo:Kind ;
                logic:morphismClass logic:WellBehavedLens ;
                logic:morphismKind logic:InstitutionMorphism ;
                logic:preservationKind logic:SoundUnderApproximation .
            "#,
    );
    let vocab = alignment_vocab("gufo", "https://w3id.org/gufo#");
    assert_eq!(residue(&ds, &vocab), 0);
}

#[test]
fn multi_binding_grounding_projection_does_not_open_the_boundary() {
    let ds = ds_of(
        r#"
            gmeow:mapKind a gmeow:ProjectionMapping, logic:GroundingCorrespondence ;
                gmeow:hasMappingPattern [ gmeow:anchor "s" ] ;
                gmeow:hasBinding
                    [ gmeow:profile "gufo" ; gmeow:relation "=" ; gmeow:toClass gufo:Kind ],
                    [ gmeow:profile "gufo-2" ; gmeow:relation "=" ; gmeow:toClass gufo:Category ] ;
                gmeow:justification gmeow:ManualMappingCuration ;
                logic:sourceEndpoint gmeow:MyKind ;
                logic:targetEndpoint gufo:Kind ;
                logic:morphismClass logic:WellBehavedLens ;
                logic:morphismKind logic:InstitutionMorphism ;
                logic:preservationKind logic:SoundUnderApproximation .
            "#,
    );
    let vocab = alignment_vocab("gufo", "https://w3id.org/gufo#");
    assert_eq!(residue(&ds, &vocab), 3);
}

#[test]
fn internal_equivalent_class_stays_in_residue() {
    let ds = ds_of(
        r#"
            gmeow:X owl:equivalentClass gmeow:Y .
            "#,
    );
    // Use a vocab whose namespace is the gmeow namespace itself and whose
    // alignment predicates include owl:equivalentClass: the triple matches the
    // vocab (object is in-namespace) but the object is INTERNAL, so the bridge
    // carve-out does not apply — a genuine second-source-of-truth axiom.
    let vocab = alignment_vocab("gmeow-internal", GMEOW);
    assert_eq!(residue(&ds, &vocab), 1);
}

#[test]
fn anonymous_nested_property_shape_counted_in_full_residue() {
    let ds = ds_of(
        r#"
            gmeow:S a sh:NodeShape ;
                sh:property [ sh:path gmeow:p ; sh:minCount 1 ] .
            "#,
    );
    // Two constructs: the named gmeow:S NodeShape, and the anonymous blank-node
    // property shape (caught via its sh:path subject, not via rdf:type).
    assert_eq!(residue(&ds, &shacl_vocab()), 2);
}

#[test]
fn anonymous_nested_property_shape_absent_from_historical_scope() {
    // The legacy axis scope (typed shapes only) does NOT see the anonymous
    // blank-node property shape — only the named sh:NodeShape.
    let ds = ds_of(
        r#"
            gmeow:S a sh:NodeShape ;
                sh:property [ sh:path gmeow:p ; sh:minCount 1 ] .
            "#,
    );
    let constructs = enumerate(&ds, &shacl_vocab(), CountMode::Historical, "");
    assert_eq!(constructs.len(), 1);
    assert_eq!(constructs[0].key, format!("{GMEOW}S"));
}

#[test]
fn aliased_namespace_construct_counted() {
    let ds = ds_of(
        r#"
            gmeow:X gufo:mediates gmeow:Y .
            "#,
    );
    let vocab = ProjectionVocabulary {
        prefix: "gufo".to_owned(),
        namespaces: vec![
            "https://w3id.org/gufo#".to_owned(),
            "http://gufo.example.org/aliased#".to_owned(),
        ],
        subsumed_by: LOGIC_NS.to_owned(),
        owner: LOGIC_NS.to_owned(),
        count_kind: CountKind::TypedAxiom,
        default_ceiling: 0,
        preservation: "SoundUnderApproximation".to_owned(),
        alignment_predicates: Vec::new(),
        counted_predicates: Vec::new(),
    };
    assert_eq!(residue(&ds, &vocab), 1);
}

#[test]
fn non_rdf_surface_vocab_is_structurally_zero() {
    let ds = ds_of(
        r#"
            gmeow:S a sh:NodeShape .
            "#,
    );
    let vocab = ProjectionVocabulary {
        prefix: "datalog".to_owned(),
        namespaces: vec!["https://blackcatinformatics.ca/datalog/".to_owned()],
        subsumed_by: LOGIC_NS.to_owned(),
        owner: LOGIC_NS.to_owned(),
        count_kind: CountKind::NonRdfSurface,
        default_ceiling: 0,
        preservation: "SoundUnderApproximation".to_owned(),
        alignment_predicates: Vec::new(),
        counted_predicates: Vec::new(),
    };
    assert_eq!(residue(&ds, &vocab), 0);
}

#[test]
fn grounded_fraction_matches_legacy_grounded_over_authored() {
    let ds = ds_of(
        r#"
            gmeow:A a sh:NodeShape ; logic:formalizes logic:Obligation .
            gmeow:B a sh:PropertyShape .
            logic:Obligation a owl:Class .
            "#,
    );
    // 1 of 2 typed shapes carries logic:formalizes → 0.5.
    assert!((grounded_fraction(&ds, &shacl_vocab()) - 0.5).abs() < f64::EPSILON);
}

#[test]
fn grounded_fraction_is_one_when_nothing_authored() {
    let ds = ds_of("gmeow:Unrelated a owl:Class .");
    assert!((grounded_fraction(&ds, &shacl_vocab()) - 1.0).abs() < f64::EPSILON);
}

// -------------------------------------------------------------------------
// ONE counter: the count is the construct set's length, never a second walk.
// -------------------------------------------------------------------------

#[test]
fn residue_count_is_exactly_the_construct_sets_length() {
    // Four countable shape nodes (two named + two anonymous nested blocks), one of
    // which is grounded and therefore NOT in the residue.
    let ds = ds_of(
        r#"
            gmeow:S a sh:NodeShape ;
                sh:property [ sh:path gmeow:p ; sh:minCount 1 ] ,
                            [ sh:path gmeow:q ; sh:minCount 1 ] .
            gmeow:T a sh:NodeShape ; logic:formalizes logic:tAxiom .
            logic:tAxiom a logic:Formula .
            "#,
    );
    let vocab = shacl_vocab();
    let constructs = residue_constructs_for_surface(&ds, &vocab, &vocab.owner);
    assert_eq!(constructs.len(), 3, "gmeow:S + its two nested blocks");
    assert!(
        constructs.iter().all(|c| !c.grounded && !c.is_bridge),
        "the residue set holds only ungrounded, non-bridge constructs"
    );
    assert_eq!(
        residue_for_surface(&ds, &vocab, &vocab.owner),
        constructs.len() as u64,
        "the count MUST be the construct set's `.len()` projection"
    );
    assert_eq!(residue(&ds, &vocab), constructs.len() as u64);
}

// -------------------------------------------------------------------------
// Witness anchoring.
// -------------------------------------------------------------------------

#[test]
fn named_subject_anchors_on_its_own_term_iri() {
    let ds = ds_of("gmeow:S a sh:NodeShape .");
    let vocab = shacl_vocab();
    let constructs = residue_constructs_for_surface(&ds, &vocab, &vocab.owner);
    assert_eq!(constructs.len(), 1);
    assert_eq!(
        constructs[0].witness,
        Witness::Anchored(format!("{GMEOW}S"))
    );
    assert!(constructs[0].witness.is_relocatable());
}

#[test]
fn structural_axiom_anchors_on_the_subject_term_not_the_whole_triple() {
    // The construct KEY is the full `s|p|o` rendering; the WITNESS is the subject
    // term IRI alone, so a blank OBJECT (whose `_:label#scope` depends on dataset
    // construction order) cannot perturb the identity.
    let ds = ds_of("gmeow:X rdfs:subClassOf [ a owl:Class ] .");
    let mut vocab = alignment_vocab("rdfs", "http://www.w3.org/2000/01/rdf-schema#");
    vocab.count_kind = CountKind::StructuralAxiom;
    vocab.counted_predicates = vec!["http://www.w3.org/2000/01/rdf-schema#subClassOf".to_owned()];
    let constructs = residue_constructs_for_surface(&ds, &vocab, &vocab.owner);
    assert_eq!(constructs.len(), 1);
    assert!(
        constructs[0].key.contains("_:"),
        "the key still renders the blank object: {}",
        constructs[0].key
    );
    assert_eq!(
        constructs[0].witness,
        Witness::Anchored(format!("{GMEOW}X")),
        "the witness is the SUBJECT term IRI, not the whole triple"
    );
}

#[test]
fn blank_subject_anchors_on_its_nearest_named_sh_ancestor() {
    // The anonymous nested property shape (blank SUBJECT) anchors on gmeow:S, and
    // the doubly-nested sh:node block anchors on gmeow:S too (two hops up).
    let ds = ds_of(
        r#"
            gmeow:S a sh:NodeShape ;
                sh:property [ sh:path gmeow:p ; sh:node [ sh:path gmeow:q ] ] .
            "#,
    );
    let vocab = shacl_vocab();
    let constructs = residue_constructs_for_surface(&ds, &vocab, &vocab.owner);
    assert_eq!(constructs.len(), 3, "gmeow:S + two nested blank shapes");
    let anchored = Witness::Anchored(format!("{GMEOW}S"));
    assert!(
        constructs.iter().all(|c| c.witness == anchored),
        "every construct anchors on the named ancestor: {:?}",
        constructs.iter().map(|c| &c.witness).collect::<Vec<_>>()
    );
    // The blank-subject constructs really are blank-keyed — the anchoring is doing
    // work, not trivially reading an IRI subject back.
    assert_eq!(
        constructs
            .iter()
            .filter(|c| c.key.starts_with("_:"))
            .count(),
        2
    );
}

#[test]
fn blank_subject_without_named_ancestor_is_non_relocatable() {
    // A top-level anonymous property shape: a blank SUBJECT with no
    // sh:property/sh:node parent at all. Fail-closed — there is no
    // relocation-invariant identity to carry, so it must NOT be forgivable.
    let ds = ds_of("[] sh:path gmeow:p ; sh:minCount 1 .");
    let vocab = shacl_vocab();
    let constructs = residue_constructs_for_surface(&ds, &vocab, &vocab.owner);
    assert_eq!(constructs.len(), 1);
    assert_eq!(constructs[0].witness, Witness::NonRelocatable);
    assert!(!constructs[0].witness.is_relocatable());
    assert_eq!(constructs[0].witness.anchor(), None);
}

#[test]
fn a_non_relocatable_construct_carries_no_relocation_warrant() {
    let source = ds_of("[] sh:path gmeow:p ; sh:minCount 1 .");
    let destination = ds_of("[] sh:path gmeow:p ; sh:minCount 1 .");
    let vocab = shacl_vocab();
    let reasons = relocation_reasons(
        &source,
        &vocab.owner,
        &destination,
        "https://blackcatinformatics.ca/gmeow/slices/kernel",
        &vocab,
    );
    assert!(
        reasons.is_empty(),
        "a NonRelocatable construct must never appear in the reason map: {reasons:?}"
    );
}

// -------------------------------------------------------------------------
// Relocation reason codes — computed from real measurements on both sides.
// -------------------------------------------------------------------------

/// A native RDF-1.2 grounding correspondence, exempt ONLY on the owner surface.
fn grounding_cell_ds() -> std::sync::Arc<RdfDataset> {
    ds_of(
        r#"
            gmeow:MyKind skos:exactMatch gufo:Kind {|
                a logic:GroundingCorrespondence ;
                gmeow:sssomFile "grounding.sssom.tsv" ;
                gmeow:justification gmeow:ManualMappingCuration ;
                logic:sourceEndpoint gmeow:MyKind ;
                logic:targetEndpoint gufo:Kind ;
                logic:morphismClass logic:WellBehavedLens ;
                logic:morphismKind logic:InstitutionMorphism ;
                logic:preservationKind logic:SoundUnderApproximation
            |} .
            "#,
    )
}

#[test]
fn reason_code_exemption_shift_owner_boundary() {
    // The SAME bytes are bridge-exempt on the vocabulary's owner surface and NOT
    // exempt one surface over: relocation alone manufactures the residue.
    let source = grounding_cell_ds();
    let destination = grounding_cell_ds();
    let vocab = alignment_vocab("gufo", "https://w3id.org/gufo#"); // owner = LOGIC_NS
    let dest_surface = "https://blackcatinformatics.ca/gmeow/slices/kernel";
    assert_eq!(residue_for_surface(&source, &vocab, &vocab.owner), 0);
    assert_eq!(residue_for_surface(&source, &vocab, dest_surface), 1);

    let reasons = relocation_reasons(&source, &vocab.owner, &destination, dest_surface, &vocab);
    let anchor = format!("{GMEOW}MyKind");
    assert_eq!(
        reasons
            .get(&anchor)
            .map(|r| r.iter().copied().collect::<Vec<_>>()),
        Some(vec![RelocationReason::ExemptionShiftOwnerBoundary]),
        "{reasons:?}"
    );
    assert_eq!(
        RelocationReason::ExemptionShiftOwnerBoundary.code(),
        "exemption-shift-owner-boundary"
    );
}

#[test]
fn reason_code_bridge_exempt_both_sides() {
    // A structural domain alignment cell is a first-class correspondence record on
    // EVERY surface, so moving it is residue-neutral — never new authored debt.
    let source = ds_of(
        r#"
            gufo:Kind rdfs:subClassOf gmeow:MyKind {|
                gmeow:sssomFile "classes.sssom.tsv" ;
                gmeow:justification gmeow:ManualMappingCuration
            |} .
            "#,
    );
    let destination = ds_of(
        r#"
            gufo:Kind rdfs:subClassOf gmeow:MyKind {|
                gmeow:sssomFile "classes.sssom.tsv" ;
                gmeow:justification gmeow:ManualMappingCuration
            |} .
            "#,
    );
    let mut vocab = alignment_vocab("rdfs", "http://www.w3.org/2000/01/rdf-schema#");
    vocab.count_kind = CountKind::StructuralAxiom;
    vocab.counted_predicates = vec!["http://www.w3.org/2000/01/rdf-schema#subClassOf".to_owned()];
    let from = "https://blackcatinformatics.ca/gmeow/slices/documents";
    let to = "https://blackcatinformatics.ca/gmeow/slices/kernel";
    assert_eq!(residue_for_surface(&source, &vocab, from), 0);
    assert_eq!(residue_for_surface(&source, &vocab, to), 0);

    let reasons = relocation_reasons(&source, from, &destination, to, &vocab);
    assert_eq!(
        reasons
            .get("https://w3id.org/gufo#Kind")
            .map(|r| r.iter().copied().collect::<Vec<_>>()),
        Some(vec![RelocationReason::BridgeExemptBothSides]),
        "{reasons:?}"
    );
    assert_eq!(
        RelocationReason::BridgeExemptBothSides.code(),
        "bridge-exempt-both-sides"
    );
}

#[test]
fn reason_code_grounding_orphaned() {
    // Source: the shape AND the logic:Formula that grounds it live in one dataset,
    // so the shape is grounded and contributes no residue.
    let source = ds_of(
        r#"
            gmeow:S a sh:NodeShape ; logic:formalizes logic:sAxiom .
            logic:sAxiom a logic:Formula .
            "#,
    );
    // Destination: the shape moved, the grounding axiom stayed behind. The
    // back-reference is intact but no longer RESOLVABLE in this dataset, so residue
    // is manufactured with no authoring at all.
    let destination = ds_of("gmeow:S a sh:NodeShape ; logic:formalizes logic:sAxiom .");
    let vocab = shacl_vocab();
    let to = "https://blackcatinformatics.ca/gmeow/slices/kernel";
    assert_eq!(residue_for_surface(&source, &vocab, &vocab.owner), 0);
    assert_eq!(residue_for_surface(&destination, &vocab, to), 1);

    let reasons = relocation_reasons(&source, &vocab.owner, &destination, to, &vocab);
    assert_eq!(
        reasons
            .get(&format!("{GMEOW}S"))
            .map(|r| r.iter().copied().collect::<Vec<_>>()),
        Some(vec![RelocationReason::GroundingOrphaned]),
        "{reasons:?}"
    );
    assert_eq!(
        RelocationReason::GroundingOrphaned.code(),
        "grounding-orphaned"
    );
}

#[test]
fn grounding_that_travels_with_its_axiom_is_not_orphaned() {
    // The control for `reason_code_grounding_orphaned`: when the logic:Formula moves
    // WITH the shape, nothing is orphaned and no reason is reported.
    let source = ds_of(
        r#"
            gmeow:S a sh:NodeShape ; logic:formalizes logic:sAxiom .
            logic:sAxiom a logic:Formula .
            "#,
    );
    let destination = ds_of(
        r#"
            gmeow:S a sh:NodeShape ; logic:formalizes logic:sAxiom .
            logic:sAxiom a logic:Formula .
            "#,
    );
    let vocab = shacl_vocab();
    let reasons = relocation_reasons(
        &source,
        &vocab.owner,
        &destination,
        "https://blackcatinformatics.ca/gmeow/slices/kernel",
        &vocab,
    );
    assert!(reasons.is_empty(), "{reasons:?}");
}
