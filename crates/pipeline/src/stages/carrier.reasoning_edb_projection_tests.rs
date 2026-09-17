// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn termination_demonstrators_are_complete_object_level_worlds() {
    let worlds = termination_demonstrator_graphs().expect("parse termination worlds");
    assert_eq!(worlds.len(), 3);
    assert_eq!(
        worlds.iter().map(|world| world.quad_count()).sum::<usize>(),
        62
    );
    for world in worlds {
        assert!(world.owned_quads().all(|quad| {
            matches!(quad.graph_name, Some(purrdf::RdfTerm::Iri(ref graph))
                    if gmeow_logic::reasoning_graphs::is_object_level_named_graph(graph))
        }));
    }
}

#[test]
fn recovery_formula_envelope_is_meta_level_but_referenced_terms_remain() {
    let trig = b"@prefix ex: <https://example.test/> .
            @prefix logic: <https://blackcatinformatics.ca/logic/> .
            @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
            ex:ordinary ex:p [ ex:q ex:o ] .
            GRAPH <https://blackcatinformatics.ca/gmeow/graph/imports> {
                ex:Source ex:retained ex:yes .
                ex:c logic:recoveryCase ex:case .
                ex:case a logic:RecoveryCase ; logic:recoveryTransform _:root .
                _:root a logic:Formula ;
                    logic:quantifiedVariable _:var ;
                    logic:forall _:implication .
                _:var a logic:TermCarrier ; logic:termIndex 0 ; logic:termVariable \"x\" .
                _:implication a logic:Formula ;
                    logic:antecedent _:source ; logic:consequent _:view .
                _:source a logic:Formula ; logic:relation rdf:type ;
                    logic:argument _:sourceSubject, _:sourceClass .
                _:sourceSubject a logic:TermCarrier ; logic:termIndex 0 ; logic:termVariable \"x\" .
                _:sourceClass a logic:TermCarrier ; logic:termIndex 1 ; logic:termIri ex:Source .
                _:view a logic:Formula ; logic:relation rdf:type ;
                    logic:argument _:viewSubject, _:viewClass .
                _:viewSubject a logic:TermCarrier ; logic:termIndex 0 ; logic:termVariable \"x\" .
                _:viewClass a logic:TermCarrier ; logic:termIndex 1 ; logic:termIri ex:View .
            }";
    let snapshot = parse_dataset(trig, "application/trig", None).expect("parse recovery fixture");
    let edb = snapshot_reasoning_edb(snapshot.as_ref()).expect("project reasoning EDB");
    let quads: Vec<RdfQuad> = edb.owned_quads().collect();

    assert!(quads.iter().any(|quad| {
        quad.subject == RdfTerm::iri("https://example.test/Source")
            && quad.predicate == "https://example.test/retained"
    }));
    assert!(quads.iter().any(|quad| {
        quad.subject == RdfTerm::iri("https://example.test/ordinary")
            && matches!(quad.object, RdfTerm::BlankNode(_))
    }));
    assert!(quads.iter().all(|quad| {
        quad.predicate != "https://blackcatinformatics.ca/logic/recoveryCase"
            && quad.subject != RdfTerm::iri("https://example.test/case")
    }));
    assert_eq!(
        quads
            .iter()
            .filter(|quad| matches!(quad.subject, RdfTerm::BlankNode(_)))
            .count(),
        1,
        "only the unrelated ordinary blank node remains"
    );
}

/// G7: an RDF-star annotation keyed on a reifier that `without_recovery_case_envelopes`
/// prunes must be pruned too — including TRANSITIVELY, when the pruned reifier's
/// identity is itself reified again (RDF 1.2 permits annotating an annotation by
/// reifying its `~reifier` triple). Zero dangling annotation metadata may survive;
/// unrelated, ordinary annotations must be untouched.
#[test]
fn without_recovery_case_envelopes_prunes_annotations_on_pruned_reifiers() {
    const EX: &str = "https://example.test/";
    let recovery_case = RdfTerm::iri(format!("{EX}case"));

    // Seeds `owned` directly: the recovery-case object.
    let seed = RdfQuad::new(
        RdfTerm::iri(format!("{EX}c")),
        "https://blackcatinformatics.ca/logic/recoveryCase",
        recovery_case.clone(),
    );

    // Reifier r1 reifies a statement whose SUBJECT is the recovery-case node, so r1
    // is recovery-owned via the subject/object rule (not because r1's own identity
    // was ever directly asserted as a recoveryCase object).
    let r1 = RdfTerm::iri(format!("{EX}evidenceStmt"));
    let r1_statement = RdfTriple::new(
        recovery_case.clone(),
        format!("{EX}hasEvidence"),
        RdfTerm::iri(format!("{EX}blob")),
    );
    let r1_reifier = purrdf::RdfReifier::new(r1.clone(), r1_statement).in_graph(None);
    let r1_annotation = purrdf::RdfAnnotation::new(
        r1.clone(),
        format!("{EX}confidence"),
        RdfTerm::iri(format!("{EX}high")),
    )
    .in_graph(None);

    // Reifier r3 reifies the ANNOTATION triple `(r1, metaNote, r1)` — i.e. it
    // reifies a triple whose subject is r1's own identity term. r3 is only
    // recovery-owned TRANSITIVELY: r1 becomes owned first (via its statement's
    // subject), and only then does r3's statement (subject = r1) become owned.
    let r3 = RdfTerm::iri(format!("{EX}metaStmt"));
    let r3_statement = RdfTriple::new(
        r1.clone(),
        format!("{EX}metaNote"),
        RdfTerm::iri(format!("{EX}annotated")),
    );
    let r3_reifier = purrdf::RdfReifier::new(r3.clone(), r3_statement).in_graph(None);
    let r3_annotation = purrdf::RdfAnnotation::new(
        r3.clone(),
        format!("{EX}derivedNote"),
        RdfTerm::iri(format!("{EX}something")),
    )
    .in_graph(None);

    // An ordinary, unrelated reifier + annotation that never touches recovery-case
    // territory — must survive untouched.
    let r2 = RdfTerm::iri(format!("{EX}otherStmt"));
    let r2_statement = RdfTriple::new(
        RdfTerm::iri(format!("{EX}ordinarySubj")),
        format!("{EX}ordinaryPred"),
        RdfTerm::iri(format!("{EX}ordinaryObj")),
    );
    let r2_reifier = purrdf::RdfReifier::new(r2.clone(), r2_statement).in_graph(None);
    let r2_annotation = purrdf::RdfAnnotation::new(
        r2.clone(),
        format!("{EX}note"),
        RdfTerm::iri(format!("{EX}fine")),
    )
    .in_graph(None);

    let mut builder = RdfDatasetBuilder::new();
    builder.push_owned_quad(&seed);
    builder.push_owned_reifier(&r1_reifier);
    builder.push_owned_annotation(&r1_annotation);
    builder.push_owned_reifier(&r3_reifier);
    builder.push_owned_annotation(&r3_annotation);
    builder.push_owned_reifier(&r2_reifier);
    builder.push_owned_annotation(&r2_annotation);
    let dataset = builder.freeze().expect("valid RDF 1.2 fixture");

    let edb =
        without_recovery_case_envelopes(dataset.as_ref()).expect("prune recovery-case envelope");

    let reifiers: Vec<purrdf::RdfReifier> = edb.owned_reifiers().collect();
    let annotations: Vec<purrdf::RdfAnnotation> = edb.owned_annotations().collect();

    assert!(
        !reifiers.iter().any(|r| r.reifier == r1),
        "recovery-owned reifier r1 must be pruned"
    );
    assert!(
        !reifiers.iter().any(|r| r.reifier == r3),
        "transitively recovery-owned reifier r3 must be pruned"
    );
    assert!(
        reifiers.iter().any(|r| r.reifier == r2),
        "unrelated reifier r2 must survive"
    );

    assert!(
        !annotations.iter().any(|a| a.reifier == r1),
        "annotation keyed on pruned reifier r1 must be gone (zero dangling metadata)"
    );
    assert!(
        !annotations.iter().any(|a| a.reifier == r3),
        "annotation keyed on transitively pruned reifier r3 must be gone"
    );
    assert!(
        annotations.iter().any(|a| a.reifier == r2),
        "unrelated annotation on r2 must survive"
    );
}

#[test]
fn shipped_correspondence_and_alignment_targets_never_enter_reasoning() {
    let trig = format!(
        "@prefix ex: <https://example.test/> .\n\
             ex:authored ex:p ex:o .\n\
             GRAPH <{GRAPH_STATEMENTS}> {{ ex:statement ex:p ex:o . }}\n\
             GRAPH <{GRAPH_IMPORTS}> {{ ex:imported ex:p ex:o . }}\n\
             GRAPH <{logic}> {{ ex:logic ex:p ex:o . }}\n\
             GRAPH <{relational}> {{ ex:relational ex:p ex:o . }}\n\
             GRAPH <{GRAPH_ALIGNMENTS}> {{ ex:map ex:target <http://www.w3.org/2002/07/owl#maxCardinality> . }}\n\
             GRAPH <{correspondence}> {{ ex:corr ex:target <http://www.w3.org/2002/07/owl#InverseFunctionalProperty> . }}\n\
             GRAPH <{reasoning}> {{ ex:result ex:p ex:o . }}\n",
        logic = crate::stages::compile_logic::GRAPH_LOGIC,
        relational = crate::stages::compile_logic::GRAPH_RELATIONAL_CORE,
        correspondence = crate::stages::compile_logic::GRAPH_CORRESPONDENCE,
        reasoning = gmeow_logic::result_rdf::GRAPH_REASONING,
    );
    let snapshot = parse_dataset(trig.as_bytes(), "application/trig", None)
        .expect("parse snapshot-shaped fixture");
    let edb = snapshot_reasoning_edb(snapshot.as_ref()).expect("project reasoning EDB");

    assert_eq!(
        edb.quad_count(),
        5,
        "default plus the four admitted reasoning worlds present in this fixture \
             (the three demonstrator worlds are admitted too but carry no quad here)"
    );
    let graph_iris: std::collections::BTreeSet<String> = edb
        .owned_quads()
        .filter_map(|quad| match quad.graph_name {
            Some(RdfTerm::Iri(iri)) => Some(iri),
            _ => None,
        })
        .collect();
    assert!(!graph_iris.contains(GRAPH_ALIGNMENTS));
    assert!(!graph_iris.contains(crate::stages::compile_logic::GRAPH_CORRESPONDENCE));
    assert!(!graph_iris.contains(gmeow_logic::result_rdf::GRAPH_REASONING));

    let input = gmeow_logic::reason::prepare_reasoning_input(edb.as_ref())
        .expect("prepare projected synthetic EDB");
    let domains = gmeow_logic::reasoning_graphs::object_level_domains()
        .expect("select the production object-level theory roles");
    let coverage = gmeow_logic::reason::dl_consistency(input, &domains)
        .expect("observe projected EDB coverage through the native execution")
        .coverage;
    assert!(
        coverage.unsupported.is_empty(),
        "meta-level target references must not become DL coverage gaps: {:?}",
        coverage.unsupported
    );
}
