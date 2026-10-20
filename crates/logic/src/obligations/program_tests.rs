// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Actual native-program, source-context and evidence-admission controls.

use super::*;
use gmeow_logic_compile::ir::{Formula, LogicProgram, Term};

fn atom(subject: &str, predicate: &str, object: &str) -> Formula {
    let term = |value: &str| match value.strip_prefix('?') {
        Some(name) => Term::var(name).unwrap(),
        None => Term::iri(value).unwrap(),
    };
    Formula::atom(
        Term::iri(predicate).unwrap(),
        vec![term(subject), term(object)],
    )
    .unwrap()
}

fn rule(head: Formula) -> Formula {
    Formula::Forall {
        vars: vec!["x".into(), "y".into()],
        body: Box::new(Formula::Implies(
            Box::new(atom("?x", "https://example.org/seed", "?y")),
            Box::new(head),
        )),
    }
}

fn program(formulas: Vec<Formula>) -> LogicProgram {
    LogicProgram::new(vec![], vec![], vec![], None).with_formulas(formulas)
}

fn source(body: &str) -> Arc<RdfDataset> {
    tests::governance_store(body)
}

fn declaration(forbidden: &str, finite: bool) -> String {
    format!(
        "ex:obligation logic:instanceOf logic:NonEntailmentObligation ; \
         logic:obligationForbiddenPredicate <{forbidden}> ; \
         logic:obligationDischargeCondition logic:DischargeSyntacticReachability{} .",
        if finite {
            ", logic:DischargeFiniteClosure"
        } else {
            ""
        },
    )
}

fn run(program: &LogicProgram, source: &Arc<RdfDataset>) -> crate::result::ReasoningResult {
    crate::reason::reason_program(
        program,
        crate::reason::prepare_reasoning_input(source).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
    )
    .unwrap()
}

fn status(result: &crate::result::ReasoningResult) -> ObligationDisposition {
    let assessment = assess_non_entailment_obligations(result).unwrap();
    assert_eq!(assessment.outcomes.len(), 1, "{assessment:?}");
    assessment.outcomes[0].disposition.clone()
}

#[test]
fn native_schema_consequences_of_nonfiring_authored_heads_are_possible() {
    let forbidden = "https://example.org/forbidden";
    let store = source(&format!(
        "{} ex:edge logic:subPropertyOf ex:forbidden .",
        declaration(forbidden, false),
    ));
    let result = run(
        &program(vec![rule(atom("?x", "https://example.org/edge", "?y"))]),
        &store,
    );
    assert!(
        !result
            .inferred()
            .iter()
            .any(|row| !row.is_edb && row.predicate == forbidden)
    );
    assert_eq!(status(&result), ObligationDisposition::Unknown);
    let assessment = assess_non_entailment_obligations(&result).unwrap();
    assert!(
        assessment.findings().any(|finding| finding
            .detail
            .as_ref()
            .is_some_and(|detail| detail.contains("dl:subPropertyOf"))),
        "{assessment:?}"
    );
}

#[test]
fn possible_open_schema_selector_cannot_certify_an_absent_predicate() {
    let forbidden = "https://example.org/previouslyUnseen";
    let store = source(&format!(
        "{} ex:s ex:edge ex:o .",
        declaration(forbidden, false)
    ));
    let result = run(
        &program(vec![rule(atom(
            "?x",
            "https://blackcatinformatics.ca/logic/subPropertyOf",
            "?y",
        ))]),
        &store,
    );
    assert!(
        !result
            .inferred()
            .iter()
            .any(|row| !row.is_edb && row.predicate == forbidden)
    );
    assert_eq!(status(&result), ObligationDisposition::Unknown);
}

#[test]
fn excluded_formula_prevents_absence_but_preserves_supported_counterexamples() {
    let forbidden = "https://example.org/forbidden";
    let excluded = Formula::Or(vec![
        atom("?x", "urn:excluded:first", "?y"),
        atom("?x", "urn:excluded:second", "?y"),
    ]);
    for seed in [false, true] {
        let store = source(&format!(
            "{} {}",
            declaration(forbidden, true),
            if seed { "ex:s ex:seed ex:o ." } else { "" }
        ));
        let result = run(
            &program(vec![rule(atom("?x", forbidden, "?y")), excluded.clone()]),
            &store,
        );
        assert!(!result.preservation.unsupported_constructs.is_empty());
        match status(&result) {
            ObligationDisposition::Violated(row) if seed => assert_eq!(row.predicate, forbidden),
            ObligationDisposition::Unknown if !seed => {}
            other => panic!("seed={seed}: {other:?}"),
        }
    }
}

#[test]
fn foreign_world_counterexamples_do_not_violate_a_local_obligation() {
    let source = format!(
        "@prefix logic: <{LOGIC_NS}> . @prefix ex: <https://example.org/> . \
         ex:local {{ {} }} \
         ex:foreign {{ {} ex:edge logic:subPropertyOf ex:forbidden . ex:s ex:edge ex:o . }}",
        declaration("https://example.org/forbidden", false),
        declaration("https://example.org/forbidden", false),
    );
    let store = purrdf::parse_dataset(source.as_bytes(), "application/trig", None).unwrap();
    let result = run(&program(vec![]), &store);
    let assessment = assess_non_entailment_obligations(&result).unwrap();
    assert_eq!(assessment.outcomes.len(), 2, "{assessment:?}");
    let local = assessment
        .outcomes
        .iter()
        .find(|outcome| outcome.world == "https://example.org/local")
        .unwrap();
    assert_eq!(
        local.disposition,
        ObligationDisposition::SyntacticallyDischarged
    );
    let foreign = assessment
        .outcomes
        .iter()
        .find(|outcome| outcome.world == "https://example.org/foreign")
        .unwrap();
    assert!(matches!(
        foreign.disposition,
        ObligationDisposition::Violated(_)
    ));
}

#[test]
fn unsupported_conditions_retain_distinct_worlds_and_native_outcomes() {
    let declaration = "ex:obligation logic:instanceOf logic:NonEntailmentObligation ;
        logic:obligationForbiddenPredicate ex:forbidden ;
        logic:obligationDischargeCondition logic:Unwired,
            <https://blackcatinformatics.ca/logic/Unwired:Condition> .
        ex:candidate logic:instanceOf logic:FormalizationCandidate ;
            logic:candidateNonEntailment ex:obligation .";
    let text = format!(
        "@prefix logic: <{LOGIC_NS}> . @prefix ex: <https://example.org/> .
         ex:first {{ {declaration} }} ex:second {{ {declaration} }}"
    );
    let store = purrdf::parse_dataset(text.as_bytes(), "application/trig", None).unwrap();
    let result = run(&program(vec![]), &store);
    let assessment = assess_non_entailment_obligations(&result).unwrap();
    assert_eq!(assessment.outcomes.len(), 2);
    assert!(assessment.outcomes.iter().all(|outcome| {
        outcome.obligation == "https://example.org/obligation"
            && outcome.disposition == ObligationDisposition::InvalidDischarge
    }));
    let native_outcomes = assessment.outcomes.clone();
    let analysis = assessment.analysis;
    let mut projection = assessment.findings().collect::<Vec<_>>();
    let errors = projection
        .iter()
        .filter(|finding| finding.severity == Severity::Error)
        .collect::<Vec<_>>();
    assert_eq!(errors.len(), 4);
    assert_eq!(
        errors
            .iter()
            .map(|finding| finding.finding_iri.as_ref().unwrap())
            .collect::<BTreeSet<_>>()
            .len(),
        4
    );
    for world in ["https://example.org/first", "https://example.org/second"] {
        for condition in [
            "https://blackcatinformatics.ca/logic/Unwired",
            "https://blackcatinformatics.ca/logic/Unwired:Condition",
        ] {
            let finding = errors
                .iter()
                .find(|finding| {
                    finding.source_graph.as_ref().map(|graph| graph.as_str()) == Some(world)
                        && finding.cited_iris.iter().any(|iri| iri == condition)
                })
                .unwrap();
            assert_eq!(
                finding.primary_location().unwrap().logical.as_deref(),
                Some("https://example.org/obligation")
            );
            assert!(
                finding
                    .cited_iris
                    .iter()
                    .any(|iri| iri == "https://example.org/candidate")
            );
        }
    }
    let inventories = projection
        .iter()
        .filter(|finding| finding.code == "verify.non-entailment.inventory")
        .collect::<Vec<_>>();
    let worlds = &result.native_execution().unwrap().families;
    assert_eq!(inventories.len(), worlds.len());
    for world in worlds {
        let inventory = inventories
            .iter()
            .find(|finding| {
                finding.primary_location().unwrap().logical.as_deref() == Some(world.world.as_str())
            })
            .unwrap();
        assert_eq!(inventory.severity, Severity::Note);
        assert_eq!(inventory.category, Some(FindingCategory::PolicyWarning));
    }
    projection.clear();
    assert_eq!(assessment.findings().count(), 4 + worlds.len());
    assert_eq!(assessment.outcomes, native_outcomes);
    assert_eq!(assessment.analysis, analysis);
    assert_eq!(
        assessment.ledger().verdict(),
        gmeow_errors::GateVerdict::Fatal
    );
}

#[test]
fn foreign_or_changed_program_analysis_is_refused() {
    let store = source(&declaration("https://example.org/forbidden", false));
    let first = run(&program(vec![]), &store);
    let changed = run(
        &program(vec![rule(atom(
            "?x",
            "https://example.org/forbidden",
            "?y",
        ))]),
        &store,
    );
    assert_eq!(
        status(&first),
        ObligationDisposition::SyntacticallyDischarged
    );
    assert_eq!(status(&changed), ObligationDisposition::Unknown);
    let mut transplanted = changed.clone();
    transplanted
        .provenance
        .native_execution
        .as_mut()
        .unwrap()
        .reachability = first.native_execution().unwrap().reachability.clone();
    let error =
        assess_non_entailment_obligations(&transplanted).expect_err("foreign analysis must refuse");
    assert!(error.to_string().contains("stale or foreign"), "{error}");
    let mut foreign_engine = first;
    foreign_engine
        .provenance
        .engine
        .version
        .push_str("-foreign");
    assert!(assess_non_entailment_obligations(&foreign_engine).is_err());
}

#[test]
fn changed_import_input_and_engine_recipe_invalidate_analysis() {
    let declaration = declaration("https://example.org/forbidden", false);
    let base = run(&program(vec![]), &source(&declaration));
    let mut imported = program(vec![]);
    imported.source_iri = Some("urn:import:other-canonical-program".to_owned());
    let changed_import = run(&imported, &source(&declaration));
    assert_ne!(
        assess_non_entailment_obligations(&base).unwrap().analysis,
        assess_non_entailment_obligations(&changed_import)
            .unwrap()
            .analysis
    );
    let mut changed_source = run(
        &program(vec![]),
        &source(&format!("{declaration} ex:import ex:version ex:changed .")),
    );
    changed_source
        .provenance
        .native_execution
        .as_mut()
        .unwrap()
        .reachability = base.native_execution().unwrap().reachability.clone();
    assert!(assess_non_entailment_obligations(&changed_source).is_err());
    let mut stale_recipe = base;
    stale_recipe.provenance.contract_hash.push_str("-stale");
    assert!(assess_non_entailment_obligations(&stale_recipe).is_err());
}

#[test]
fn budgeted_closure_cannot_discharge_finite_absence() {
    let store = source(&declaration("https://example.org/forbidden", true));
    let (result, _, _) = crate::reason::reason_program_budgeted(
        &program(vec![rule(atom(
            "?x",
            "https://example.org/forbidden",
            "?y",
        ))]),
        crate::reason::prepare_reasoning_input(&store).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        Some(0),
    )
    .unwrap();
    assert_eq!(status(&result), ObligationDisposition::Unknown);
}

#[test]
fn zero_budget_does_not_mistake_missing_schema_admission_for_an_empty_source() {
    for with_schema in [false, true] {
        let store = source(&format!(
            "{} {}",
            declaration("https://example.org/forbidden", false),
            if with_schema {
                "ex:Restriction logic:onProperty ex:p ; logic:maxCardinality \"invalid\" . ex:x logic:instanceOf ex:Restriction ."
            } else {
                ""
            },
        ));
        let (result, _, _) = crate::reason::reason_program_budgeted(
            &program(vec![]),
            crate::reason::prepare_reasoning_input(&store).unwrap(),
            &crate::physical::SelectedDomains::new([]).unwrap(),
            Some(0),
        )
        .unwrap();
        assert_eq!(
            status(&result),
            if with_schema {
                ObligationDisposition::Unknown
            } else {
                ObligationDisposition::SyntacticallyDischarged
            },
            "with_schema={with_schema}"
        );
    }
}

#[test]
fn a_committed_schema_operator_cannot_bypass_admission_at_a_budget_cut() {
    let store = source(&format!(
        "{} ex:s ex:seed ex:o .",
        declaration("https://example.org/forbidden", false)
    ));
    let selected = "https://blackcatinformatics.ca/logic/maxCardinality";
    let head = Formula::atom(
        Term::iri(selected).unwrap(),
        vec![
            Term::iri("https://example.org/Restriction").unwrap(),
            Term::literal("invalid", None).unwrap(),
        ],
    )
    .unwrap();
    let (result, _, _) = crate::reason::reason_program_budgeted(
        &program(vec![rule(head)]),
        crate::reason::prepare_reasoning_input(&store).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        Some(1),
    )
    .unwrap();
    assert!(
        result
            .inferred()
            .iter()
            .any(|row| !row.is_edb && row.predicate == selected)
    );
    assert_eq!(status(&result), ObligationDisposition::Unknown);
}

#[test]
fn independent_document_blank_obligations_survive_native_ingress() {
    let body = "_:obligation logic:instanceOf logic:NonEntailmentObligation ; \
        logic:obligationForbiddenPredicate ex:forbidden ; \
        logic:obligationDischargeCondition logic:DischargeSyntacticReachability .";
    let first = source(body);
    let second = source(body);
    let mut builder = purrdf::RdfDatasetBuilder::new();
    builder.push_dataset(&first).unwrap();
    builder.push_dataset(&second).unwrap();
    let store = builder.freeze().unwrap();
    let result = run(&program(vec![]), &store);
    let assessment = assess_non_entailment_obligations(&result).unwrap();
    assert_eq!(assessment.outcomes.len(), 2, "{assessment:?}");
    assert_ne!(
        assessment.outcomes[0].obligation,
        assessment.outcomes[1].obligation
    );
    assert!(
        assessment
            .outcomes
            .iter()
            .all(|outcome| outcome.disposition == ObligationDisposition::SyntacticallyDischarged)
    );
}

/// Execute a tiny authored program; no repository corpus or fixture is produced.
fn mixed_typing_worlds() -> (Arc<RdfDataset>, crate::result::ReasoningResult) {
    let declaration = |name: &str, typing: &str| {
        format!(
            "ex:{name} {typing} ; logic:obligationForbiddenPredicate ex:forbidden ; \
             logic:obligationDischargeCondition logic:DischargeSyntacticReachability ."
        )
    };
    let text = format!(
        "@prefix logic: <{LOGIC_NS}> . @prefix ex: <https://example.org/> .
         ex:first {{ {} {} {} ex:seedRecord ex:seed ex:value .
             ex:derived logic:obligationForbiddenPredicate ex:forbidden ;
                 logic:obligationDischargeCondition logic:DischargeSyntacticReachability . }}
         ex:second {{ {} }}
         ex:empty {{ ex:record ex:unrelated ex:value . }}",
        declaration("rdf", "a logic:NonEntailmentObligation"),
        declaration("native", "logic:instanceOf logic:NonEntailmentObligation"),
        declaration(
            "both",
            "a logic:NonEntailmentObligation ; logic:instanceOf logic:NonEntailmentObligation"
        ),
        declaration("rdf", "logic:instanceOf logic:NonEntailmentObligation"),
    );
    let store = purrdf::parse_dataset(text.as_bytes(), "application/trig", None).unwrap();
    let result = run(
        &program(vec![rule(atom(
            "https://example.org/derived",
            &format!("{LOGIC_NS}instanceOf"),
            &format!("{LOGIC_NS}NonEntailmentObligation"),
        ))]),
        &store,
    );
    (store, result)
}

/// Inventory describes selected native worlds, including empty ones, rather
/// than the verifier's flattened presentation or the number of type triples.
#[test]
fn mixed_typing_inventory_retains_exact_world_counts_and_identities() {
    let (_, result) = mixed_typing_worlds();
    let assessment = assess_non_entailment_obligations(&result).unwrap();
    let identities: BTreeSet<_> = assessment
        .outcomes
        .iter()
        .map(|outcome| (outcome.world.as_str(), outcome.obligation.as_str()))
        .collect();
    assert_eq!(assessment.outcomes.len(), 5);
    assert_eq!(
        identities,
        BTreeSet::from([
            ("https://example.org/first", "https://example.org/rdf"),
            ("https://example.org/first", "https://example.org/native"),
            ("https://example.org/first", "https://example.org/both"),
            ("https://example.org/first", "https://example.org/derived"),
            ("https://example.org/second", "https://example.org/rdf"),
        ])
    );
    assert!(
        assessment.outcomes.iter().all(|outcome| {
            outcome.disposition == ObligationDisposition::SyntacticallyDischarged
        })
    );
    let findings = assessment.findings().collect::<Vec<_>>();
    for (world, authored, count) in [("first", 3, 4), ("second", 1, 1), ("empty", 0, 0)] {
        let world = format!("https://example.org/{world}");
        let inventories = findings
            .iter()
            .filter(|finding| {
                finding.code == "verify.non-entailment.inventory"
                    && finding.source_graph.as_ref().map(|graph| graph.as_str())
                        == Some(world.as_str())
            })
            .collect::<Vec<_>>();
        assert_eq!(inventories.len(), 1, "{world}: {findings:?}");
        let inventory = inventories[0];
        assert!(inventory.message.contains(&format!("{count} distinct")));
        assert!(inventory.message.contains(&format!(
            "authored={authored}, admitted={count}, discharged={count}"
        )));
        assert_eq!(inventory.cited_iris.len(), count);
        assert_eq!(inventory.severity, Severity::Note);
        assert_eq!(
            assessment
                .outcomes
                .iter()
                .filter(|outcome| outcome.world == world && outcome.authored)
                .count(),
            authored,
        );
        assert!(
            assessment
                .outcomes
                .iter()
                .filter(|outcome| outcome.world == world)
                .all(|outcome| {
                    outcome.authored == (outcome.obligation != "https://example.org/derived")
                })
        );
    }
}

/// The public verifier additionally consumes authenticated producer-selected laws.
/// Its normal corpus lane must retain every native inventory field verbatim.
#[test]
fn corpus_verifier_preserves_mixed_typing_obligation_inventories() {
    let (store, result) = mixed_typing_worlds();
    let assessment = assess_non_entailment_obligations(&result).unwrap();
    let report = crate::verify::verify_with_reasoning_result(&store, &result, &[]).unwrap();
    let inventories = assessment
        .findings()
        .filter(|finding| finding.code == "verify.non-entailment.inventory")
        .collect::<Vec<_>>();
    assert!(!inventories.is_empty());
    for inventory in inventories {
        assert!(report.findings.iter().any(|finding| {
            finding.code == inventory.code
                && finding.source_graph == inventory.source_graph
                && finding.message == inventory.message
                && finding.cited_iris == inventory.cited_iris
                && finding.detail == inventory.detail
        }));
    }
}
