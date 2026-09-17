// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::ir::{CorrespondenceLaw, DischargeVerdict, LawClaimIr, MorphismClass, PreservationKind};
use crate::projections::correspondence_frontend::{CorrespondenceAnalysis, TypedRelation};
use crate::projections::get_leg::{Item, MappingPattern};
use crate::projections::reified_claim::*;
use purrdf::sparql::{GraphPattern, NamedNodePattern, TriplePattern};

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const MLS_MODEL: &str = "http://www.w3.org/ns/mls#Model";
const GM_MODEL_ARTIFACT: &str = "https://blackcatinformatics.ca/gmeow/ModelArtifact";

/// A bare `?x a <class>` source atom (non-optional, so it seeds the anchor).
fn type_atom(subject: &str, class_iri: &str) -> Atom {
    Atom {
        subject_var: subject.to_owned(),
        predicate: Some(RDF_TYPE.to_owned()),
        predicate_var: None,
        path: None,
        path_alts: Vec::new(),
        object_var: None,
        object_value: Some(class_iri.to_owned()),
        object_literal: None,
        optional: false,
    }
}

/// A one-atom class cell: the gmeow source `?x a gmeow:ModelArtifact` mapping to the
/// external `?x a mls:Model` (via `to_class`), authored with the given up-lift knobs.
fn class_cell(
    relation: &str,
    mnemomorphic: bool,
    ingest_claim: Option<LawClaimIr>,
) -> ProjectionCell {
    let pattern = MappingPattern {
        anchor: "x".to_owned(),
        value: None,
        atoms: vec![Item::Atom(type_atom("x", GM_MODEL_ARTIFACT))],
        suppress_when: Vec::new(),
        project_when: Vec::new(),
        exclude_when: Vec::new(),
        filters: Vec::new(),
        binds: Vec::new(),
        mints: Vec::new(),
        edoal_source: None,
        edoal_source_kind: None,
        edoal_path: false,
    };
    let binding = ProfileBinding {
        profile: "ml-schema".to_owned(),
        to_predicate: None,
        to_class: Some(MLS_MODEL.to_owned()),
        template_atoms: Vec::new(),
        value_class_map: Vec::new(),
        relation: relation.to_owned(),
        transform: None,
        confidence: None,
        lossy_drops: Vec::new(),
        edoal_target: None,
        edoal_target_kind: None,
        morphism_class: None,
        ingest_claim,
        ingest_residue: Vec::new(),
        mnemomorphic,
        emit_sssom: false,
        sssom_predicate: None,
        sssom_file: None,
    };
    ProjectionCell {
        iri: "https://blackcatinformatics.ca/gmeow/example/mlModelCell".to_owned(),
        label: String::new(),
        pattern,
        bindings: vec![binding],
        grounding: None,
    }
}

fn put_get_claim() -> LawClaimIr {
    LawClaimIr {
        law: CorrespondenceLaw::PutGet,
        verdict: DischargeVerdict::ObligationUnknown,
        condition: None,
    }
}

fn predicate_atom(subject: &str, predicate_iri: &str, object: &str) -> Atom {
    Atom {
        subject_var: subject.to_owned(),
        predicate: Some(predicate_iri.to_owned()),
        predicate_var: None,
        path: None,
        path_alts: Vec::new(),
        object_var: Some(object.to_owned()),
        object_value: None,
        object_literal: None,
        optional: false,
    }
}

fn emitted(cells: &[ProjectionCell]) -> Option<(Query, String)> {
    let mut program = native::LegBuilder::default();
    let mut index = 0;
    let mut claim = false;
    for cell in cells {
        for binding in &cell.bindings {
            if let Some(leg) = lower_binding(
                cell,
                binding,
                &mut index,
                &HelperNames::for_binding(cell, binding),
            )
            .unwrap()
            {
                claim |= leg.claim;
                program.push(&leg.template, &leg.pattern);
            }
        }
    }
    program.finish().map(|query| {
        let output = emit_profile("ml-schema", claim, query.clone()).unwrap();
        (query, output)
    })
}

fn template(query: &Query) -> Vec<TriplePattern> {
    let Query::Construct { template, .. } = query else {
        panic!("carrier required")
    };
    template
        .iter()
        .map(|quad| {
            assert!(quad.graph.is_none());
            quad.triple.clone()
        })
        .collect()
}

fn has(triples: &[TriplePattern], predicate: &str, object: TermPattern) -> bool {
    triples.iter().any(|triple| {
        triple.predicate == NamedNodePattern::NamedNode(native::iri(predicate).unwrap())
            && triple.object == object
    })
}

#[test]
fn complete_over_multi_atom_emits_every_source_atom_bare_and_no_envelope() {
    let mut cell = class_cell("=", true, None);
    cell.pattern
        .atoms
        .push(Item::Atom(predicate_atom("x", "urn:related", "y")));
    let (query, text) = emitted(&[cell]).unwrap();
    assert_eq!(
        template(&query),
        vec![
            native::triple(
                native::var("x"),
                RDF_TYPE,
                native::named(GM_MODEL_ARTIFACT).unwrap()
            )
            .unwrap(),
            native::triple(native::var("x"), "urn:related", native::var("y")).unwrap(),
        ]
    );
    let Query::Construct { pattern, .. } = query else {
        unreachable!()
    };
    assert_eq!(
        pattern,
        native::bgp(vec![
            native::triple(
                native::var("x"),
                RDF_TYPE,
                native::named(MLS_MODEL).unwrap()
            )
            .unwrap()
        ])
    );
    assert!(text.contains("identity on the displayable image"));
    assert!(!text.contains("= id_S"));
}

#[test]
fn validation_only_binding_reifies_the_lift_as_an_inert_claim() {
    let (query, text) = emitted(&[class_cell("<=", false, Some(put_get_claim()))]).unwrap();
    let triples = template(&query);
    let source = native::named(GM_MODEL_ARTIFACT).unwrap();
    assert!(!has(&triples, RDF_TYPE, source.clone()));
    assert!(has(
        &triples,
        RDF_TYPE,
        native::named(GM_STATEMENT_METADATA).unwrap()
    ));
    assert!(has(&triples, GM_Q_SUBJECT, native::var("x")));
    assert!(has(
        &triples,
        GM_Q_PREDICATE,
        native::named(RDF_TYPE).unwrap()
    ));
    assert!(has(&triples, GM_Q_OBJECT, source));
    assert!(has(
        &triples,
        GM_ANN_PROPERTY,
        native::named(GM_MAPPED_FROM).unwrap()
    ));
    assert!(has(
        &triples,
        GM_ANN_VALUE,
        native::named(MLS_MODEL).unwrap()
    ));
    assert!(has(
        &triples,
        GM_WAS_GENERATED_BY,
        native::named("https://blackcatinformatics.ca/gmeow/import/ml-schema").unwrap()
    ));
    let import_class =
        native::named("https://blackcatinformatics.ca/gmeow/ImportActivity").unwrap();
    assert_eq!(
        triples
            .iter()
            .filter(|triple| triple.object == import_class)
            .count(),
        1
    );
    assert!(has(
        &triples,
        "http://www.w3.org/2000/01/rdf-schema#label",
        TermPattern::Literal(Literal::new_simple(
            "inverse-ingest of ml-schema into GMEOW"
        ))
    ));
    assert!(has(
        &triples,
        "https://blackcatinformatics.ca/gmeow/wasAssociatedWith",
        native::named(IMPORTER_AGENT_IRI).unwrap()
    ));
    assert!(has(
        &triples,
        RDF_TYPE,
        native::named("https://blackcatinformatics.ca/gmeow/SoftwareAgent").unwrap()
    ));
    let Query::Construct { pattern, .. } = query else {
        unreachable!()
    };
    assert_eq!(
        pattern,
        GraphPattern::Bgp {
            patterns: vec![
                native::triple(
                    native::var("x"),
                    RDF_TYPE,
                    native::named(MLS_MODEL).unwrap()
                )
                .unwrap()
            ]
        }
    );
    assert!(text.contains("Mint-with-claim, validation-only"));
    assert!(!text.contains("NOW("));
}

#[test]
fn mixed_profile_asserts_recovery_bare_and_reifies_the_lossy_lift_under_one_import() {
    let recovery = class_cell("=", true, None);
    let mut lossy = class_cell("<=", false, Some(put_get_claim()));
    lossy.pattern.anchor = "y".into();
    lossy.pattern.atoms = vec![Item::Atom(type_atom("y", GM_MODEL_ARTIFACT))];
    let (query, text) = emitted(&[recovery, lossy]).unwrap();
    let triples = template(&query);
    assert!(
        triples.contains(
            &native::triple(
                native::var("x"),
                RDF_TYPE,
                native::named(GM_MODEL_ARTIFACT).unwrap()
            )
            .unwrap()
        )
    );
    assert!(
        !triples.contains(
            &native::triple(
                native::var("y"),
                RDF_TYPE,
                native::named(GM_MODEL_ARTIFACT).unwrap()
            )
            .unwrap()
        )
    );
    assert!(has(&triples, GM_Q_SUBJECT, native::var("y")));
    let import = native::named("https://blackcatinformatics.ca/gmeow/ImportActivity").unwrap();
    assert_eq!(
        triples
            .iter()
            .filter(|triple| triple.object == import)
            .count(),
        1
    );
    assert!(text.contains("Mint-with-claim, validation-only"));
}

#[test]
fn emission_is_deterministic_and_clock_free() {
    let cells = [class_cell("<=", false, Some(put_get_claim()))];
    let first = emitted(&cells).unwrap();
    assert_eq!(first, emitted(&cells).unwrap());
    assert!(!first.1.contains("NOW("));
}

#[test]
fn binding_residue_is_interned_once_and_unsupported_put_is_absent() {
    let note = "no durable subject or tenure; never fabricated";
    let mut cell = class_cell("<=", false, Some(put_get_claim()));
    cell.bindings[0].ingest_residue = vec![note.into()];
    let (relation, morphism_class, morphism_kind) = cell.bindings[0].lattice();
    let lookup = CorrespondenceAnalysis::for_binding_test(
        &cell,
        &cell.bindings[0],
        TypedRelation {
            relation,
            morphism_class,
            morphism_kind,
        },
    );
    let lowered = super::super::sparql::lower_cells(
        &[cell.clone()],
        &super::super::sparql::SuppressionVocab::empty(),
        &lookup,
        &["ml-schema"],
    )
    .unwrap();
    let rows = lowered
        .ledger
        .iter()
        .filter(|row| row.target.starts_with("sparql-put:"))
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].preservation, PreservationKind::ValidationOnly);
    let drops = lowered.loss.projection_drops_for(&rows[0].target);
    assert!(drops.contains(&format!("actual: {note}")));
    assert!(rows[0].target.contains(&cell.iri));
    assert!(rows[0].target.contains("ml-schema"));
    assert!(
        !drops
            .iter()
            .any(|drop| drop.starts_with("actual: correspondence: "))
    );
    assert_eq!(lowered.legs.len(), 1);
    cell.bindings[0].ingest_claim = None;
    assert!(emitted(&[cell.clone()]).is_none());
    cell.bindings[0].relation = "=".into();
    cell.bindings[0].mnemomorphic = true;
    let recovery = lower_binding(
        &cell,
        &cell.bindings[0],
        &mut 0,
        &HelperNames::for_binding(&cell, &cell.bindings[0]),
    )
    .unwrap()
    .unwrap();
    assert!(recovery.residue.is_empty());
}

#[test]
fn candidate_literal_claim_retains_datatype_and_direction_without_assertion() {
    use crate::ingest::DslTerm;
    let mut cell = class_cell("<=", false, Some(put_get_claim()));
    for literal in [
        DslTerm::Literal {
            lexical_form: "0.90".into(),
            datatype: "http://www.w3.org/2001/XMLSchema#decimal".into(),
            language: None,
            direction: None,
        },
        DslTerm::Literal {
            lexical_form: "hello".into(),
            datatype: "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString".into(),
            language: Some("en".into()),
            direction: Some(purrdf::RdfTextDirection::Rtl),
        },
    ] {
        let mut atom = predicate_atom("x", "urn:observed", "value");
        atom.object_var = None;
        atom.object_literal = Some(literal.clone());
        cell.pattern.atoms = vec![Item::Atom(atom)];
        let (query, _) = emitted(&[cell.clone()]).unwrap();
        let triples = template(&query);
        assert!(has(
            &triples,
            GM_Q_OBJECT_LITERAL,
            native::literal_term(&literal).unwrap()
        ));
        assert!(!has(
            &triples,
            "urn:observed",
            native::literal_term(&literal).unwrap()
        ));
    }
}

#[test]
fn classify_put_is_the_single_authority_for_the_three_polarities() {
    use crate::projections::put_derivation::PutClass;
    assert_eq!(
        classify_put(false, MorphismClass::LossyLens, &[put_get_claim()]),
        PutClass::ValidationOnly
    );
    assert_eq!(
        classify_put(true, MorphismClass::WellBehavedLens, &[]),
        PutClass::CompleteOver
    );
    assert_eq!(
        classify_put(false, MorphismClass::LossyLens, &[]),
        PutClass::Unsupported
    );
}
