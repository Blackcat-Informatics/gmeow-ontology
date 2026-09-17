// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Tiny source-owned interactions of the single native producer graph.
use super::*;
use crate::physical::{DomainProfile, LogicalGraph, SelectedDomains, SelectedLogicalWorld};
use crate::reason::refute::native::{
    NativeClosureStatus, NativeProofOrigin, NativeRefutationFamily, NativeSourceTerms,
};
use gmeow_logic_compile::ir::{AtomicTerm, ContextualScope, LogicAxiom, LogicProgram, LogicRule};
use purrdf::{RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm};

const TYPE: &str = "https://blackcatinformatics.ca/logic/instanceOf";
const NOTHING: &str = "https://blackcatinformatics.ca/logic/Nothing";
const SAME: &str = "https://blackcatinformatics.ca/logic/sameAs";
const DIFFERENT: &str = "https://blackcatinformatics.ca/logic/differentFrom";
const C: &str = "urn:joint:C";
const X: &str = "urn:joint:x";
const Y: &str = "urn:joint:y";
const Z: &str = "urn:joint:z";

fn quad(s: &str, p: &str, o: &str) -> RdfQuad {
    RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o))
}
fn source(rows: &[RdfQuad]) -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for row in rows {
        builder.push_owned_quad(row);
    }
    builder.freeze().unwrap()
}
fn atom(s: &str, p: &str, o: &str, negative: bool) -> LogicAxiom {
    LogicAxiom::new(
        s,
        p,
        AtomicTerm::Iri(o.to_owned()),
        negative,
        ContextualScope::default(),
    )
    .unwrap()
}
fn absence() -> LogicRule {
    LogicRule::new(
        atom("?x", "urn:joint:absence", C, false),
        vec![atom("?x", TYPE, C, false), atom("?x", TYPE, NOTHING, true)],
        vec![],
        ContextualScope::default(),
    )
}
fn run(rows: &[RdfQuad], rules: Vec<LogicRule>) -> ProgramClosure {
    let program = LogicProgram::new(vec![], rules, vec![], None);
    execute(
        &crate::program_analysis::prepare_program(&program).unwrap(),
        prepare_reasoning_input(&source(rows)).unwrap(),
        &SelectedDomains::new([]).unwrap(),
        None,
    )
    .unwrap()
}

#[test]
fn source_empty_late_equality_clash_precedes_naf_and_leaves_disconnected_individual_local() {
    let rows = [
        quad(X, "urn:joint:late-equality", Y),
        quad(X, DIFFERENT, Y),
        quad(X, TYPE, C),
        quad(Z, TYPE, C),
    ];
    let input = prepare_reasoning_input(&source(&rows)).unwrap();
    assert!(input.sources.prepare().unwrap().rules.is_empty());
    let equality = LogicRule::new(
        atom("?x", SAME, Y, false),
        vec![atom("?x", "urn:joint:late-equality", Y, false)],
        vec![],
        ContextualScope::default(),
    );
    let result = run(&rows, vec![equality, absence()]);
    let local: std::collections::BTreeSet<_> = result
        .inferred
        .iter()
        .filter(|row| !row.is_edb && row.predicate == TYPE && row.object == TermValue::iri(NOTHING))
        .map(|row| row.subject.as_str())
        .collect();
    assert!(local.contains(X) && local.contains(Y));
    assert!(!local.contains(Z));
    let absent: Vec<_> = result
        .inferred
        .iter()
        .filter(|row| !row.is_edb && row.predicate == "urn:joint:absence")
        .map(|row| row.subject.as_str())
        .collect();
    assert_eq!(absent, vec![Z]);
    assert_eq!(result.native_status, NativeClosureStatus::Completed);
    let ledger = result
        .native_families
        .iter()
        .find(|ledger| ledger.has_conflict())
        .unwrap();
    assert!(
        ledger
            .proofs
            .iter()
            .any(|proof| proof.statement.predicate == SAME
                && matches!(proof.origin, NativeProofOrigin::Derived { .. }))
    );
    assert!(
        ledger
            .outcomes
            .iter()
            .filter(|outcome| outcome.family == NativeRefutationFamily::Identity)
            .flat_map(|outcome| &outcome.conclusions)
            .all(|clash| clash.committed.is_some())
    );
    ledger.validate().unwrap();
}

#[test]
fn malformed_selected_npa_does_not_publish_clash_or_release_its_absence_reader() {
    let owner = "urn:joint:npa";
    let rows = [
        quad(owner, "http://www.w3.org/2002/07/owl#sourceIndividual", X),
        quad(
            owner,
            "http://www.w3.org/2002/07/owl#assertionProperty",
            "urn:joint:p",
        ),
        quad(owner, "http://www.w3.org/2002/07/owl#targetIndividual", Y),
        quad(owner, "http://www.w3.org/2002/07/owl#targetIndividual", Z),
        quad(X, "urn:joint:p", Y),
        quad(X, TYPE, C),
    ];
    let result = run(&rows, vec![absence()]);
    assert!(matches!(
        result.native_status,
        NativeClosureStatus::Blocked { .. }
    ));
    assert!(!result.inferred.iter().any(|row| !row.is_edb
        && (row.object == TermValue::iri(NOTHING) || row.predicate == "urn:joint:absence")));
    let owner = result
        .source_coverage
        .worlds
        .values()
        .flat_map(|world| &world.admissions)
        .find(|admission| admission.owner == TermValue::iri(owner))
        .unwrap();
    assert_eq!(owner.selectors.len(), 4);
    assert!(!owner.obstructions.is_empty());
    assert!(!owner.support.is_empty());
}

#[test]
fn missing_npa_target_cannot_be_pruned_out_of_the_completion_proof() {
    let rows = [
        quad(
            "urn:joint:npa",
            "http://www.w3.org/2002/07/owl#sourceIndividual",
            X,
        ),
        quad(
            "urn:joint:npa",
            "http://www.w3.org/2002/07/owl#assertionProperty",
            "urn:joint:p",
        ),
        quad(X, TYPE, C),
    ];
    let result = run(&rows, vec![absence()]);
    assert!(matches!(
        result.native_status,
        NativeClosureStatus::Blocked { .. }
    ));
    assert!(
        !result
            .inferred
            .iter()
            .any(|row| !row.is_edb && row.predicate == "urn:joint:absence")
    );
    assert!(
        result
            .source_coverage
            .worlds
            .values()
            .flat_map(|world| &world.admissions)
            .any(|owner| !owner.obstructions.is_empty())
    );
}

#[test]
fn ordinary_unconstrained_data_is_not_an_isolated_family_vocabulary_refusal() {
    let rows = [
        quad(X, TYPE, "urn:joint:r"),
        quad(
            "urn:joint:r",
            "https://blackcatinformatics.ca/logic/onProperty",
            "urn:joint:p",
        ),
        quad(
            "urn:joint:r",
            "https://blackcatinformatics.ca/logic/onClass",
            C,
        ),
        RdfQuad::new(
            RdfTerm::iri("urn:joint:r"),
            "https://blackcatinformatics.ca/logic/minQualifiedCardinality",
            RdfTerm::literal(RdfLiteral::typed(
                "0",
                "http://www.w3.org/2001/XMLSchema#nonNegativeInteger",
            )),
        ),
        quad(X, "urn:ordinary:unconstrained", Z),
    ];
    let result = run(&rows, vec![]);
    assert!(
        result
            .native_families
            .iter()
            .flat_map(|ledger| &ledger.outcomes)
            .flat_map(|outcome| &outcome.obstructions)
            .all(
                |obstruction| !obstruction.detail.contains("unhandled predicate")
                    && !obstruction.detail.contains("source role")
            )
    );
    assert!(
        result
            .inferred
            .iter()
            .any(|row| row.is_edb && row.predicate == "urn:ordinary:unconstrained")
    );
}

#[test]
fn every_world_has_actual_chase_admission_even_when_zero_budget_commits_no_domain_head() {
    let selected = SelectedLogicalWorld::new(
        LogicalGraph::Named(TermValue::iri("urn:joint:empty")),
        DomainProfile::NonemptyObjectDomainV1,
        "urn:test:explicit-domain".to_owned(),
        [41; 32],
    )
    .unwrap();
    let domains = SelectedDomains::new([selected]).unwrap();
    let program = LogicProgram::new(vec![], vec![], vec![], None);
    let prepared = crate::program_analysis::prepare_program(&program).unwrap();
    let result = execute(
        &prepared,
        prepare_reasoning_input(&source(&[])).unwrap(),
        &domains,
        Some(0),
    )
    .unwrap();
    assert_eq!(result.certificates.len(), result.graphs.len());
    assert!(
        result
            .certificates
            .iter()
            .all(|certificate| certificate.input_contract == result.input_contract)
    );
    assert!(result.inferred.iter().all(|row| row.is_edb));
    assert!(result.witnesses.is_empty());
    assert_eq!(result.native_status, NativeClosureStatus::Exhausted);
    assert!(
        result
            .native_families
            .iter()
            .all(|ledger| ledger.outcomes.len() == 4)
    );
    assert!(
        result
            .native_families
            .iter()
            .all(|ledger| ledger.source_terms == NativeSourceTerms::RdfSkolem)
    );
}

#[test]
fn malformed_class_preflight_retains_typed_world_source_and_input_identity_before_writers() {
    let rows = [
        quad(
            "urn:joint:selected",
            "https://blackcatinformatics.ca/logic/oneOf",
            "urn:joint:unfinished",
        ),
        quad(
            "urn:joint:unfinished",
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#first",
            X,
        ),
    ];
    let source = source(&rows);
    let program = LogicProgram::new(vec![], vec![absence()], vec![], None);
    let error = execute(
        &crate::program_analysis::prepare_program(&program).unwrap(),
        prepare_reasoning_input(&source).unwrap(),
        &SelectedDomains::new([]).unwrap(),
        None,
    )
    .err()
    .expect("malformed original selected grammar refuses admission");
    let typed = error
        .downcast_ref::<crate::error::NativeSourceAdmission>()
        .expect("typed original source refusal");
    assert_ne!(typed.input_contract, [0; 32]);
    assert_eq!(
        typed.admission.refusal_class().unwrap(),
        Some(crate::reason::refute::ClassSourceRefusal::Invalid)
    );
    assert!(
        typed
            .admission
            .selected_worlds
            .values()
            .all(|world| !world.definitions.is_empty() && world.refusal.is_some())
    );
}

fn modal_rows(endpoint: &str) -> Vec<RdfQuad> {
    use crate::modal::{
        ATOM_OBJECT, ATOM_PREDICATE, ATOM_SUBJECT, MODAL_EVAL_WORLD, NECESSARILY,
        OVER_ACCESSIBILITY,
    };
    let mut rows = vec![
        ("urn:modal:F", NECESSARILY, "urn:modal:B"),
        (
            "urn:modal:F",
            OVER_ACCESSIBILITY,
            "https://blackcatinformatics.ca/logic/epistemicallyPossible",
        ),
        ("urn:modal:F", MODAL_EVAL_WORLD, "urn:modal:w0"),
        ("urn:modal:B", ATOM_SUBJECT, X),
        ("urn:modal:B", ATOM_PREDICATE, "urn:modal:p"),
        ("urn:modal:B", ATOM_OBJECT, Y),
        (
            "urn:modal:w0",
            "https://blackcatinformatics.ca/logic/epistemicallyPossible",
            endpoint,
        ),
        ("urn:modal:F", "urn:modal:watch", "urn:modal:B"),
    ]
    .into_iter()
    .map(|(s, p, o)| {
        let mut row = quad(s, p, o);
        row.graph_name = Some(RdfTerm::iri("urn:modal:owner"));
        row
    })
    .collect::<Vec<_>>();
    let mut seed = quad(X, "urn:modal:seed", Y);
    seed.graph_name = Some(RdfTerm::iri(endpoint));
    rows.push(seed);
    rows
}

#[test]
fn cross_world_modal_read_observes_late_native_writer_before_owner_consumer_and_naf() {
    let mut rows = modal_rows("urn:modal:reached");
    let mut unrelated = quad("urn:modal:F", "urn:modal:watch", "urn:modal:B");
    unrelated.graph_name = Some(RdfTerm::iri("urn:modal:unrelated"));
    rows.push(unrelated);
    let derive = LogicRule::new(
        atom("?x", "urn:modal:p", Y, false),
        vec![atom("?x", "urn:modal:seed", Y, false)],
        vec![],
        ContextualScope::default(),
    );
    let consume = LogicRule::new(
        atom("?f", "urn:modal:ready", "urn:modal:B", false),
        vec![atom(
            "?f",
            crate::modal::MODAL_NECESSITY_HOLDS,
            "urn:modal:B",
            false,
        )],
        vec![],
        ContextualScope::default(),
    );
    let absent = LogicRule::new(
        atom("?f", "urn:modal:absent", "urn:modal:B", false),
        vec![
            atom("?f", "urn:modal:watch", "urn:modal:B", false),
            atom("?f", "urn:modal:ready", "urn:modal:B", true),
        ],
        vec![],
        ContextualScope::default(),
    );
    let result = run(&rows, vec![derive, consume, absent]);
    assert_eq!(result.native_status, NativeClosureStatus::Completed);
    let ready: Vec<_> = result
        .inferred
        .iter()
        .filter(|row| !row.is_edb && row.predicate == "urn:modal:ready")
        .map(|row| row.world.as_str())
        .collect();
    assert_eq!(ready, ["urn:modal:owner"]);
    let absent: Vec<_> = result
        .inferred
        .iter()
        .filter(|row| !row.is_edb && row.predicate == "urn:modal:absent")
        .map(|row| row.world.as_str())
        .collect();
    assert_eq!(absent, ["urn:modal:unrelated"]);
    let owner = result
        .native_families
        .iter()
        .find(|ledger| ledger.world == "urn:modal:owner")
        .unwrap();
    let proof = owner
        .proofs
        .iter()
        .find(|proof| matches!(proof.origin, NativeProofOrigin::Modal { .. }))
        .unwrap();
    let NativeProofOrigin::Modal { evidence } = &proof.origin else {
        panic!("actual modal application")
    };
    let body = evidence.supports.last().unwrap();
    assert_eq!(body.world, "urn:modal:reached");
    let reached = result
        .native_families
        .iter()
        .find(|ledger| ledger.world == body.world)
        .unwrap();
    let actual = reached
        .proofs
        .iter()
        .find(|proof| proof.id == body.proof)
        .unwrap();
    assert_eq!(actual.statement.predicate, "urn:modal:p");
    assert!(matches!(actual.origin, NativeProofOrigin::Derived { .. }));
    assert!(
        result
            .inferred
            .iter()
            .filter(|row| row.modal_evaluation.is_some())
            .all(|row| row.world == "urn:modal:owner" && !row.premises.is_empty())
    );
}

#[test]
fn modal_absence_cycle_is_refused_by_the_same_world_qualified_producer_graph() {
    let rows = modal_rows("urn:modal:owner");
    let rule = LogicRule::new(
        atom(X, "urn:modal:p", Y, false),
        vec![atom(
            "urn:modal:F",
            crate::modal::MODAL_NECESSITY_HOLDS,
            "urn:modal:B",
            false,
        )],
        vec![],
        ContextualScope::default(),
    );
    let program = LogicProgram::new(vec![], vec![rule], vec![], None);
    let error = execute(
        &crate::program_analysis::prepare_program(&program).unwrap(),
        prepare_reasoning_input(&source(&rows)).unwrap(),
        &SelectedDomains::new([]).unwrap(),
        None,
    )
    .err()
    .expect("recursive completed modal premise is not admitted");
    assert!(error.message().contains("NonStratifiable"));
}

#[test]
fn reachable_modal_grammar_writer_is_refused_before_source_definition_can_change() {
    let rows = modal_rows("urn:modal:reached");
    let rule = LogicRule::new(
        atom(
            "urn:modal:F",
            crate::modal::MODAL_EVAL_WORLD,
            "urn:modal:forged",
            false,
        ),
        vec![atom("urn:modal:F", "urn:modal:watch", "urn:modal:B", false)],
        vec![],
        ContextualScope::default(),
    );
    let program = LogicProgram::new(vec![], vec![rule], vec![], None);
    let error = execute(
        &crate::program_analysis::prepare_program(&program).unwrap(),
        prepare_reasoning_input(&source(&rows)).unwrap(),
        &SelectedDomains::new([]).unwrap(),
        None,
    )
    .err()
    .expect("immutable modal grammar");
    let kind = error
        .downcast_ref::<crate::error::NativeCoverage>()
        .expect("typed source capability refusal");
    assert_eq!(kind.profile, "native-immutable-modal-source-v1");
    assert_eq!(kind.world, "urn:modal:owner");
}
