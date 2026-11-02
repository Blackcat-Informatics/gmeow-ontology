// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use std::fmt::Write as _;

fn obligation(iri: &str, pred: &str, conds: &[&str]) -> Obligation {
    Obligation {
        iri: iri.to_owned(),
        forbidden_predicate: pred.to_owned(),
        discharge_conditions: conds.iter().map(|s| format!("{LOGIC_NS}{s}")).collect(),
    }
}

pub(super) fn governance_store(body: &str) -> Arc<RdfDataset> {
    let source = format!(
        "@prefix logic: <{LOGIC_NS}> .\n\
             @prefix ex: <https://example.org/> .\n\
             @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n{body}"
    );
    purrdf::parse_dataset(source.as_bytes(), "text/turtle", None).expect("governance fixture")
}

/// Supply actual native derivations for assessment API controls. This produces
/// only this test's tiny synthetic store, never a repository corpus or receipt.
fn reason_governance_store(
    store: &Arc<RdfDataset>,
    derived: &BTreeSet<String>,
) -> crate::result::ReasoningResult {
    use gmeow_logic_compile::ir::{Formula, LogicProgram, Term};
    use purrdf::{DatasetMut, MutableDataset, QuadValues};

    let mut source = MutableDataset::new(Arc::clone(store));
    source
        .insert(QuadValues {
            s: TermValue::iri("urn:governance:subject"),
            p: TermValue::iri("urn:governance:seed"),
            o: TermValue::iri("urn:governance:object"),
            g: None,
        })
        .unwrap();
    let source = source.freeze().unwrap();
    let atom = |predicate: &str| {
        Formula::atom(
            Term::iri(predicate).unwrap(),
            vec![Term::var("x").unwrap(), Term::var("y").unwrap()],
        )
        .unwrap()
    };
    let formulas = derived
        .iter()
        .map(|predicate| Formula::Forall {
            vars: vec!["x".into(), "y".into()],
            body: Box::new(Formula::Implies(
                Box::new(atom("urn:governance:seed")),
                Box::new(atom(predicate)),
            )),
        })
        .collect();
    let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(formulas);
    crate::reason::reason_program(
        &program,
        crate::reason::prepare_reasoning_input(&source).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
    )
    .unwrap()
}

fn check_store(
    store: &Arc<RdfDataset>,
    derived: &BTreeSet<String>,
) -> gmeow_errors::Result<Vec<Finding>> {
    Ok(
        assess_non_entailment_obligations(&reason_governance_store(store, derived))?
            .findings()
            .collect(),
    )
}

#[test]
fn malformed_governance_query_retains_the_original_native_cause() {
    let store = governance_store("");
    let error = select(&store, "SELECT ?broken WHERE {").unwrap_err();
    assert!(error.is::<purrdf::RdfDiagnostic>(), "{error:?}");
    let native_message = error
        .downcast_ref::<purrdf::RdfDiagnostic>()
        .unwrap()
        .to_string();
    let mut ledger = gmeow_errors::DiagLedger::new();
    ledger.attach(error, gmeow_errors::StageId::new("verify.obligations"));
    assert_eq!(ledger.verdict(), gmeow_errors::GateVerdict::Fatal);
    let report = ledger.project_report("verify");
    assert_eq!(
        report.findings[0].message,
        "governance query evaluation failed"
    );
    assert!(
        report.findings[0]
            .detail
            .as_ref()
            .is_some_and(|detail| detail.contains(&native_message))
    );
}

#[test]
fn authored_program_heads_are_unknown_until_finite_closure_is_selected() {
    use gmeow_logic_compile::ir::{Formula, LogicProgram, Term};

    let atom = |predicate| {
        Formula::atom(
            Term::iri(predicate).unwrap(),
            vec![Term::var("x").unwrap(), Term::var("y").unwrap()],
        )
        .unwrap()
    };
    let program =
        LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![Formula::Forall {
            vars: vec!["x".into(), "y".into()],
            body: Box::new(Formula::Implies(
                Box::new(atom("https://example.org/seed")),
                Box::new(atom("https://example.org/forbidden")),
            )),
        }]);
    for seed in [false, true] {
        for finite in [false, true] {
            let store = governance_store(&format!(
                "ex:obligation logic:instanceOf logic:NonEntailmentObligation ; \
                 logic:obligationForbiddenPredicate ex:forbidden ; \
                 logic:obligationDischargeCondition logic:DischargeSyntacticReachability{} . {}",
                if finite {
                    ", logic:DischargeFiniteClosure"
                } else {
                    ""
                },
                if seed {
                    "ex:subject ex:seed ex:object ."
                } else {
                    ""
                },
            ));
            let result = crate::reason::reason_program(
                &program,
                crate::reason::prepare_reasoning_input(&store).unwrap(),
                &crate::physical::SelectedDomains::new([]).unwrap(),
            )
            .unwrap();
            result.validate_native_closure().unwrap();
            assert!(result.is_conclusive(), "{result:?}");
            let derived = result
                .inferred()
                .iter()
                .filter(|row| !row.is_edb)
                .map(|row| row.predicate.clone())
                .collect::<BTreeSet<_>>();
            assert_eq!(derived.contains("https://example.org/forbidden"), seed);
            let assessment = assess_non_entailment_obligations(&result).unwrap();
            let findings: Vec<_> = assessment.findings().collect();
            let unknown = findings
                .iter()
                .any(|finding| finding.code == "verify.non-entailment.not-discharged");
            let violated = findings
                .iter()
                .any(|finding| finding.code == "verify.non-entailment.derived");
            assert_eq!(
                unknown,
                !finite && !seed,
                "seed={seed}, finite={finite}: {findings:?}"
            );
            assert_eq!(violated, seed);
        }
    }
}

#[test]
fn native_obligation_inventory_covers_both_typings_once_with_attribution() {
    let mut source = String::new();
    for (name, typing) in [
        ("rdf", "a logic:NonEntailmentObligation"),
        ("native", "logic:instanceOf logic:NonEntailmentObligation"),
        (
            "both",
            "a logic:NonEntailmentObligation ; logic:instanceOf logic:NonEntailmentObligation",
        ),
    ] {
        writeln!(
            source,
            "ex:{name} {typing} ; logic:obligationForbiddenPredicate ex:forbidden ; \
                 logic:obligationDischargeCondition logic:DischargeFiniteClosure .\n\
                 ex:candidate_{name} logic:instanceOf logic:FormalizationCandidate ; \
                 logic:candidateNonEntailment ex:{name} ."
        )
        .expect("write fixture");
    }
    let store = governance_store(&source);
    let obligations = parse_obligations(&store).expect("complete typed discovery");
    assert_eq!(obligations.len(), 3);
    let findings = check_store(
        &store,
        &BTreeSet::from(["https://example.org/forbidden".to_owned()]),
    )
    .expect("governance checks");
    let violations: Vec<_> = findings
        .iter()
        .filter(|finding| finding.code == "verify.non-entailment.derived")
        .collect();
    assert_eq!(violations.len(), 3);
    for name in ["rdf", "native", "both"] {
        assert!(violations.iter().any(|finding| {
            finding
                .tags
                .contains(&format!("candidate:https://example.org/candidate_{name}"))
        }));
    }
    let inventory = findings
        .iter()
        .find(|finding| finding.code == "verify.non-entailment.inventory")
        .expect("visible obligation inventory");
    assert_eq!(inventory.cited_iris.len(), 3);
    assert!(inventory.message.contains("3 distinct"));
}

#[test]
fn malformed_obligation_declarations_cannot_disappear_or_choose_a_first_value() {
    for (fields, expected) in [
        ("", "has no logic:obligationForbiddenPredicate"),
        (
            "; logic:obligationForbiddenPredicate ex:p, ex:q",
            "multiple distinct",
        ),
        (
            "; logic:obligationForbiddenPredicate []",
            "predicate IRI or xsd:anyURI",
        ),
        (
            "; logic:obligationForbiddenPredicate \"https://example.org/p\"",
            "predicate IRI or xsd:anyURI",
        ),
        (
            "; logic:obligationForbiddenPredicate \"relative\"^^xsd:anyURI",
            "absolute forbidden predicate",
        ),
        (
            "; logic:obligationForbiddenPredicate \"invalid iri\"^^xsd:anyURI",
            "invalid forbidden predicate",
        ),
        (
            "; logic:obligationForbiddenPredicate ex:p ; logic:obligationDischargeCondition \"DischargeFiniteClosure\"",
            "non-IRI discharge condition",
        ),
        (
            "; logic:obligationForbiddenPredicate ex:p ; logic:obligationDischargeCondition []",
            "non-IRI discharge condition",
        ),
    ] {
        let store = governance_store(&format!(
            "ex:obligation logic:instanceOf logic:NonEntailmentObligation {fields} ."
        ));
        let error = check_store(&store, &BTreeSet::new())
            .expect_err("malformed declaration must fail admission");
        assert!(error.to_string().contains(expected), "{fields}: {error}");
    }
}

#[test]
fn native_obligation_discovery_preserves_independent_document_blank_scopes() {
    let source = "[] logic:instanceOf logic:NonEntailmentObligation ; \
                      logic:obligationForbiddenPredicate ex:forbidden ; \
                      logic:obligationDischargeCondition logic:DischargeFiniteClosure .";
    let first = governance_store(source);
    let second = governance_store(source);
    let mut builder = purrdf::RdfDatasetBuilder::new();
    builder.push_dataset(&first).unwrap();
    builder.push_dataset(&second).unwrap();
    let merged = builder.freeze().expect("merged authored fixture");
    let obligations = parse_obligations(&merged).expect("distinct scoped obligations");
    assert_eq!(obligations.len(), 2);
    assert_ne!(obligations[0].iri, obligations[1].iri);
    assert!(
        obligations
            .iter()
            .all(|obligation| obligation.iri.starts_with("_:"))
    );
}

#[test]
fn canonical_candidate_typing_reaches_coverage_and_prose_drift_checks() {
    // The declared digest commits to "Previous source prose.", before the edit.
    let source = "ex:candidate logic:instanceOf logic:FormalizationCandidate ; \
                      logic:candidateFormalizes ex:term ; \
                      logic:candidateSourceField ex:field ; \
                      logic:candidateSourceHash \"3b99e0060c89e4312f9fe633fe9af09d691a68becc2a5f4c06df7569339d54e9\"^^<https://blackcatinformatics.ca/gmeow/sha256Digest> .\n\
                      ex:field logic:proseFieldProperty ex:definition .\n\
                      ex:term ex:definition \"Current source prose.\"@x-gmeow-english .";
    let store = governance_store(source);
    let coverage = formalization_coverage(&store).expect("candidate coverage");
    assert!(coverage.iter().any(
        |finding| finding.code == "verify.formalization.uncategorized"
            && finding.message.contains("https://example.org/candidate")
    ));
    let drift = check_candidate_source_hash_drift(&store).expect("candidate drift");
    assert_eq!(drift.len(), 1);
    assert_eq!(drift[0].code, "verify.candidate-hash.drift");
}

#[test]
fn empty_selected_obligation_inventory_is_visible() {
    let store = governance_store("ex:s ex:p ex:o .");
    let findings = check_store(&store, &BTreeSet::new()).expect("empty inventory is reported");
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Note);
    assert_eq!(findings[0].code, "verify.non-entailment.inventory");
    assert!(findings[0].message.contains("0 distinct"));
}

#[test]
fn standing_obligations_discharge_on_the_real_strata() {
    // The two standing obligations forbid assertion-only gmeow: predicates, which
    // are not foundation rule heads → no violation.
    let heads = foundation::head_predicate_iris();
    let counterpart = obligation(
        "ex:counterpart",
        "https://blackcatinformatics.ca/gmeow/counterpartOf",
        &["DischargeSyntacticReachability"],
    );
    let deception = obligation(
        "ex:deception",
        "https://blackcatinformatics.ca/gmeow/deceptiveIntentClaim",
        &["DischargeSyntacticReachability", "DischargeFiniteClosure"],
    );
    let no_candidates = BTreeMap::new();
    assert!(
        check_reachability(
            &purrdf::iri::BaseIri::parse("urn:test:world").unwrap(),
            &counterpart,
            heads.contains(&counterpart.forbidden_predicate),
            &no_candidates
        )
        .is_none()
    );
    assert!(
        check_reachability(
            &purrdf::iri::BaseIri::parse("urn:test:world").unwrap(),
            &deception,
            heads.contains(&deception.forbidden_predicate),
            &no_candidates
        )
        .is_none()
    );
    assert!(
        check_discharge_conditions(
            &purrdf::iri::BaseIri::parse("urn:test:world").unwrap(),
            &counterpart
        )
        .is_empty()
    );
    assert!(
        check_discharge_conditions(
            &purrdf::iri::BaseIri::parse("urn:test:world").unwrap(),
            &deception
        )
        .is_empty()
    );
}

#[test]
fn reachable_head_remains_unknown_without_a_counterexample() {
    // A possible head does not prove its premises hold. The required syntactic
    // discharge remains unresolved, and the finding must never fabricate a violation.
    let mut heads = foundation::head_predicate_iris();
    heads.insert("https://blackcatinformatics.ca/gmeow/counterpartOf".to_owned());
    let counterpart = obligation(
        "ex:counterpart",
        "https://blackcatinformatics.ca/gmeow/counterpartOf",
        &["DischargeSyntacticReachability"],
    );
    let no_candidates = BTreeMap::new();
    let finding = check_reachability(
        &purrdf::iri::BaseIri::parse("urn:test:world").unwrap(),
        &counterpart,
        true,
        &no_candidates,
    )
    .expect("must fire");
    assert_eq!(finding.grade().severity, Severity::Error);
    assert_eq!(
        gmeow_errors::code::code_str(finding.code()),
        "verify.non-entailment.not-discharged"
    );
    assert!(finding.message().contains("logic:ObligationUnknown"));
    assert!(!finding.message().contains("logic:ObligationViolated"));
    let finite_only = obligation(
        "ex:finite",
        &counterpart.forbidden_predicate,
        &["DischargeFiniteClosure"],
    );
    assert!(
        check_reachability(
            &purrdf::iri::BaseIri::parse("urn:test:world").unwrap(),
            &finite_only,
            true,
            &no_candidates
        )
        .is_none()
    );
}

/// A completed finite closure is a sufficient discharge condition even when
/// syntactic analysis cannot rule out a firing. Only an actual derivation violates it.
#[test]
fn finite_closure_can_discharge_a_reachable_head() {
    let predicate = foundation::head_predicate_iris()
        .into_iter()
        .next()
        .expect("foundation head");
    let store = governance_store(&format!(
        "ex:obligation logic:instanceOf logic:NonEntailmentObligation ;
             logic:obligationForbiddenPredicate <{predicate}> ;
             logic:obligationDischargeCondition logic:DischargeSyntacticReachability,
                 logic:DischargeFiniteClosure ."
    ));
    let findings = check_store(&store, &BTreeSet::new()).expect("complete empty closure");
    assert!(
        findings
            .iter()
            .all(|finding| finding.severity != Severity::Error),
        "{findings:?}"
    );
    let findings = check_store(&store, &BTreeSet::from([predicate])).expect("counterexample");
    assert!(
        findings
            .iter()
            .any(|finding| finding.code == "verify.non-entailment.derived")
    );
    assert!(
        findings
            .iter()
            .all(|finding| finding.code != "verify.non-entailment.not-discharged")
    );
}

#[test]
fn arm_b_finite_closure_green_then_red() {
    let pred = "https://blackcatinformatics.ca/gmeow/deceptiveIntentClaim";
    let deception = obligation(
        "ex:deception",
        pred,
        &["DischargeSyntacticReachability", "DischargeFiniteClosure"],
    );
    // Green: the forbidden predicate is NOT among the derived edges (the foundation
    // never derives it; an asserted, attributed intent claim is EDB, not derived).
    let empty = BTreeSet::new();
    let no_candidates = BTreeMap::new();
    assert!(
        check_finite_closure(
            &purrdf::iri::BaseIri::parse("urn:test:world").unwrap(),
            &deception,
            &empty,
            &no_candidates
        )
        .is_none()
    );
    // Red: a (synthetic) derivation of the forbidden predicate trips the obligation.
    let mut derived = BTreeSet::new();
    derived.insert(pred.to_owned());
    let finding = check_finite_closure(
        &purrdf::iri::BaseIri::parse("urn:test:world").unwrap(),
        &deception,
        &derived,
        &no_candidates,
    )
    .expect("must fire");
    assert_eq!(finding.grade().severity, Severity::Error);
    assert!(gmeow_errors::code::code_str(finding.code()).contains("non-entailment.derived"));
    // Focus bytes are identity coordinates, independent of emitted source positions.
    // Literal focus and fingerprint values captured from the original diagnostic writer.
    for (predicate, expected_focus, expected_fingerprint) in [
        (
            pred,
            r#"["https://blackcatinformatics.ca/gmeow/deceptiveIntentClaim","derived"]"#,
            "ee74df1811034941cd03ce775d9b83d4",
        ),
        (
            "https://example.org/猫/é/ادعاء",
            r#"["https://example.org/猫/é/ادعاء","derived"]"#,
            "1c0ab382d6f1d5575b8dd45aece93a26",
        ),
    ] {
        let obligation = obligation("ex:deception", predicate, &["DischargeFiniteClosure"]);
        let diagnostic = check_finite_closure(
            &purrdf::iri::BaseIri::parse("urn:test:world").unwrap(),
            &obligation,
            &BTreeSet::from([predicate.to_owned()]),
            &no_candidates,
        )
        .expect("committed counterexample");
        let focus = &diagnostic.inner().source_ctx.focus.as_ref().unwrap().0;
        let fingerprint = gmeow_errors::DiagFingerprint::compute(
            gmeow_errors::code::code_str(diagnostic.code()),
            diagnostic.grade().category,
            &diagnostic.inner().source_ctx,
        );
        assert_eq!(focus, expected_focus);
        assert_eq!(fingerprint.hex(), expected_fingerprint);
    }
}

#[test]
fn counterexample_survives_without_finite_closure_condition() {
    // A selected negative-proof arm cannot suppress a real positive counterexample.
    let pred = "https://blackcatinformatics.ca/gmeow/counterpartOf";
    let counterpart = obligation("ex:counterpart", pred, &["DischargeSyntacticReachability"]);
    let mut derived = BTreeSet::new();
    derived.insert(pred.to_owned());
    let no_candidates = BTreeMap::new();
    assert!(
        check_finite_closure(
            &purrdf::iri::BaseIri::parse("urn:test:world").unwrap(),
            &counterpart,
            &derived,
            &no_candidates
        )
        .is_some()
    );
}

#[test]
fn candidate_over_typing_a_non_assertion_is_surfaced_and_attributed_to_the_candidate() {
    // The over-typing review is realized by the shipped non-entailment machinery, not
    // a separate flag: a FormalizationCandidate categorized
    // logic:CategoryNonEntailmentObligation records a deliberate non-assertion, and its
    // axiom is FORBIDDEN from letting the engine derive that predicate. If a
    // formalization over-types — entailing the deliberately withheld conclusion — the
    // executable check SURFACES it as logic:ObligationViolated (a hard error), never
    // silently asserting it; when the predicate is genuinely absent the obligation is
    // DISCHARGED (actively checked, never silently skipped).
    //
    // This test also proves `logic:candidateNonEntailment` is load-bearing, not
    // decorative: the check traverses candidate->obligation via that edge
    // (`candidates_by_obligation`) and APPENDS the declaring candidate's IRI to the
    // violation finding's message/tags. That traversal is what makes
    // LOGIC-FOUNDATION.md's claim — the over-typing review is "realized through the
    // typed candidate lifecycle" — literally true in this code path: removing the
    // edge from the store must make the candidate-attribution assertion below fail,
    // even though the obligation itself still fires (structural presence of the edge
    // is separately hard-enforced by
    // queries/verify/non-entailment-carrier-required.rq, not re-checked here).
    let logic = "https://blackcatinformatics.ca/logic/";
    let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    let any_uri = "http://www.w3.org/2001/XMLSchema#anyURI";
    let forbidden = "https://blackcatinformatics.ca/gmeow/overTypedClaim";
    let candidate_iri = "https://ex/cand";
    // A store carrying the obligation AND the candidate that declares it — the exact
    // shape of a candidate whose formalization touches a deliberate non-assertion.
    let ntriples = format!(
        "<https://ex/obl> <{rdf_type}> <{logic}NonEntailmentObligation> .\n\
             <https://ex/obl> <{logic}obligationForbiddenPredicate> \"{forbidden}\"^^<{any_uri}> .\n\
             <https://ex/obl> <{logic}obligationDischargeCondition> <{logic}DischargeFiniteClosure> .\n\
             <{candidate_iri}> <{rdf_type}> <{logic}FormalizationCandidate> .\n\
             <{candidate_iri}> <{logic}candidateCategory> <{logic}CategoryNonEntailmentObligation> .\n\
             <{candidate_iri}> <{logic}candidateNonEntailment> <https://ex/obl> .\n"
    );
    let store = store_from_ntriples(&ntriples);

    // Green: the forbidden predicate is not derived → the obligation is DISCHARGED. The
    // check runs and surfaces no error: checked-and-passed, not silently skipped.
    let discharged = check_store(&store, &BTreeSet::new()).expect("check runs");
    assert!(
        !discharged.iter().any(|f| f.severity == Severity::Error),
        "a discharged non-entailment obligation must produce no error finding: {discharged:?}"
    );

    // Red: the formalization over-types — the forbidden predicate appears as a DERIVED
    // edge. The check must SURFACE it as ObligationViolated, never silently assert it,
    // AND name the declaring candidate — this assertion fails if
    // `candidateNonEntailment` is removed from the store or not traversed.
    let mut derived = BTreeSet::new();
    derived.insert(forbidden.to_owned());
    let surfaced = check_store(&store, &derived).expect("check runs");
    let violation = surfaced
        .iter()
        .find(|f| f.code == "verify.non-entailment.derived" && f.severity == Severity::Error)
        .expect(
            "a candidate over-typing a deliberate non-assertion must be surfaced as \
                 logic:ObligationViolated",
        );
    assert!(
        violation.message.contains(candidate_iri)
            || violation
                .tags
                .iter()
                .any(|t| t == &format!("candidate:{candidate_iri}")),
        "the violation must name the declaring candidate <{candidate_iri}> in its message \
             or tags, proving candidateNonEntailment is load-bearing: {violation:?}"
    );

    // Companion: an obligation with NO declaring candidate must NOT get a candidate
    // suffix — the attribution is conditional on the edge, not always-on.
    let orphan_ntriples = format!(
        "<https://ex/orphan-obl> <{rdf_type}> <{logic}NonEntailmentObligation> .\n\
             <https://ex/orphan-obl> <{logic}obligationForbiddenPredicate> \"{forbidden}\"^^<{any_uri}> .\n\
             <https://ex/orphan-obl> <{logic}obligationDischargeCondition> <{logic}DischargeFiniteClosure> .\n"
    );
    let orphan_store = store_from_ntriples(&orphan_ntriples);
    let orphan_surfaced = check_store(&orphan_store, &derived).expect("check runs");
    let orphan_violation = orphan_surfaced
        .iter()
        .find(|f| f.code == "verify.non-entailment.derived" && f.severity == Severity::Error)
        .expect("an undischarged obligation with no declaring candidate must still fire");
    assert!(
        !orphan_violation
            .message
            .contains("declared by formalization candidate")
            && !orphan_violation
                .tags
                .iter()
                .any(|t| t.starts_with("candidate:")),
        "an obligation with no declaring candidate must not carry candidate attribution: \
             {orphan_violation:?}"
    );
}

#[test]
fn unwired_discharge_condition_is_a_hard_error() {
    // Declaring a discharge condition the engine does not wire is an error, never a
    // silent unknown.
    let obl = obligation(
        "ex:bounded",
        "https://blackcatinformatics.ca/gmeow/somePredicate",
        &["DischargeBoundedCorpus"],
    );
    let findings = check_discharge_conditions(
        &purrdf::iri::BaseIri::parse("urn:test:world").unwrap(),
        &obl,
    );
    assert_eq!(findings.len(), 1);
    assert!(gmeow_errors::code::code_str(findings[0].code()).contains("unwired-discharge"));
    assert_eq!(findings[0].grade().severity, Severity::Error);
}

#[test]
fn missing_discharge_condition_is_a_hard_error() {
    let obl = obligation(
        "ex:bare",
        "https://blackcatinformatics.ca/gmeow/somePredicate",
        &[],
    );
    let findings = check_discharge_conditions(
        &purrdf::iri::BaseIri::parse("urn:test:world").unwrap(),
        &obl,
    );
    assert_eq!(findings.len(), 1);
    assert!(gmeow_errors::code::code_str(findings[0].code()).contains("no-discharge"));
}

/// Build a minimal in-memory dataset from N-Triples for query tests.
fn store_from_ntriples(ntriples: &str) -> Arc<RdfDataset> {
    purrdf::parse_dataset(ntriples.as_bytes(), "application/n-triples", None)
        .expect("load N-Triples")
}

#[test]
fn duplicate_category_on_candidate_is_a_hard_error() {
    // A candidate that carries two DISTINCT candidateCategory values is malformed;
    // candidateCategory is single-valued by spec. The function must emit exactly one
    // error Finding with the multi-category code and the report must not be ok().
    let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    let logic = "https://blackcatinformatics.ca/logic/";
    let ntriples = format!(
        "<https://ex/cand1> <{logic}candidateCategory> <{logic}CategoryDerivationRule> .\n\
             <https://ex/cand1> <{logic}candidateCategory> <{logic}CategoryIntegrityConstraint> .\n\
             <https://ex/cand1> <{rdf_type}> <{logic}FormalizationCandidate> .\n",
    );
    let store = store_from_ntriples(&ntriples);
    let findings = formalization_coverage(&store).expect("query must not error");
    let errors: Vec<_> = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .collect();
    assert_eq!(
        errors.len(),
        1,
        "expected exactly one error finding; got {errors:?}"
    );
    assert!(
        errors[0].code.contains("multi-category"),
        "error code must contain 'multi-category'; got {:?}",
        errors[0].code
    );
    assert!(
        errors[0].message.contains("https://ex/cand1"),
        "error message must name the offending candidate; got {:?}",
        errors[0].message
    );
    // The presence of any error-severity Finding means the run is not ok.
    let has_error = findings.iter().any(|f| f.severity == Severity::Error);
    assert!(
        has_error,
        "findings must contain at least one error for malformed multi-category candidate"
    );
}

/// Build a store carrying one harvested candidate: it formalizes `term` via a source
/// field whose `logic:proseFieldProperty` is `prop`, records `declared_hash`, and the
/// term carries `prose` on `prop` in the given language. Mirrors the shape of a real
/// foundational-partition candidate's harvest back-link.
fn harvested_candidate_store(
    term: &str,
    prop: &str,
    prose: &str,
    lang: &str,
    declared_hash: &str,
) -> Arc<RdfDataset> {
    let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    let logic = "https://blackcatinformatics.ca/logic/";
    let field = "https://blackcatinformatics.ca/logic/ProseFieldDefinition";
    let ntriples = format!(
        "<https://ex/cand> <{rdf_type}> <{logic}FormalizationCandidate> .\n\
             <https://ex/cand> <{logic}candidateFormalizes> <{term}> .\n\
             <https://ex/cand> <{logic}candidateSourceField> <{field}> .\n\
             <https://ex/cand> <{logic}candidateSourceHash> \"{declared_hash}\"^^<https://blackcatinformatics.ca/gmeow/sha256Digest> .\n\
             <{field}> <{logic}proseFieldProperty> <{prop}> .\n\
             <{term}> <{prop}> \"{prose}\"@{lang} .\n"
    );
    store_from_ntriples(&ntriples)
}

#[test]
fn source_hash_matches_prose_no_drift() {
    // A candidate whose recorded hash IS the sha256 of the current source-language
    // prose produces no finding — the teeth stay silent when nothing drifted.
    let term = "https://blackcatinformatics.ca/logic/Endurant";
    let prop = "http://www.w3.org/2004/02/skos/core#definition";
    let prose = "A continuant wholly present at each moment of its existence.";
    let hash = candidate_source_hash(prose);
    let store = harvested_candidate_store(term, prop, prose, SOURCE_LANG, &hash);
    let findings = check_candidate_source_hash_drift(&store).expect("check runs");
    assert!(
        findings.is_empty(),
        "a matching source hash must produce no drift finding: {findings:?}"
    );
}

#[test]
fn edited_prose_surfaces_as_drift() {
    // The recorded hash anchors the OLD prose; the term now carries EDITED prose, so
    // the recompute no longer matches — the drift check must fire a hard error, giving
    // the "a later prose edit surfaces as drift" governance claim real teeth.
    let term = "https://blackcatinformatics.ca/logic/Endurant";
    let prop = "http://www.w3.org/2004/02/skos/core#definition";
    let stale_hash = candidate_source_hash("The ORIGINAL, reviewed definition prose.");
    let edited_prose = "The definition prose after an un-reviewed edit.";
    let store = harvested_candidate_store(term, prop, edited_prose, SOURCE_LANG, &stale_hash);
    let findings = check_candidate_source_hash_drift(&store).expect("check runs");
    let drift = findings
        .iter()
        .find(|f| f.code == "verify.candidate-hash.drift")
        .expect("edited prose must surface as a drift finding");
    assert_eq!(drift.severity, Severity::Error);
    assert!(
        drift.message.contains(term),
        "the drift finding must name the formalized term: {drift:?}"
    );
}

#[test]
fn projected_translation_is_not_hashed() {
    // Only the @x-gmeow-english source literal is the hashed text. A projected public
    // translation (@en) on the same field must be ignored: the term carries ONLY an
    // @en literal here, so the harvest resolves no source-language prose and the check
    // reports a dangling link rather than silently hashing the translation.
    let term = "https://blackcatinformatics.ca/logic/Endurant";
    let prop = "http://www.w3.org/2004/02/skos/core#definition";
    let prose = "An English projection that must never be hashed.";
    let hash = candidate_source_hash(prose);
    let store = harvested_candidate_store(term, prop, prose, "en", &hash);
    let findings = check_candidate_source_hash_drift(&store).expect("check runs");
    assert!(
        findings
            .iter()
            .any(|f| f.code == "verify.candidate-hash.no-source-prose"),
        "a term with only a projected @en literal must report no source-language prose, \
             never silently hash the translation: {findings:?}"
    );
}

#[test]
fn candidate_without_harvest_link_is_skipped() {
    // The Event⊥Situation shape: a candidate carrying neither back-link leg has no
    // single source triple to recompute against and must be silently skipped, never a
    // false drift error.
    let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    let logic = "https://blackcatinformatics.ca/logic/";
    let ntriples = format!(
        "<https://ex/cand> <{rdf_type}> <{logic}FormalizationCandidate> .\n\
             <https://ex/cand> <{logic}candidateSourceHash> \"sha256:deadbeef\" .\n"
    );
    let store = store_from_ntriples(&ntriples);
    let findings = check_candidate_source_hash_drift(&store).expect("check runs");
    assert!(
        findings.is_empty(),
        "a candidate with no harvest back-link must be skipped, not flagged: {findings:?}"
    );
}

/// An advisory `logic:Constraint` whose `logic:message` equals the current
/// `gmeow:avoidWhen` prose of its `logic:formalizes` term (for the field named by
/// `logic:adviceSourceField`) is silent; a message that diverges from that prose is a
/// hard binding error naming the term. This is the soft-advice peer of the
/// candidateSourceHash drift gate — a direct string binding, so the surfaced advice can
/// never silently drift from the prose it formalizes.
#[test]
fn advisory_constraint_message_binds_avoidwhen_prose() {
    let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    let logic = "https://blackcatinformatics.ca/logic/";
    let avoid_when = "https://blackcatinformatics.ca/gmeow/avoidWhen";
    let field = "https://blackcatinformatics.ca/logic/ProseFieldAvoidWhen";
    let term = "https://blackcatinformatics.ca/gmeow/Entity";
    let prose =
        "Avoid typing an instance as a bare gmeow:Entity when a more specific sortal applies.";
    let constraint_store = |message: &str| {
        let ntriples = format!(
            "<https://ex/adviceConstraint> <{rdf_type}> <{logic}Constraint> .\n\
                 <https://ex/adviceConstraint> <{logic}formalizes> <{term}> .\n\
                 <https://ex/adviceConstraint> <{logic}adviceSourceField> <{field}> .\n\
                 <https://ex/adviceConstraint> <{logic}message> \"{message}\" .\n\
                 <{field}> <{logic}proseFieldProperty> <{avoid_when}> .\n\
                 <{term}> <{avoid_when}> \"{prose}\"@{SOURCE_LANG} .\n"
        );
        store_from_ntriples(&ntriples)
    };

    // Message == current avoidWhen prose → silent.
    let findings =
        check_advice_message_prose_binding(&constraint_store(prose)).expect("check runs");
    assert!(
        findings.is_empty(),
        "an advisory constraint whose message equals its term's avoidWhen prose must not drift: {findings:?}"
    );

    // Message diverges from the prose → hard binding error naming the term.
    let drift =
        check_advice_message_prose_binding(&constraint_store("A stale, paraphrased message."))
            .expect("check runs")
            .into_iter()
            .find(|f| f.code == "verify.advice-message.drift")
            .expect("a diverged advisory message must surface as a binding error");
    assert_eq!(drift.severity, Severity::Error);
    assert!(
        drift.message.contains(term),
        "the binding finding must name the formalized term: {drift:?}"
    );
}

/// The same binding gate covers a first-class `logic:AdviceGuidance` (useWhen) carrier —
/// selected by its `logic:adviceSourceField` back-link, not a type filter — so a
/// useWhen carrier whose message diverges from the term's current useWhen prose is a hard
/// error, exactly as for an avoidWhen constraint.
#[test]
fn advice_guidance_message_binds_usewhen_prose() {
    let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    let logic = "https://blackcatinformatics.ca/logic/";
    let use_when = "https://blackcatinformatics.ca/gmeow/useWhen";
    let field = "https://blackcatinformatics.ca/logic/ProseFieldUseWhen";
    let term = "https://blackcatinformatics.ca/gmeow/Entity";
    let prose = "Use as the universal domain for anything that persists and bears properties.";
    let guidance_store = |message: &str| {
        let ntriples = format!(
            "<https://ex/adviceGuidance> <{rdf_type}> <{logic}AdviceGuidance> .\n\
                 <https://ex/adviceGuidance> <{logic}formalizes> <{term}> .\n\
                 <https://ex/adviceGuidance> <{logic}adviceSourceField> <{field}> .\n\
                 <https://ex/adviceGuidance> <{logic}message> \"{message}\" .\n\
                 <{field}> <{logic}proseFieldProperty> <{use_when}> .\n\
                 <{term}> <{use_when}> \"{prose}\"@{SOURCE_LANG} .\n"
        );
        store_from_ntriples(&ntriples)
    };

    // Message == current useWhen prose → silent.
    let findings = check_advice_message_prose_binding(&guidance_store(prose)).expect("check runs");
    assert!(
        findings.is_empty(),
        "an AdviceGuidance whose message equals its term's useWhen prose must not drift: {findings:?}"
    );

    // Diverged message → hard binding error naming the term.
    let drift = check_advice_message_prose_binding(&guidance_store("A stale useWhen paraphrase."))
        .expect("check runs")
        .into_iter()
        .find(|f| f.code == "verify.advice-message.drift")
        .expect("a diverged AdviceGuidance message must surface as a binding error");
    assert_eq!(drift.severity, Severity::Error);
    assert!(drift.message.contains(term));
}

#[test]
fn single_category_candidate_produces_correct_coverage() {
    // A well-formed candidate with a single valid category must not produce any
    // error finding; the coverage note must count it under the right category.
    let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    let logic = "https://blackcatinformatics.ca/logic/";
    let ntriples = format!(
        "<https://ex/cand2> <{rdf_type}> <{logic}FormalizationCandidate> .\n\
             <https://ex/cand2> <{logic}candidateCategory> <{logic}CategoryDerivationRule> .\n\
             <https://ex/cand2> <{logic}candidateLifecycle> <{logic}CandidateAccepted> .\n",
    );
    let store = store_from_ntriples(&ntriples);
    let findings = formalization_coverage(&store).expect("query must not error");
    let errors: Vec<_> = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "no errors expected for well-formed candidate; got {errors:?}"
    );
    let note = findings
        .iter()
        .find(|f| f.code == "verify.formalization.coverage")
        .expect("coverage note must be present");
    let detail = note.detail.as_deref().unwrap_or("");
    assert!(
        detail.contains("CategoryDerivationRule: total=1"),
        "coverage detail must count the single candidate under CategoryDerivationRule; got {detail:?}",
    );
}
