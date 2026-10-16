// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_logic_compile::ir::{
    CorrespondenceRelation, MorphismKind, PreservationKind, TransactionProgramIr,
};

fn native_test_triple(subject: &str, predicate: &str, object: TermPattern) -> SparqlTriplePattern {
    SparqlTriplePattern {
        subject: TermPattern::Variable(Variable::new(subject)),
        predicate: NamedNodePattern::NamedNode(NamedNode::new(predicate).unwrap()),
        object,
    }
}

#[test]
fn recovery_seed_application_preserves_driver_aliases_in_quoted_terms_and_graphs() {
    use purrdf::sparql::algebra::ApplicationPolicy;
    let input = Variable::new("input");
    let driver = Variable::new("driver");
    let quoted = SparqlTriplePattern {
        subject: TermPattern::Variable(input.clone()),
        predicate: NamedNodePattern::Variable(input.clone()),
        object: TermPattern::Variable(input.clone()),
    };
    let pattern = GraphPattern::Apply {
        left: Child::new(GraphPattern::Bgp {
            patterns: vec![native_test_triple(
                "driver",
                "urn:source",
                TermPattern::Variable(Variable::new("value")),
            )],
        }),
        right: Child::new(GraphPattern::Graph {
            name: NamedNodePattern::Variable(input.clone()),
            inner: Child::new(GraphPattern::Bgp {
                patterns: vec![SparqlTriplePattern {
                    subject: TermPattern::Variable(input.clone()),
                    predicate: NamedNodePattern::Variable(input.clone()),
                    object: TermPattern::Triple(Child::new(quoted)),
                }],
            }),
        }),
        policy: Box::new(ApplicationPolicy {
            dataset_required: true,
            row_pipeline: false,
            reduced_adjacent: false,
            group_domain: None,
            inputs: vec![(input, driver)],
            optional: None,
        }),
    };
    let branches = dnf_branches(&pattern, 0).unwrap();
    assert_eq!(branches.len(), 1);
    let rows = instantiate_branch(&branches[0], &mut 0);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].subject, rows[0].subject);
    assert_eq!(rows[1].graph_name.as_ref(), Some(&rows[0].subject));
    let RdfTerm::Iri(driver) = &rows[0].subject else {
        panic!("IRI seed")
    };
    assert_eq!(&rows[1].predicate, driver);
    let RdfTerm::Triple(quoted) = &rows[1].object else {
        panic!("quoted seed")
    };
    assert_eq!(quoted.subject, rows[0].subject);
    assert_eq!(&quoted.predicate, driver);
    assert_eq!(quoted.object, rows[0].subject);
}

#[test]
fn recovery_seed_optional_application_requires_only_its_driver() {
    use purrdf::sparql::algebra::{ApplicationPolicy, OptionalApplication};
    let pattern = GraphPattern::Apply {
        left: Child::new(GraphPattern::Bgp {
            patterns: vec![native_test_triple(
                "driver",
                "urn:required",
                TermPattern::Variable(Variable::new("value")),
            )],
        }),
        right: Child::new(GraphPattern::Bgp {
            patterns: vec![native_test_triple(
                "driver",
                "urn:optional",
                TermPattern::Variable(Variable::new("other")),
            )],
        }),
        policy: Box::new(ApplicationPolicy {
            dataset_required: false,
            row_pipeline: false,
            reduced_adjacent: false,
            group_domain: None,
            inputs: Vec::new(),
            optional: Some(OptionalApplication {
                retry_inputs: Vec::new(),
                forget_marker: Variable::new("forget"),
            }),
        }),
    };
    let branches = dnf_branches(&pattern, 0).unwrap();
    assert_eq!(branches.len(), 1);
    let rows = instantiate_branch(&branches[0], &mut 0);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].predicate, "urn:required");
}

#[test]
fn native_mapping_law_admission_keeps_independent_view_domains_and_bounded_evidence() {
    let term = |iri: &str| TermPattern::NamedNode(NamedNode::new(iri).unwrap());
    let source = native_test_triple("s", "urn:source", term("urn:value"));
    let target = native_test_triple("s", "urn:view", term("urn:default"));
    let edited = native_test_triple(
        "s",
        "urn:view",
        TermPattern::Variable(Variable::new("edited")),
    );
    let get = construct_algebra(vec![target], vec![source.clone()]);
    let put = construct_algebra(vec![source], vec![edited]);
    let claims =
        discharge_algebra_laws(get.clone(), put.clone(), MorphismClass::SectionRetraction).unwrap();
    assert_eq!(claims[0].law, CorrespondenceLaw::SectionLaw);
    assert_eq!(claims[0].verdict, DischargeVerdict::ObligationDischarged);
    assert_eq!(claims[1].law, CorrespondenceLaw::PutGet);
    assert_eq!(claims[1].verdict, DischargeVerdict::ObligationViolated);
    assert!(
        claims
            .iter()
            .all(|claim| claim.condition == Some(DischargeCondition::DischargeBoundedCorpus))
    );
    assert!(
        discharge_algebra_laws(get.clone(), put.clone(), MorphismClass::BridgeView)
            .unwrap()
            .is_empty()
    );
    let select = Query::Select {
        pattern: GraphPattern::Bgp {
            patterns: Vec::new(),
        },
        dataset: Default::default(),
        base_iri: None,
        version: None,
    };
    assert!(discharge_algebra_laws(select.clone(), put, MorphismClass::SectionRetraction).is_err());
    assert!(discharge_algebra_laws(get, select, MorphismClass::SectionRetraction).is_err());
}

#[test]
fn native_mapping_branch_domain_catches_fabricated_guard_with_deterministic_countermodel() {
    let variable = |name| TermPattern::Variable(Variable::new(name));
    let source = [
        native_test_triple("a", "urn:source1", variable("b")),
        native_test_triple("c", "urn:source2", variable("d")),
    ];
    let view = [
        native_test_triple("a", "urn:view1", variable("b")),
        native_test_triple("c", "urn:view2", variable("d")),
    ];
    let union = |patterns: &[SparqlTriplePattern; 2]| {
        GraphPattern::union(
            GraphPattern::Bgp {
                patterns: vec![patterns[0].clone()],
            },
            GraphPattern::Bgp {
                patterns: vec![patterns[1].clone()],
            },
        )
    };
    let query = |template: Vec<SparqlTriplePattern>, pattern| {
        let mut query = construct_algebra(template, Vec::new());
        let Query::Construct { pattern: root, .. } = &mut query else {
            unreachable!()
        };
        *root = pattern;
        query
    };
    let get = query(view.to_vec(), union(&source));
    let mut fabricated = source.to_vec();
    fabricated.push(native_test_triple(
        "c",
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
        TermPattern::NamedNode(NamedNode::new("urn:FabricatedGuard").unwrap()),
    ));
    let put = query(fabricated, union(&view));
    let prepared = PreparedLawExecution::from_algebra(get.clone(), put.clone()).unwrap();
    let sources = derive_query_seeds(prepared.get.query()).unwrap();
    assert_eq!(sources.len(), 3);
    let first = prepared
        .roundtrip(sources.iter().map(LawCase::Seed), true)
        .unwrap();
    assert_eq!(first.verdict, DischargeVerdict::ObligationViolated);
    let countermodel = first.countermodel.as_ref().unwrap();
    assert!(
        countermodel
            .spurious
            .iter()
            .any(|atom| atom.2 == "urn:FabricatedGuard")
    );
    assert!(countermodel.missing.is_empty());
    assert_eq!(
        first,
        prepared
            .roundtrip(sources.iter().map(LawCase::Seed), true)
            .unwrap()
    );
    let again = PreparedLawExecution::from_algebra(get, put).unwrap();
    let sources = derive_query_seeds(again.get.query()).unwrap();
    assert_eq!(
        first,
        again
            .roundtrip(sources.iter().map(LawCase::Seed), true)
            .unwrap()
    );
}

#[test]
fn native_law_inputs_retain_empty_standpoint_graphs_through_execution() {
    let mut dataset = purrdf::RdfDatasetBuilder::new();
    let graph = dataset
        .intern_iri("https://example.org/standpoint/withheld")
        .unwrap();
    dataset.declare_named_graph(graph).unwrap();
    let input = LawInput {
        identity: "withheld-context".into(),
        dataset: dataset.freeze().unwrap(),
    };
    let query = "CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o }";
    let prepared = PreparedLawExecution::new(query, query).unwrap();
    // Equality on an empty quad vector would falsely discharge both laws.
    for outcome in [
        prepared.section(std::slice::from_ref(&input)).unwrap(),
        prepared.put_get(std::slice::from_ref(&input)).unwrap(),
    ] {
        assert_eq!(outcome.verdict, DischargeVerdict::ObligationViolated);
        let countermodel = outcome.countermodel.unwrap();
        assert_eq!(countermodel.seed_label, input.identity);
        assert_eq!(
            countermodel.missing_graphs,
            ["https://example.org/standpoint/withheld"]
        );
        assert!(countermodel.missing.is_empty());
    }
}

#[test]
fn prepared_law_execution_refuses_non_carrier_query_forms() {
    let construct = "CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o }";
    let select = "SELECT ?s WHERE { ?s ?p ?o }";
    assert!(PreparedLawExecution::new(select, construct).is_err());
    assert!(PreparedLawExecution::new(construct, select).is_err());
    let select = leg_relation_algebra(&step("https://example.org/sourceDetail")).unwrap();
    let recovery = lower_recovery_case(&recovery_case(true)).unwrap();
    assert!(PreparedLawExecution::from_algebra(select.clone(), recovery.get.clone()).is_err());
    assert!(PreparedLawExecution::from_algebra(recovery.put, select).is_err());
    assert!(PreparedLawExecution::from_algebra(recovery.get.clone(), recovery.get).is_ok());
}

#[test]
fn typed_recovery_admission_rejects_invalid_paths_before_native_execution() {
    let valid = step("https://example.org/sourceDetail");
    for invalid in [
        step("relative-predicate"),
        step("https://example.org/invalid predicate"),
        LegPath::Seq(Vec::new()),
        LegPath::Alt(Vec::new()),
        LegPath::Inverse(Box::new(LegPath::Seq(Vec::new()))),
    ] {
        assert!(PreparedRecoveryLegs::new(&invalid, &valid).is_err());
        assert!(PreparedRecoveryLegs::new(&valid, &invalid).is_err());
    }
    let mut case = recovery_case(true);
    let Formula::Forall { body, .. } = &mut case.transform else {
        panic!("quantified recovery case");
    };
    let Formula::Implies(source, _) = body.as_mut() else {
        panic!("ordered implication");
    };
    let Formula::And(atoms) = source.as_mut() else {
        panic!("source conjunction");
    };
    let Formula::Atom { relation, .. } = &mut atoms[0] else {
        panic!("source atom");
    };
    *relation = Term::Iri("relative-predicate".into());
    assert!(lower_recovery_case(&case).is_err());
}

#[test]
fn independently_edited_view_refutes_put_get_when_source_recovery_passes() {
    let get = "CONSTRUCT { ?s <https://example.org/view> <https://example.org/default> }
                   WHERE { ?s <https://example.org/source> <https://example.org/value> }";
    let put = "CONSTRUCT { ?s <https://example.org/source> <https://example.org/value> }
                   WHERE { ?s <https://example.org/view> ?value }";
    let source = SeedGraph::from_iri_atoms(
        "source",
        vec![(
            "https://example.org/item".into(),
            "https://example.org/source".into(),
            "https://example.org/value".into(),
        )],
    );
    assert_eq!(
        discharge_section_law(get, put, &[source]).unwrap().verdict,
        DischargeVerdict::ObligationDischarged
    );
    let view = |value: &str| {
        SeedGraph::from_iri_atoms(
            "edited-view",
            vec![(
                "https://example.org/item".into(),
                "https://example.org/view".into(),
                value.into(),
            )],
        )
    };
    assert_eq!(
        discharge_put_get_law(get, put, &[view("https://example.org/default")])
            .unwrap()
            .verdict,
        DischargeVerdict::ObligationDischarged
    );
    let edited = discharge_put_get_law(get, put, &[view("https://example.org/edited")]).unwrap();
    assert_eq!(edited.verdict, DischargeVerdict::ObligationViolated);
    let countermodel = edited.countermodel.expect("edited value was discarded");
    assert_eq!(countermodel.seed_label, "edited-view");
    assert_eq!(countermodel.missing[0].2, "https://example.org/edited");
    assert_eq!(countermodel.spurious[0].2, "https://example.org/default");
    let claims = discharge_laws(get, put, MorphismClass::SectionRetraction).unwrap();
    assert_eq!(claims[0].verdict, DischargeVerdict::ObligationDischarged);
    assert_eq!(claims[1].verdict, DischargeVerdict::ObligationViolated);
    assert!(
        claims
            .iter()
            .all(|claim| claim.condition == Some(DischargeCondition::DischargeBoundedCorpus))
    );
}

#[test]
fn view_domain_can_be_empty_without_fabricating_put_get_evidence() {
    let get = "CONSTRUCT { ?s <https://example.org/view> ?o }
                   WHERE { ?s <https://example.org/source> ?o }";
    let put = "CONSTRUCT { ?s <https://example.org/source> ?o }
                   WHERE { ?s <https://example.org/view> ?o }";
    let prepared = PreparedLawExecution::new(get, put).unwrap();
    let source = derive_seeds(get).unwrap();
    assert_eq!(
        prepared
            .roundtrip(source.iter().map(LawCase::Seed), true)
            .unwrap()
            .verdict,
        DischargeVerdict::ObligationDischarged
    );
    assert_eq!(
        prepared.roundtrip([], false).unwrap().verdict,
        DischargeVerdict::ObligationUnknown
    );
}

#[test]
fn empty_context_graph_incidence_participates_in_the_law_isomorphism() {
    let carrier = |first| {
        let mut builder = purrdf::RdfDatasetBuilder::new();
        let a = builder
            .intern_blank("a", purrdf::BlankScope::DEFAULT)
            .unwrap();
        let b = builder
            .intern_blank("b", purrdf::BlankScope::DEFAULT)
            .unwrap();
        let predicate = builder.intern_iri("https://example.org/claim").unwrap();
        // A source term occupying the first marker candidate must be preserved.
        let object = builder
            .intern_iri("urn:gmeow:correspondence-carrier:0")
            .unwrap();
        builder.push_quad(a, predicate, object, None).unwrap();
        builder.push_quad(b, predicate, object, None).unwrap();
        builder
            .declare_named_graph(if first { a } else { b })
            .unwrap();
        builder.freeze().unwrap()
    };
    let seed = SeedGraph::from_iri_atoms("symmetric-context", Vec::new());
    let outcome = compare_graphs(&seed, "SectionLaw", &carrier(true), &carrier(false)).unwrap();
    assert_eq!(
        outcome.verdict,
        DischargeVerdict::ObligationDischarged,
        "{outcome:?}"
    );
    assert!(outcome.countermodel.is_none());
}

#[test]
fn a_comparison_refusal_is_an_operational_error() {
    let carrier = |object| {
        SeedGraph::from_iri_atoms(
            "refused-comparison",
            vec![(
                "urn:purrdf:rdfc:reserved".to_owned(),
                "https://example.org/p".to_owned(),
                object,
            )],
        )
        .dataset()
        .unwrap()
    };
    let seed = SeedGraph::from_iri_atoms("refused-comparison", Vec::new());
    let error = compare_graphs(
        &seed,
        "SectionLaw",
        &carrier("https://example.org/a".to_owned()),
        &carrier("https://example.org/b".to_owned()),
    )
    .expect_err("canonicalization refusal is not a law verdict");
    assert!(
        error.inner().source.is_some(),
        "the native cause must survive"
    );
    assert!(format!("{error:#}").contains("canonicalize correspondence carrier"));
}

fn incomplete_law_seed(label: &str, subject: &str) -> SeedGraph {
    SeedGraph::from_iri_atoms(
        label,
        vec![
            (
                subject.to_owned(),
                "urn:source".to_owned(),
                "urn:value".to_owned(),
            ),
            (
                subject.to_owned(),
                "urn:detail".to_owned(),
                "urn:lost".to_owned(),
            ),
        ],
    )
}

const RECOVERY_GET: &str = "CONSTRUCT { ?s <urn:view> ?o } WHERE { ?s <urn:source> ?o }";
const RECOVERY_PUT: &str = "CONSTRUCT { ?s <urn:source> ?o } WHERE { ?s <urn:view> ?o }";

#[test]
fn failed_law_cases_cannot_hide_a_later_real_counterexample() {
    let refused = incomplete_law_seed("a-refused", "urn:purrdf:rdfc:reserved");
    let violated = incomplete_law_seed("b-counterexample", "urn:subject");
    let outcome = discharge_section_law(RECOVERY_GET, RECOVERY_PUT, &[violated, refused])
        .expect("a concrete refutation remains decisive");
    assert_eq!(outcome.verdict, DischargeVerdict::ObligationViolated);
    let countermodel = outcome.countermodel.unwrap();
    assert_eq!(countermodel.seed_label, "b-counterexample");
    assert_eq!(countermodel.missing.len(), 1);
    assert_eq!(countermodel.missing[0].1, "urn:detail");
    assert_eq!(outcome.prior_failures.len(), 1);
    assert_eq!(outcome.prior_failures[0].case, "a-refused");
    assert!(!outcome.prior_failures[0].diagnostic.root.frames.is_empty());
}

#[test]
fn failed_law_cases_cannot_be_disguised_by_a_successful_case() {
    let refused = incomplete_law_seed("a-refused", "urn:purrdf:rdfc:reserved");
    let success = SeedGraph::from_iri_atoms(
        "b-success",
        vec![(
            "urn:subject".to_owned(),
            "urn:source".to_owned(),
            "urn:value".to_owned(),
        )],
    );
    let error = discharge_section_law(RECOVERY_GET, RECOVERY_PUT, &[success, refused])
        .expect_err("one successful case cannot repair an unavailable case");
    assert!(
        error.inner().source.is_some(),
        "single-case failure retains the live native cause"
    );
    assert!(format!("{error:#}").contains("a-refused"));
}

#[test]
fn every_unavailable_law_case_survives_the_diagnostic_boundary() {
    let first = incomplete_law_seed("a-refused", "urn:purrdf:rdfc:first");
    let second = incomplete_law_seed("b-refused", "urn:purrdf:rdfc:second");
    let error = discharge_section_law(RECOVERY_GET, RECOVERY_PUT, &[second, first])
        .expect_err("unavailable cases never manufacture semantic verdicts");
    let recorded = gmeow_errors::DiagLedger::new()
        .record(error, gmeow_errors::StageId::new("law-test-consumer"));
    let observed = recorded.root.observations[0].observed.as_ref().unwrap();
    assert_eq!(
        observed.datatype.as_deref(),
        Some("http://www.w3.org/1999/02/22-rdf-syntax-ns#JSON")
    );
    let failures: Vec<LawCaseFailure> = serde_json::from_str(&observed.lexical).unwrap();
    assert_eq!(
        failures
            .iter()
            .map(|failure| failure.case.as_str())
            .collect::<Vec<_>>(),
        vec!["a-refused", "b-refused"]
    );
    assert!(
        failures
            .iter()
            .all(|failure| !failure.diagnostic.root.frames.is_empty())
    );
}

#[test]
fn recovery_cannot_discard_an_empty_context_graph() {
    let mut source = purrdf::RdfDatasetBuilder::new();
    let graph = source
        .intern_iri("https://example.org/standpoint/withheld")
        .unwrap();
    source.declare_named_graph(graph).unwrap();
    let source = source.freeze().unwrap();
    let recovered = purrdf::RdfDatasetBuilder::new().freeze().unwrap();
    let seed = SeedGraph::from_iri_atoms("empty-context", Vec::new());
    let outcome = compare_graphs(&seed, "SectionLaw", &recovered, &source).unwrap();
    assert_eq!(outcome.verdict, DischargeVerdict::ObligationViolated);
    let witness = outcome.countermodel.unwrap();
    assert_eq!(
        witness.missing_graphs,
        ["https://example.org/standpoint/withheld"]
    );
    assert!(witness.missing.is_empty());
    let inverse = compare_graphs(&seed, "PutGet", &source, &recovered)
        .unwrap()
        .countermodel
        .unwrap();
    assert_eq!(inverse.spurious_graphs, witness.missing_graphs);
}

#[test]
fn carrier_laws_compare_empty_blank_graphs_up_to_isomorphism() {
    let carrier = |label| {
        let mut dataset = purrdf::RdfDatasetBuilder::new();
        let graph = dataset
            .intern_blank(label, purrdf::BlankScope::DEFAULT)
            .unwrap();
        dataset.declare_named_graph(graph).unwrap();
        dataset.freeze().unwrap()
    };
    let seed = SeedGraph::from_iri_atoms("empty-context", Vec::new());
    let outcome = compare_graphs(&seed, "GetPut", &carrier("old"), &carrier("new")).unwrap();
    assert_eq!(outcome.verdict, DischargeVerdict::ObligationDischarged);
}

#[test]
fn directional_literal_seed_round_trips_through_the_law_executor() {
    let get = r#"CONSTRUCT { ?s <urn:view:label> "claim"@ar--rtl }
            WHERE { ?s <urn:source:label> "claim"@ar--rtl }"#;
    let put = r#"CONSTRUCT { ?s <urn:source:label> "claim"@ar--rtl }
            WHERE { ?s <urn:view:label> "claim"@ar--rtl }"#;
    let seeds = derive_seeds(get).unwrap();
    assert!(!seeds.is_empty());
    let outcome = discharge_section_law(get, put, &seeds).unwrap();
    assert_eq!(
        outcome.verdict,
        DischargeVerdict::ObligationDischarged,
        "{outcome:#?}"
    );
    let wrong_direction = put.replace("--rtl", "--ltr");
    assert_eq!(
        discharge_section_law(get, &wrong_direction, &seeds)
            .unwrap()
            .verdict,
        DischargeVerdict::ObligationViolated
    );
}

#[test]
fn correspondence_graph_scope_is_executed_and_compared() {
    let get = "CONSTRUCT { GRAPH ?g { ?s <urn:view:p> ?o } }
            WHERE { GRAPH ?g { ?s <urn:source:p> ?o } }";
    let put = "CONSTRUCT { GRAPH ?g { ?s <urn:source:p> ?o } }
            WHERE { GRAPH ?g { ?s <urn:view:p> ?o } }";
    let seeds = derive_seeds(get).unwrap();
    assert!(!seeds.is_empty());
    assert!(
        seeds
            .iter()
            .all(|seed| seed.quads.iter().all(|quad| quad.graph_name.is_some()))
    );
    assert_eq!(
        discharge_section_law(get, put, &seeds).unwrap().verdict,
        DischargeVerdict::ObligationDischarged
    );
    let put_in_default = "CONSTRUCT { ?s <urn:source:p> ?o }
            WHERE { GRAPH ?g { ?s <urn:view:p> ?o } }";
    let outcome = discharge_section_law(get, put_in_default, &seeds).unwrap();
    assert_eq!(outcome.verdict, DischargeVerdict::ObligationViolated);
    let countermodel = outcome.countermodel.expect("graph loss witness");
    assert!(countermodel.missing.iter().all(|atom| atom.3.is_some()));
    assert!(countermodel.spurious.iter().all(|atom| atom.3.is_none()));
}

#[test]
fn quoted_recovery_seed_is_not_discarded_or_coerced_to_an_iri() {
    let get = r#"CONSTRUCT { ?claim <urn:view:evidence> ?value }
            WHERE { ?claim <urn:source:evidence> <<( ?agent <urn:asserted> "claim"@ar--rtl )>> }
        "#;
    let seeds = derive_seeds(get).unwrap();
    assert!(!seeds.is_empty());
    let RdfTerm::Triple(triple) = &seeds[0].quads[0].object else {
        panic!("quoted evidence must remain a triple term");
    };
    let RdfTerm::Literal(literal) = &triple.object else {
        panic!("quoted claim must retain its literal");
    };
    assert_eq!(literal.direction, Some(purrdf::RdfTextDirection::Rtl));
    // The view omits the quoted claim entirely; a seed extractor that discards
    // quoted patterns would hide this loss behind an empty corpus.
    let put = "CONSTRUCT { ?claim <urn:source:evidence> ?value }
            WHERE { ?claim <urn:view:evidence> ?value }";
    assert_eq!(
        discharge_section_law(get, put, &seeds).unwrap().verdict,
        DischargeVerdict::ObligationViolated
    );
}

#[test]
fn correspondence_literal_keys_preserve_direction_and_lexical_boundaries() {
    let literal = |direction| {
        RdfTerm::Literal(purrdf::RdfLiteral {
            lexical_form: "claim\"@ar".into(),
            datatype: None,
            language: Some("ar".into()),
            direction,
        })
    };
    assert_ne!(
        term_key(&literal(Some(purrdf::RdfTextDirection::Ltr))),
        term_key(&literal(Some(purrdf::RdfTextDirection::Rtl)))
    );
    assert_ne!(
        term_key(&literal(None)),
        term_key(&RdfTerm::Literal(purrdf::RdfLiteral::simple(
            "claim\"@ar\"@ar"
        )))
    );
}

fn step(predicate: &str) -> LegPath {
    LegPath::Step(predicate.to_owned())
}

fn atom(predicate: &str, subject: Term, object: Term) -> Formula {
    Formula::atom(
        Term::iri(predicate).expect("predicate IRI"),
        vec![subject, object],
    )
    .expect("binary atom")
}

fn recovery_case(view_keeps_detail: bool) -> RecoveryCaseIr {
    let subject = Term::var("subject").expect("subject variable");
    let detail = Term::var("detail").expect("detail variable");
    let source = Formula::And(vec![
        atom(
            "https://example.org/sourceKind",
            subject.clone(),
            Term::iri("https://example.org/Language").expect("class IRI"),
        ),
        atom(
            "https://example.org/sourceDetail",
            subject.clone(),
            detail.clone(),
        ),
    ]);
    let mut view = vec![atom(
        "https://example.org/viewKind",
        subject.clone(),
        Term::iri("https://example.org/SignSystem").expect("class IRI"),
    )];
    if view_keeps_detail {
        view.push(atom("https://example.org/viewDetail", subject, detail));
    }
    RecoveryCaseIr::new(
        "https://example.org/recovery/case",
        Formula::Forall {
            vars: vec!["subject".to_owned(), "detail".to_owned()],
            body: Box::new(Formula::Implies(
                Box::new(source),
                Box::new(Formula::And(view)),
            )),
        },
    )
    .expect("recovery case")
}

fn recovery_correspondence(case: RecoveryCaseIr) -> Correspondence {
    Correspondence::new(
        "https://example.org/correspondence",
        CorrespondenceRelation::Subsumes,
        MorphismClass::SectionRetraction,
        MorphismKind::InstitutionMorphism,
        true,
        None,
        Some("https://example.org/get".to_owned()),
        Some("https://example.org/put".to_owned()),
        Vec::new(),
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .expect("correspondence")
    .with_recovery_cases(vec![case])
    .expect("case")
}

#[test]
fn atomic_inverse_recovers_the_real_source_predicate() {
    let get = step("https://example.org/source");
    assert_eq!(
        leg_pair_verdict(&get, &get.invert()).unwrap(),
        DischargeVerdict::ObligationDischarged
    );
}

#[test]
fn wrong_atomic_put_yields_a_real_missing_and_spurious_difference() {
    assert_eq!(
        leg_pair_verdict(
            &step("https://example.org/source"),
            &step("https://example.org/wrong")
        )
        .unwrap(),
        DischargeVerdict::ObligationViolated
    );
}

#[test]
fn composite_path_is_unknown_without_a_complete_recovery_case() {
    let get = LegPath::Seq(vec![
        step("https://example.org/a"),
        step("https://example.org/b"),
    ]);
    assert_eq!(
        leg_pair_verdict(&get, &get.invert()).unwrap(),
        DischargeVerdict::ObligationUnknown
    );
}

#[test]
fn recovery_formula_discharges_only_when_the_view_retains_every_source_variable() {
    let get = step("https://example.org/sourceDetail");
    let put = get.invert();
    let good = discharge_recovery_case(&recovery_case(true), &get, &put).unwrap();
    assert_eq!(
        good.verdict,
        DischargeVerdict::ObligationDischarged,
        "{good:#?}"
    );

    let bad = discharge_recovery_case(&recovery_case(false), &get, &put).unwrap();
    assert_eq!(
        bad.verdict,
        DischargeVerdict::ObligationViolated,
        "{bad:#?}"
    );
    let countermodel = bad.countermodel.expect("loss has a countermodel");
    assert_eq!(countermodel.missing.len(), 1, "{countermodel:#?}");
    assert!(countermodel.spurious.is_empty(), "{countermodel:#?}");
}

#[test]
fn shared_recovery_legs_do_not_reuse_another_cases_success_or_bindings() {
    let get = step("https://example.org/sourceDetail");
    let put = get.invert();
    let legs = PreparedRecoveryLegs::new(&get, &put).unwrap();
    let good = recovery_case(true);
    let mut lossy = recovery_case(false);
    lossy.iri = "https://example.org/recovery/lossy".into();
    let mut unrelated = recovery_case(true);
    unrelated.iri = "https://example.org/recovery/unrelated".into();
    let Formula::Forall { body, .. } = &mut unrelated.transform else {
        panic!("quantified recovery case");
    };
    let Formula::Implies(source, _) = body.as_mut() else {
        panic!("recovery implication");
    };
    let Formula::And(atoms) = source.as_mut() else {
        panic!("source conjunction");
    };
    let Formula::Atom { relation, .. } = &mut atoms[1] else {
        panic!("source detail atom");
    };
    *relation = Term::Iri("https://example.org/otherDetail".into());

    for case in [&good, &lossy, &unrelated, &good] {
        assert_eq!(
            legs.discharge(case).unwrap(),
            discharge_recovery_case(case, &get, &put).unwrap(),
            "preparation reuse must preserve each case's evidence and countermodel"
        );
    }
    assert_eq!(
        legs.discharge(&good).unwrap().verdict,
        DischargeVerdict::ObligationDischarged
    );
    let unrelated_result = legs.discharge(&unrelated).unwrap();
    assert_eq!(
        unrelated_result.verdict,
        DischargeVerdict::ObligationViolated
    );
    assert!(
        unrelated_result
            .countermodel
            .unwrap()
            .reason
            .contains("produced no relation")
    );
    let correspondence = recovery_correspondence(good.clone())
        .with_recovery_cases(vec![lossy.clone(), good])
        .unwrap();
    assert_eq!(
        discharge_recovery_cases(&correspondence, &get, &put).unwrap(),
        legs.discharge(&lossy).unwrap(),
        "one successful case cannot hide a later loss"
    );
}

#[test]
fn recovery_formula_literal_endpoints_fail_closed() {
    let subject = Term::var("subject").expect("subject variable");
    let source = atom(
        "https://example.org/sourceValue",
        subject.clone(),
        Term::literal("source", None).expect("source literal"),
    );
    let view = atom(
        "https://example.org/viewValue",
        subject,
        Term::literal("view", None).expect("view literal"),
    );
    let case = RecoveryCaseIr::new(
        "https://example.org/recovery/literal-endpoint",
        Formula::Forall {
            vars: vec!["subject".to_owned()],
            body: Box::new(Formula::Implies(Box::new(source), Box::new(view))),
        },
    )
    .expect("literal-endpoint recovery case");
    let get = step("https://example.org/sourceValue");

    let error = discharge_recovery_case(&case, &get, &get.invert())
        .expect_err("inadmissible recovery input must not fabricate a countermodel");
    assert!(
        error
            .message()
            .contains("literals are outside the recovery-case RDF-atom fragment"),
        "{error}"
    );
}

#[test]
fn recovery_case_colliding_with_the_reserved_recovery_namespace_never_discharges() {
    // The view predicate is authored as the SAME IRI the executor generates internally
    // (`VIEW_PREDICATE`).  Without the reserved-namespace guard this collision would make
    // the mechanically synthesized view carrier indistinguishable from the authored view
    // atom in the seed graph, and the atom-set comparison in `discharge_section_law` could
    // FALSELY discharge a lossy correspondence.  The guard must reject it before any seed
    // is built.
    let subject = Term::var("subject").expect("subject variable");
    let object = Term::var("object").expect("object variable");
    let source = atom(
        "https://example.org/sourceKind",
        subject.clone(),
        object.clone(),
    );
    let view = atom(VIEW_PREDICATE, subject, object);
    let case = RecoveryCaseIr::new(
        "https://example.org/recovery/reserved-namespace-collision",
        Formula::Forall {
            vars: vec!["subject".to_owned(), "object".to_owned()],
            body: Box::new(Formula::Implies(Box::new(source), Box::new(view))),
        },
    )
    .expect("recovery case");

    let get = step("https://example.org/sourceKind");
    let error = discharge_recovery_case(&case, &get, &get.invert())
        .expect_err("inadmissible recovery input must not fabricate a countermodel");
    assert!(
        error
            .message()
            .contains("reserved recovery-execution namespace"),
        "{error}"
    );
}

#[test]
fn canonical_recovery_vocabulary_outside_the_execution_namespaces_remains_usable() {
    let subject = Term::var("subject").expect("subject variable");
    let object = Term::var("object").expect("object variable");
    let predicate = "https://blackcatinformatics.ca/logic/recoveryTransform";
    let source = atom(predicate, subject.clone(), object.clone());
    let view = atom("https://example.org/viewKind", subject, object);
    let case = RecoveryCaseIr::new(
        "https://example.org/recovery/canonical-vocabulary-prefix",
        Formula::Forall {
            vars: vec!["subject".to_owned(), "object".to_owned()],
            body: Box::new(Formula::Implies(Box::new(source), Box::new(view))),
        },
    )
    .expect("recovery case");

    let get = step(predicate);
    let outcome = discharge_recovery_case(&case, &get, &get.invert()).unwrap();
    assert_eq!(
        outcome.verdict,
        DischargeVerdict::ObligationDischarged,
        "canonical logic:recovery* terms outside the generated execution namespaces must \
             not be rejected by a raw string-prefix collision guard: {outcome:#?}"
    );
}

#[test]
fn program_requires_both_recovery_evidence_and_the_resolved_leg_bodies() {
    let correspondence = recovery_correspondence(recovery_case(false));
    let program = CorrespondenceProgram::new(vec![correspondence], PreservationKind::SoundUnder)
        .with_leg_programs(vec![
            TransactionProgramIr {
                iri: "https://example.org/get".to_owned(),
                body: step("https://example.org/source"),
            },
            TransactionProgramIr {
                iri: "https://example.org/put".to_owned(),
                body: step("https://example.org/source").invert(),
            },
        ]);
    assert_eq!(
        program_verdicts(&program).unwrap()["https://example.org/correspondence"]
            .section
            .verdict,
        DischargeVerdict::ObligationViolated,
        "the mechanically perfect path pair must not override a refuting source case"
    );
}

#[test]
fn mutating_only_the_resolved_get_body_refutes_a_fixed_recovery_case() {
    let source_detail = step("https://example.org/sourceDetail");
    let program = CorrespondenceProgram::new(
        vec![recovery_correspondence(recovery_case(true))],
        PreservationKind::SoundUnder,
    )
    .with_leg_programs(vec![
        TransactionProgramIr {
            iri: "https://example.org/get".to_owned(),
            body: source_detail.clone(),
        },
        TransactionProgramIr {
            iri: "https://example.org/put".to_owned(),
            body: source_detail.invert(),
        },
    ]);
    assert_eq!(
        program_verdicts(&program).unwrap()["https://example.org/correspondence"]
            .section
            .verdict,
        DischargeVerdict::ObligationDischarged
    );

    let mut mutated = program.clone();
    mutated
        .leg_programs
        .iter_mut()
        .find(|leg| leg.iri == "https://example.org/get")
        .expect("get body")
        .body = step("https://example.org/unrelatedSource");
    assert_eq!(
        program_verdicts(&mutated).unwrap()["https://example.org/correspondence"]
            .section
            .verdict,
        DischargeVerdict::ObligationViolated,
        "the unchanged recovery case cannot discharge after only the formerly inert get \
             body changes"
    );
}

#[test]
fn honest_lossy_recovery_requires_legs_and_keeps_its_refutation() {
    use gmeow_logic_compile::projections::correspondence_gates::{assert_gates, evaluate_gates};

    let mut correspondence = recovery_correspondence(recovery_case(false));
    correspondence.relation = CorrespondenceRelation::RelatedMatch;
    correspondence.morphism_class = MorphismClass::LossyLens;
    correspondence.mnemomorphic = false;
    let incomplete = CorrespondenceProgram::new(vec![correspondence], PreservationKind::SoundUnder);
    assert!(program_verdicts(&incomplete).is_err());

    let get = step("https://example.org/sourceKind");
    let complete = incomplete.with_leg_programs(vec![
        TransactionProgramIr {
            iri: "https://example.org/get".to_owned(),
            body: get.clone(),
        },
        TransactionProgramIr {
            iri: "https://example.org/put".to_owned(),
            body: get.invert(),
        },
    ]);
    let verdicts = program_verdicts(&complete).expect("execute the lossy recovery case");
    assert_eq!(
        verdicts["https://example.org/correspondence"]
            .section
            .verdict,
        DischargeVerdict::ObligationViolated,
        "supplying executable paths must not erase the lost source detail"
    );
    assert_gates(&evaluate_gates(&complete, &[], &verdicts))
        .expect("an honestly declared lossy mapping may retain a recovery refutation");

    let mut overclaim = complete;
    overclaim.correspondences[0].morphism_class = MorphismClass::SectionRetraction;
    overclaim.correspondences[0].mnemomorphic = true;
    assert!(assert_gates(&evaluate_gates(&overclaim, &[], &verdicts)).is_err());
}

#[test]
fn recovery_evidence_with_a_missing_resolved_leg_fails_closed() {
    let program = CorrespondenceProgram::new(
        vec![recovery_correspondence(recovery_case(true))],
        PreservationKind::SoundUnder,
    )
    .with_leg_programs(vec![TransactionProgramIr {
        iri: "https://example.org/get".to_owned(),
        body: step("https://example.org/sourceDetail"),
    }]);
    let error = program_verdicts(&program).expect_err("missing legs cannot produce verdicts");
    assert_eq!(
        error.inner().source_ctx.focus.as_ref().unwrap().0,
        "https://example.org/correspondence"
    );
}

#[test]
fn malformed_recovery_formula_fails_closed_before_leg_execution() {
    let subject = Term::var("subject").expect("subject variable");
    let object = Term::var("object").expect("object variable");
    let case = RecoveryCaseIr::new(
        "https://example.org/recovery/malformed",
        atom("https://example.org/sourceDetail", subject, object),
    )
    .expect("recovery case carrier");
    let get = step("https://example.org/sourceDetail");
    assert!(discharge_recovery_case(&case, &get, &get.invert()).is_err());
}

#[test]
fn complete_composite_recovery_executes_the_resolved_path_bodies() {
    let subject = Term::var("subject").expect("subject variable");
    let middle = Term::var("middle").expect("middle variable");
    let object = Term::var("object").expect("object variable");
    let source = Formula::And(vec![
        atom("https://example.org/a", subject.clone(), middle.clone()),
        atom("https://example.org/b", middle.clone(), object.clone()),
    ]);
    let view = Formula::And(vec![
        atom("https://example.org/viewEndpoint", subject.clone(), object),
        atom("https://example.org/viewWitness", subject, middle),
    ]);
    let case = RecoveryCaseIr::new(
        "https://example.org/recovery/composite",
        Formula::Forall {
            vars: vec![
                "subject".to_owned(),
                "middle".to_owned(),
                "object".to_owned(),
            ],
            body: Box::new(Formula::Implies(Box::new(source), Box::new(view))),
        },
    )
    .expect("composite recovery case");
    let get = LegPath::Seq(vec![
        step("https://example.org/a"),
        step("https://example.org/b"),
    ]);
    let outcome = discharge_recovery_case(&case, &get, &get.invert()).unwrap();
    assert_eq!(
        outcome.verdict,
        DischargeVerdict::ObligationDischarged,
        "{outcome:#?}"
    );
}

#[test]
fn branch_seed_derivation_covers_plain_and_union_queries() {
    let plain = "CONSTRUCT { ?s <http://view/p> ?o } WHERE { ?s <http://source/p> ?o . }";
    let seeds = derive_seeds(plain).unwrap();
    assert_eq!(seeds.len(), 2, "branch plus combined: {seeds:#?}");

    let union = "CONSTRUCT { ?s <http://view/p> ?o } WHERE { { ?s <http://source/a> ?o . } UNION { ?s <http://source/b> ?o . } }";
    let seeds = derive_seeds(union).unwrap();
    assert_eq!(
        seeds
            .iter()
            .map(|seed| seed.label.as_str())
            .collect::<Vec<_>>(),
        vec!["branch-0", "branch-1", "combined"]
    );
}

#[test]
fn inadmissible_recovery_queries_cannot_supply_an_empty_domain() {
    for query in [
        "this is not valid SPARQL {{{",
        "SELECT ?s WHERE { ?s <http://ex.example/p> ?o }",
    ] {
        assert!(derive_seeds(query).is_err());
        assert!(discharge_laws(query, query, MorphismClass::SectionRetraction).is_err());
    }
}

#[test]
fn recovery_admission_bounds_cartesian_branch_expansion() {
    // Thirteen binary choices would expand to 8192 cases before adding the
    // combined case. Bound this GMEOW synthesis, not native SPARQL execution.
    let choices = (0..13)
        .map(|index| {
            format!(
                "{{ ?s <urn:source:{index}:left> ?o }} UNION {{ ?s <urn:source:{index}:right> ?o }}"
            )
        })
        .map(|choice| format!("{{ {choice} }}"))
        .collect::<Vec<_>>()
        .join(" ");
    let query = format!("CONSTRUCT {{ ?s <urn:view> ?o }} WHERE {{ {choices} }}");
    let error = derive_seeds(&query).unwrap_err();
    assert!(error.message().contains("admission limits"), "{error}");
    assert!(discharge_laws(&query, &query, MorphismClass::SectionRetraction).is_err());
}

#[test]
fn recovery_admission_bounds_repeated_patterns_before_distribution() {
    let patterns = vec![
        SparqlTriplePattern {
            subject: TermPattern::Variable(Variable::new("s")),
            predicate: NamedNodePattern::NamedNode(algebra_iri("urn:source").unwrap()),
            object: TermPattern::Variable(Variable::new("o")),
        };
        MAX_SEED_PATTERNS / 2 + 1
    ];
    let choices = GraphPattern::union(
        GraphPattern::Bgp { patterns: vec![] },
        GraphPattern::Bgp { patterns: vec![] },
    );
    let pattern = GraphPattern::Join {
        left: Child::new(GraphPattern::Bgp { patterns }),
        right: Child::new(choices),
    };
    let error = match dnf_branches(&pattern, 0) {
        Ok(_) => panic!("duplicated pattern inventory must exceed admission"),
        Err(error) => error,
    };
    assert!(error.message().contains("admission limits"), "{error}");
}

#[test]
fn recovery_admission_bounds_nested_algebra_without_truncating_it() {
    let mut pattern = GraphPattern::Bgp { patterns: vec![] };
    for _ in 0..=MAX_SEED_ALGEBRA_DEPTH {
        pattern = GraphPattern::Distinct {
            inner: Child::new(pattern),
        };
    }
    let error = match dnf_branches(&pattern, 0) {
        Ok(_) => panic!("excessive source depth must fail admission"),
        Err(error) => error,
    };
    assert!(error.message().contains("algebra exceeds depth"), "{error}");
}

// The WHERE parser must treat a dot inside a full `<IRI>` as IRI content, not
// as a triple-pattern separator. A real
// SPARQL predicate IRI is atomic to the parser regardless of embedded dots, so it must
// survive into the seed as ONE triple pattern, not be chopped into garbage statements.
#[test]
fn full_dotted_iri_predicate_is_not_mis_split() {
    let get = "CONSTRUCT { ?s <http://view.example/p> ?o } \
                    WHERE { ?s <http://ex.example/p.q> ?o . }";
    let seeds = derive_seeds(get).unwrap();
    let branch = seeds
        .iter()
        .find(|seed| seed.label == "branch-0")
        .expect("one branch for the single BGP");
    assert_eq!(
        branch.quads.len(),
        1,
        "the dotted-IRI predicate triple must survive as exactly one atom: {branch:#?}"
    );
    assert_eq!(branch.quads[0].predicate, "http://ex.example/p.q");
}

// A triple pattern joined OUTSIDE a `UNION` (`?s a ex:C .` here) must be
// distributed into EVERY branch, not dropped. The branch normalizer
// extracted patterns found INSIDE `{...}` groups, silently losing this shared atom.
#[test]
fn triple_pattern_shared_outside_union_appears_in_every_branch() {
    let get = "PREFIX ex: <http://ex.example/> \
                    CONSTRUCT { ?s <http://view.example/p> ?o } \
                    WHERE { ?s a ex:C . { ?s ex:r1 ?o } UNION { ?s ex:r2 ?o } }";
    let seeds = derive_seeds(get).unwrap();
    let branch_labels: Vec<&str> = seeds
        .iter()
        .filter(|seed| seed.label.starts_with("branch-"))
        .map(|seed| seed.label.as_str())
        .collect();
    assert_eq!(branch_labels, vec!["branch-0", "branch-1"], "{seeds:#?}");
    let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    for label in ["branch-0", "branch-1"] {
        let branch = seeds
            .iter()
            .find(|seed| seed.label == label)
            .unwrap_or_else(|| panic!("{label} present"));
        assert_eq!(branch.quads.len(), 2, "{branch:#?}");
        assert!(
            branch.quads.iter().any(|quad| quad.predicate == rdf_type
                && quad.object == RdfTerm::iri("http://ex.example/C")),
            "the shared `?s a ex:C` atom must appear in {label}: {branch:#?}"
        );
    }
}

// A literal value containing a dot must not be split by any character-level pass; the
// real parser hands us the literal's lexical form as one atomic token.
#[test]
fn dotted_literal_object_is_not_mis_split() {
    let get = "CONSTRUCT { ?s <http://view.example/p> ?o } \
                    WHERE { ?s <http://src.example/value> \"3.14\" . }";
    let seeds = derive_seeds(get).unwrap();
    let branch = seeds
        .iter()
        .find(|seed| seed.label == "branch-0")
        .expect("one branch for the single BGP");
    assert_eq!(branch.quads.len(), 1, "{branch:#?}");
    assert_eq!(
        branch.quads[0].object,
        RdfTerm::literal(RdfLiteral::typed(
            "3.14",
            "http://www.w3.org/2001/XMLSchema#string"
        ))
    );
}
