// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Object witnesses must feed the authored fixed point with exact world-local
//! evidence. These independent inputs never build or read the repository corpus.

use super::*;
use gmeow_logic_compile::ir::{Formula, LogicProgram, Term};
use purrdf::{RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm};

const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const ON_PROPERTY: &str = "https://blackcatinformatics.ca/logic/onProperty";
const ON_CLASS: &str = "https://blackcatinformatics.ca/logic/onClass";
const MIN: &str = "https://blackcatinformatics.ca/logic/minQualifiedCardinality";
const EXACT: &str = "https://blackcatinformatics.ca/logic/qualifiedCardinality";
const DIFFERENT: &str = "http://www.w3.org/2002/07/owl#differentFrom";
const P: &str = "urn:minimum:p";
const R: &str = "urn:minimum:restriction";
const C: &str = "urn:minimum:class";
const S: &str = "urn:minimum:subject";
const SEEN: &str = "urn:minimum:seen";

fn fact(s: &str, p: &str, o: &str) -> RdfQuad {
    RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o))
}

fn request(predicate: &str, count: &str, class: &str) -> Vec<RdfQuad> {
    vec![
        fact(R, ON_PROPERTY, P),
        fact(R, ON_CLASS, class),
        fact(S, TYPE, R),
        RdfQuad::new(
            RdfTerm::iri(R),
            predicate,
            RdfTerm::Literal(RdfLiteral::typed(
                count,
                "http://www.w3.org/2001/XMLSchema#integer",
            )),
        ),
    ]
}

fn source(rows: Vec<RdfQuad>) -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for row in rows {
        builder.push_owned_quad(&row);
    }
    builder.freeze().unwrap()
}

fn copying(edges: &[(&str, &str)]) -> LogicProgram {
    let atom = |predicate: &str| {
        Formula::atom(
            Term::iri(predicate).unwrap(),
            vec![Term::var("x").unwrap(), Term::var("y").unwrap()],
        )
        .unwrap()
    };
    LogicProgram::new(vec![], vec![], vec![], None).with_formulas(
        edges
            .iter()
            .map(|(from, to)| Formula::Forall {
                vars: vec!["x".to_owned(), "y".to_owned()],
                body: Box::new(Formula::Implies(Box::new(atom(from)), Box::new(atom(to)))),
            })
            .collect(),
    )
}

fn run(
    rows: Vec<RdfQuad>,
    edges: &[(&str, &str)],
    budget: Option<u64>,
) -> gmeow_errors::Result<ProgramClosure> {
    let prepared = crate::program_analysis::prepare_program(&copying(edges))?;
    execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&source(rows)).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        budget,
    )
}

fn fillers(result: &ProgramClosure, predicate: &str) -> std::collections::BTreeSet<TermValue> {
    result
        .inferred
        .iter()
        .filter(|row| row.subject == S && row.predicate == predicate)
        .map(|row| row.object.clone())
        .collect()
}

#[test]
fn qualified_minimum_and_exact_witnesses_feed_native_consumers_with_actual_premises() {
    for predicate in [MIN, EXACT] {
        let result = run(request(predicate, "2", C), &[(P, SEEN)], Some(100)).unwrap();
        assert_eq!(result.status, crate::seam::BudgetStatus::Ok);
        assert_eq!(fillers(&result, P).len(), 2);
        assert_eq!(fillers(&result, P), fillers(&result, SEEN));
        assert_eq!(result.witnesses.len(), 2);
        for row in result.inferred.iter().filter(|row| {
            row.rule_name
                .as_deref()
                .is_some_and(|name| name.starts_with("dl:minimum-witness:"))
        }) {
            assert_eq!(row.premises.len(), 4);
            assert!(
                row.premises
                    .iter()
                    .any(|(s, p, _)| s == R && p == predicate)
            );
            assert!(
                row.premises
                    .iter()
                    .any(|(s, p, _)| s == R && p == ON_PROPERTY)
            );
            assert!(row.premises.iter().any(|(s, p, _)| s == R && p == ON_CLASS));
            assert!(row.premises.iter().any(|(s, p, _)| s == S && p == TYPE));
        }
        assert!(result.inferred.iter().any(|row| row.predicate == DIFFERENT));
    }
}

#[test]
fn universal_qualifiers_reuse_existing_resources_without_requiring_type_evidence() {
    const EXISTING: &str = "urn:minimum:existing";
    for class in [
        "https://blackcatinformatics.ca/logic/Thing",
        "http://www.w3.org/2002/07/owl#Thing",
    ] {
        let mut rows = request(MIN, "1", class);
        rows.push(fact(S, P, EXISTING));
        let result = run(rows, &[(P, SEEN)], Some(100)).unwrap();
        assert_eq!(result.status, crate::seam::BudgetStatus::Ok);
        assert!(result.witnesses.is_empty(), "universal qualifier {class}");
        assert_eq!(
            fillers(&result, P),
            [TermValue::iri(EXISTING)].into_iter().collect()
        );
        assert_eq!(fillers(&result, P), fillers(&result, SEEN));
        assert!(result.inferred.iter().any(|row| {
            row.subject == R && row.predicate == ON_CLASS && row.object == TermValue::iri(class)
        }));
    }
}

#[test]
fn a_nonuniversal_qualifier_still_requires_explicit_member_evidence() {
    for class in [C, "urn:minimum:Thing"] {
        let mut rows = request(MIN, "1", class);
        rows.push(fact(S, P, "urn:minimum:existing"));
        let result = run(rows, &[(P, SEEN)], Some(100)).unwrap();
        assert_eq!(result.status, crate::seam::BudgetStatus::Ok);
        assert_eq!(result.witnesses.len(), 1);
        assert_eq!(fillers(&result, P).len(), 2);
        assert_eq!(fillers(&result, P), fillers(&result, SEEN));
        assert!(result.inferred.iter().any(|row| {
            row.predicate == TYPE && row.object == TermValue::iri(class) && !row.is_edb
        }));
    }
}

#[test]
fn a_complete_distinct_subset_blocks_invention_after_a_failed_greedy_prefix() {
    let mut rows = request(MIN, "2", C);
    for value in ["urn:minimum:a", "urn:minimum:b", "urn:minimum:c"] {
        rows.extend([fact(S, P, value), fact(value, TYPE, C)]);
    }
    rows.push(fact("urn:minimum:c", DIFFERENT, "urn:minimum:b"));
    let result = run(rows, &[(P, SEEN)], Some(100)).unwrap();
    assert_eq!(result.status, crate::seam::BudgetStatus::Ok);
    assert!(
        result.witnesses.is_empty(),
        "the reverse-only b/c pair satisfies the lower bound"
    );
    assert_eq!(fillers(&result, SEEN).len(), 3);
}

#[test]
fn late_restriction_fields_activate_the_witness_family_in_the_same_fixed_point() {
    let mut rows = request(MIN, "2", C);
    for row in &mut rows {
        row.predicate = match row.predicate.as_str() {
            ON_PROPERTY => "urn:minimum:property-seed",
            ON_CLASS => "urn:minimum:class-seed",
            MIN => "urn:minimum:count-seed",
            _ => continue,
        }
        .to_owned();
    }
    let result = run(
        rows,
        &[
            ("urn:minimum:property-seed", ON_PROPERTY),
            ("urn:minimum:class-seed", ON_CLASS),
            ("urn:minimum:count-seed", MIN),
            (P, SEEN),
        ],
        Some(100),
    )
    .unwrap();
    assert_eq!(result.status, crate::seam::BudgetStatus::Ok);
    assert_eq!(fillers(&result, SEEN).len(), 2);
}

#[test]
fn witness_identity_retains_the_world_even_for_identical_source_terms() {
    let mut rows = Vec::new();
    for world in ["urn:minimum:world-a", "urn:minimum:world-b"] {
        for row in request(MIN, "1", C) {
            rows.push(row.in_graph(RdfTerm::iri(world)));
        }
    }
    let result = run(rows, &[(P, SEEN)], Some(100)).unwrap();
    assert_eq!(result.status, crate::seam::BudgetStatus::Ok);
    let witnesses = fillers(&result, SEEN);
    assert_eq!(
        witnesses.len(),
        2,
        "separate standpoints cannot share one witness identity"
    );
    assert_eq!(result.witnesses.len(), 2);
}

#[test]
fn a_huge_count_withholds_before_allocating_witnesses_or_a_quadratic_head() {
    let result = run(
        // The native XSD integer value representation is signed i128. Its largest
        // admitted count still overflows a naive quadratic u128 head calculation.
        request(MIN, &i128::MAX.to_string(), C),
        &[(P, SEEN)],
        Some(100),
    )
    .unwrap();
    assert_eq!(result.status, crate::seam::BudgetStatus::Exhausted);
    assert!(result.witnesses.is_empty());
    assert!(fillers(&result, P).is_empty());
    assert!(!result.frontier.saturated_preds.contains(P));
}

#[test]
fn cyclic_witnesses_withhold_without_publishing_a_model_only_self_loop() {
    let result = run(request(MIN, "1", R), &[(P, SEEN)], Some(12)).unwrap();
    assert_eq!(result.status, crate::seam::BudgetStatus::Exhausted);
    assert!(
        result
            .inferred
            .iter()
            .filter(|row| row.predicate == P)
            .all(|row| row.object != TermValue::iri(&row.subject))
    );
    assert!(!result.frontier.saturated_preds.contains(P));
}

#[test]
fn finite_object_witnesses_receive_source_bound_unbounded_admission() {
    let result = run(request(MIN, "2", C), &[(P, SEEN)], None).unwrap();
    assert_eq!(result.status, crate::seam::BudgetStatus::Ok);
    assert_eq!(fillers(&result, SEEN).len(), 2);
    assert!(!result.certificates.is_empty());
    assert_ne!(result.input_contract, [0; 32]);
    assert!(
        result
            .certificates
            .iter()
            .all(|certificate| certificate.admission.admits_native())
    );
    assert!(
        result
            .certificates
            .iter()
            .all(|certificate| certificate.input_contract == result.input_contract)
    );
}

#[test]
fn cached_finite_admission_cannot_authorize_a_cyclic_definition_with_the_same_abstract_columns() {
    let prepared = crate::program_analysis::prepare_program(&copying(&[(P, SEEN)])).unwrap();
    let finite = source(request(MIN, "2", C));
    let cyclic = source(request(MIN, "2", R));
    let first = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&finite).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
    .unwrap();
    assert!(
        execute(
            &prepared,
            crate::reason::program::prepare_reasoning_input(&cyclic).unwrap(),
            &crate::physical::SelectedDomains::new([]).unwrap(),
            None
        )
        .is_err()
    );
    let again = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&finite).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(first.certificates, again.certificates);
    assert_eq!(fillers(&first, SEEN), fillers(&again, SEEN));
}

#[test]
fn every_immutable_definition_binding_participates_and_mutable_fields_are_not_frozen() {
    let mut ambiguous = request(MIN, "1", C);
    ambiguous.push(fact(R, ON_CLASS, R));
    assert!(run(ambiguous, &[(P, SEEN)], None).is_err());

    let mut mutable = request(MIN, "1", C);
    mutable.push(fact(R, "urn:minimum:class-seed", R));
    assert!(
        run(
            mutable,
            &[("urn:minimum:class-seed", ON_CLASS), (P, SEEN)],
            None
        )
        .is_err()
    );
}

#[test]
fn static_inheritance_can_activate_a_finite_witness_chain_without_a_runtime_budget() {
    let mut rows = request(MIN, "1", C);
    rows.retain(|row| row.predicate != TYPE);
    rows.extend([
        fact(S, TYPE, "urn:minimum:subclass"),
        fact(
            "urn:minimum:subclass",
            "https://blackcatinformatics.ca/logic/subClassOf",
            R,
        ),
    ]);
    let result = run(rows, &[(P, SEEN)], None).unwrap();
    assert_eq!(result.status, crate::seam::BudgetStatus::Ok);
    assert_eq!(fillers(&result, SEEN).len(), 1);
}

#[test]
fn unqualified_object_and_some_value_obligations_share_the_witness_pipeline() {
    const OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
    for predicate in [
        "http://www.w3.org/2002/07/owl#minCardinality",
        "http://www.w3.org/2002/07/owl#cardinality",
    ] {
        let mut rows = request(predicate, "2", C);
        rows.retain(|row| row.predicate != ON_CLASS);
        rows.push(fact(P, TYPE, OBJECT_PROPERTY));
        let result = run(rows, &[(P, SEEN)], Some(100)).unwrap();
        assert_eq!(fillers(&result, SEEN).len(), 2);
    }
    let result = run(
        vec![
            fact(R, ON_PROPERTY, P),
            fact(S, TYPE, R),
            fact(P, TYPE, OBJECT_PROPERTY),
            fact(R, "http://www.w3.org/2002/07/owl#someValuesFrom", C),
        ],
        &[(P, SEEN)],
        Some(100),
    )
    .unwrap();
    assert_eq!(fillers(&result, SEEN).len(), 1);
}

#[test]
fn universal_object_qualification_accepts_existing_resources_without_invented_membership() {
    let mut rows = request(MIN, "1", "http://www.w3.org/2002/07/owl#Thing");
    rows.push(fact(S, P, "urn:minimum:existing"));
    let result = run(rows, &[(P, SEEN)], Some(100)).unwrap();
    assert!(result.witnesses.is_empty());
    assert_eq!(fillers(&result, SEEN).len(), 1);
}

#[test]
fn an_unfinished_distinct_subset_probe_never_authorizes_invention() {
    // Eight pairs of indistinguishable resources: every cross-pair inequality is
    // present, but no nine-clique exists. Searching its subsets exceeds the small
    // traversal budget even though nine new witnesses would fit its row ceiling.
    let mut rows = request(MIN, "9", C);
    let values: Vec<_> = (0..16)
        .map(|index| format!("urn:minimum:value:{index:02}"))
        .collect();
    for (index, value) in values.iter().enumerate() {
        rows.extend([fact(S, P, value), fact(value, TYPE, C)]);
        for (prior, other) in values[..index].iter().enumerate() {
            if prior / 2 != index / 2 {
                rows.push(fact(other, DIFFERENT, value));
            }
        }
    }
    let limited = run(rows.clone(), &[(P, SEEN)], Some(64)).unwrap();
    assert_eq!(limited.status, crate::seam::BudgetStatus::Exhausted);
    assert!(limited.witnesses.is_empty());
    assert!(!limited.frontier.saturated_preds.contains(P));
    let complete = run(rows, &[(P, SEEN)], Some(100_000)).unwrap();
    assert_eq!(complete.status, crate::seam::BudgetStatus::Ok);
    assert_eq!(complete.witnesses.len(), 9);
}

#[test]
fn recursion_requiring_three_distinct_siblings_cannot_hide_in_a_two_ordinal_analysis() {
    let atom = |predicate: &str, left: &str, right: Term| {
        Formula::atom(
            Term::iri(predicate).unwrap(),
            vec![Term::var(left).unwrap(), right],
        )
        .unwrap()
    };
    let body = Formula::And(vec![
        atom(DIFFERENT, "a", Term::var("b").unwrap()),
        atom(DIFFERENT, "b", Term::var("c").unwrap()),
        atom(DIFFERENT, "a", Term::var("c").unwrap()),
    ]);
    let program = copying(&[]).with_formulas(vec![Formula::Forall {
        vars: vec!["a".into(), "b".into(), "c".into()],
        body: Box::new(Formula::Implies(
            Box::new(body),
            Box::new(atom(TYPE, "a", Term::iri(R).unwrap())),
        )),
    }]);
    let prepared = crate::program_analysis::prepare_program(&program).unwrap();
    let source = source(request(MIN, "3", C));
    assert!(
        execute(
            &prepared,
            crate::reason::program::prepare_reasoning_input(&source).unwrap(),
            &crate::physical::SelectedDomains::new([]).unwrap(),
            None
        )
        .is_err()
    );
    let bounded = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&source).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        Some(100),
    )
    .unwrap();
    assert_eq!(bounded.status, crate::seam::BudgetStatus::Exhausted);
    assert!(!bounded.frontier.saturated_preds.contains(P));
}

#[test]
fn an_existing_existential_witness_does_not_materialize_the_rest_of_its_relation() {
    let mut rows = request(MIN, "1", C);
    for index in 0..256 {
        let value = format!("urn:minimum:existing:{index}");
        rows.extend([fact(S, P, &value), fact(&value, TYPE, C)]);
    }
    let result = run(rows, &[], Some(8)).unwrap();
    assert_eq!(result.status, crate::seam::BudgetStatus::Ok);
    assert!(result.witnesses.is_empty());
}
