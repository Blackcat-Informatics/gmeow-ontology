// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Local proof ownership and source admission over tiny native inputs.

use super::*;
use crate::native_semantics::SemanticVocabulary;
use crate::physical::{JointTemplate, NativeOutcome};
use crate::rule_ir::{EvalAtom, EvalRule, EvalTerm, Fact};
use purrdf::{RdfDatasetBuilder, RdfLiteral, RdfQuad};
use std::sync::Arc;

const W: &str = "urn:scope:world";
const OTHER: &str = "urn:scope:other-world";
const X: &str = "urn:scope:x";
const Y: &str = "urn:scope:y";
const Z: &str = "urn:scope:z";
const A: &str = "urn:scope:a";
const B: &str = "urn:scope:b";
const C: &str = "urn:scope:class";
const NOT_C: &str = "urn:scope:not-class";
const D: &str = "urn:scope:disconnected-class";
const U: &str = "urn:scope:union";
const H: &str = "urn:scope:head";
const T: &str = "urn:scope:tail";
const AFFECTED: &str = "urn:scope:affected";

fn row(world: &str, subject: &str, predicate: &str, object: &str) -> RdfQuad {
    RdfQuad::new(RdfTerm::iri(subject), predicate, RdfTerm::iri(object))
        .in_graph(RdfTerm::iri(world))
}
fn fact(subject: &str, predicate: &str, object: &str) -> RdfQuad {
    row(W, subject, predicate, object)
}
fn dataset(rows: &[RdfQuad]) -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for row in rows {
        builder.push_owned_quad(row);
    }
    builder.freeze().unwrap()
}
fn premises(rows: &[RdfQuad]) -> BTreeSet<RefutationPremise> {
    rows.iter()
        .map(|row| RefutationPremise {
            subject: crate::reason::dataset::value(&row.subject),
            predicate: row.predicate.clone(),
            object: crate::reason::dataset::value(&row.object),
            graph: row.graph_name.as_ref().map(crate::reason::dataset::value),
        })
        .collect()
}
fn contradiction(dataset: &RdfDataset) -> Witness {
    let Some(RefutationCertificate::InFragment {
        decision: Decision::Inconsistent,
        witness,
    }) = decide(dataset)
    else {
        panic!("the selected theory must have a supported contradiction")
    };
    assert_eq!(witness.evidence.contextual_conflicts.len(), 1);
    witness
}
fn heads(witness: &Witness) -> BTreeSet<String> {
    witness
        .clashes
        .iter()
        .map(|clash| clash.individual.clone())
        .collect()
}
fn complement(canonical: bool) -> Vec<RdfQuad> {
    let membership = if canonical {
        "https://blackcatinformatics.ca/logic/instanceOf"
    } else {
        RDF_TYPE
    };
    let complement = if canonical {
        "https://blackcatinformatics.ca/logic/complementOf"
    } else {
        OWL_COMPLEMENT_OF
    };
    vec![
        fact(X, membership, C),
        fact(X, membership, NOT_C),
        fact(NOT_C, complement, C),
        fact(Z, membership, D),
    ]
}
fn list2(first: &str, second: &str) -> Vec<RdfQuad> {
    vec![
        fact(H, RDF_FIRST, first),
        fact(H, RDF_REST, T),
        fact(T, RDF_FIRST, second),
        fact(T, RDF_REST, RDF_NIL),
    ]
}

/// Execute the actual local publication delta through an ordinary native consumer.
fn downstream(witness: &Witness) -> BTreeSet<String> {
    let rule = EvalRule::positive(
        "urn:scope:consumer",
        EvalAtom::positive(
            EvalTerm::Var("?x".to_owned()),
            AFFECTED,
            EvalTerm::ConstNamed(C.to_owned()),
        ),
        vec![EvalAtom::positive(
            EvalTerm::Var("?x".to_owned()),
            RDF_TYPE,
            EvalTerm::ConstNamed(OWL_NOTHING.to_owned()),
        )],
    );
    let template = Arc::new(
        JointTemplate::new(&[rule], &[], &[], SemanticVocabulary::GroundedLogicV1).unwrap(),
    );
    let mut facts: BTreeMap<String, Vec<Fact>> = BTreeMap::new();
    for clash in &witness.clashes {
        facts.entry(clash.world.clone()).or_default().push(Fact {
            subject: TermValue::iri(&clash.individual),
            predicate: RDF_TYPE.to_owned(),
            object: TermValue::iri(OWL_NOTHING),
        });
    }
    let input = template
        .input(&facts, std::sync::Arc::from([]), &[])
        .unwrap();
    let NativeOutcome::Decided(program) = input.prepare().unwrap() else {
        panic!("finite positive consumer")
    };
    let NativeOutcome::Decided(result) = program.materialize_input(&input, None).unwrap() else {
        panic!("complete positive consumer")
    };
    result
        .result
        .rows
        .iter()
        .filter(|row| row.predicate == AFFECTED)
        .map(|row| row.subject.as_iri().unwrap().to_owned())
        .collect()
}

#[test]
fn disconnected_membership_never_becomes_a_local_clash_or_downstream_fact() {
    for canonical in [false, true] {
        let rows = complement(canonical);
        let witness = contradiction(&dataset(&rows));
        assert_eq!(heads(&witness), BTreeSet::from([X.to_owned()]));
        assert_eq!(downstream(&witness), BTreeSet::from([X.to_owned()]));
        let conflict = &witness.evidence.contextual_conflicts[0];
        assert_eq!(conflict.world, W);
        assert_eq!(conflict.proof.premises(), premises(&rows[..3]));
        assert!(witness.evidence.source_boundaries.is_empty());
        assert!(
            witness
                .clashes
                .iter()
                .all(|clash| clash.premises.len() == 3)
        );
    }
}

#[test]
fn equality_paths_are_source_premises_and_only_justify_their_own_resources() {
    let rows = [
        fact(X, RDF_TYPE, C),
        fact(Y, RDF_TYPE, NOT_C),
        fact(NOT_C, OWL_COMPLEMENT_OF, C),
        fact(X, OWL_SAME_AS, Y),
        fact(Z, RDF_TYPE, D),
    ];
    let witness = contradiction(&dataset(&rows));
    assert_eq!(
        heads(&witness),
        BTreeSet::from([X.to_owned(), Y.to_owned()])
    );
    assert_eq!(
        witness.evidence.contextual_conflicts[0].proof.premises(),
        premises(&rows[..4])
    );
    assert_eq!(downstream(&witness), heads(&witness));
}

#[test]
fn singleton_nominal_clash_retains_memberships_definitions_list_and_inequality() {
    let rows = [
        fact(U, OWL_ONE_OF, H),
        fact(H, RDF_FIRST, A),
        fact(H, RDF_REST, RDF_NIL),
        fact(X, RDF_TYPE, U),
        fact(Y, RDF_TYPE, U),
        fact(X, OWL_DIFFERENT_FROM, Y),
        fact(Z, RDF_TYPE, D),
    ];
    let witness = contradiction(&dataset(&rows));
    assert_eq!(
        heads(&witness),
        BTreeSet::from([X.to_owned(), Y.to_owned(), A.to_owned()])
    );
    assert_eq!(
        witness.evidence.contextual_conflicts[0].proof.premises(),
        premises(&rows[..6])
    );
    assert!(!downstream(&witness).contains(Z));
}

#[test]
fn every_disjunction_branch_retains_its_own_assumption_and_closing_sources() {
    let not_d = "urn:scope:not-disconnected-class";
    let mut rows = vec![
        fact(X, RDF_TYPE, U),
        fact(U, OWL_UNION_OF, H),
        fact(A, RDFS_SUBCLASSOF, C),
        fact(A, RDFS_SUBCLASSOF, NOT_C),
        fact(NOT_C, OWL_COMPLEMENT_OF, C),
        fact(B, RDFS_SUBCLASSOF, D),
        fact(B, RDFS_SUBCLASSOF, not_d),
        fact(not_d, OWL_COMPLEMENT_OF, D),
    ];
    rows.extend(list2(A, B));
    let expected = premises(&rows);
    rows.push(fact(Z, RDF_TYPE, "urn:scope:unrelated"));
    let witness = contradiction(&dataset(&rows));
    let proof = &witness.evidence.contextual_conflicts[0].proof;
    assert_eq!(proof.premises(), expected);
    let RefutationProof::Cases { branches, .. } = proof else {
        panic!("both source alternatives must be closed")
    };
    assert_eq!(branches.len(), 2);
    let assumptions = branches
        .iter()
        .map(|branch| branch.assumption.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        assumptions,
        BTreeSet::from([A, B].map(|class| RefutationAssumption::Membership {
            subject: X.to_owned(),
            expression: Concept::Pos(class.to_owned()),
        }))
    );
    assert_eq!(heads(&witness), BTreeSet::from([X.to_owned()]));
    assert_eq!(downstream(&witness), heads(&witness));
}

#[test]
fn nominal_alternatives_do_not_publish_branch_local_equalities_as_global_memberships() {
    let mut rows = vec![
        fact(X, RDF_TYPE, U),
        fact(U, OWL_ONE_OF, H),
        fact(X, OWL_DIFFERENT_FROM, A),
        fact(X, OWL_DIFFERENT_FROM, B),
    ];
    rows.extend(list2(A, B));
    let witness = contradiction(&dataset(&rows));
    let proof = &witness.evidence.contextual_conflicts[0].proof;
    let RefutationProof::Cases { branches, .. } = proof else {
        panic!("exhaustive nominal equality choice")
    };
    assert_eq!(branches.len(), 2);
    assert_eq!(
        branches
            .iter()
            .map(|branch| branch.assumption.clone())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([A, B].map(|member| RefutationAssumption::Equality {
            subject: X.to_owned(),
            member: member.to_owned()
        }))
    );
    assert_eq!(proof.premises(), premises(&rows));
    assert_eq!(heads(&witness), BTreeSet::from([X.to_owned()]));
    assert_eq!(downstream(&witness), heads(&witness));
}

#[test]
fn source_order_and_duplicate_physical_rows_preserve_the_same_supported_proof() {
    let mut rows = complement(true);
    rows.push(fact(X, OWL_SAME_AS, Y));
    let expected = contradiction(&dataset(&rows));
    rows.reverse();
    rows.extend(rows.clone());
    assert_eq!(contradiction(&dataset(&rows)), expected);
}

#[test]
fn source_membership_does_not_cross_asserting_worlds_without_a_bridge() {
    let rows = [
        fact(X, RDF_TYPE, C),
        row(OTHER, X, RDF_TYPE, NOT_C),
        fact(NOT_C, OWL_COMPLEMENT_OF, C),
        row(OTHER, NOT_C, OWL_COMPLEMENT_OF, C),
    ];
    assert!(matches!(
        decide(dataset(&rows).as_ref()),
        Some(RefutationCertificate::InFragment {
            decision: Decision::Consistent,
            ..
        })
    ));
    let mut conflict_rows = rows.to_vec();
    conflict_rows.push(fact(X, RDF_TYPE, NOT_C));
    let witness = contradiction(&dataset(&conflict_rows));
    assert!(witness.clashes.iter().all(|clash| clash.world == W));
    assert!(
        witness.evidence.contextual_conflicts[0]
            .proof
            .premises()
            .iter()
            .all(|premise| premise.graph == Some(TermValue::iri(W)))
    );
}

#[test]
fn asserted_annotation_sources_contribute_without_asserting_their_quoted_triples() {
    let rows = complement(true);
    let mut builder = RdfDatasetBuilder::new();
    let world = builder.intern_iri(W);
    let quoted_subject = builder.intern_iri(Z);
    let quoted_predicate = builder.intern_iri(RDF_TYPE);
    let quoted_object = builder.intern_iri(OWL_NOTHING);
    let quoted = builder.intern_triple(quoted_subject, quoted_predicate, quoted_object);
    let mut reifiers = BTreeSet::new();
    for row in &rows[..3] {
        let subject = builder.intern_owned_term(&row.subject);
        if reifiers.insert(subject) {
            builder.push_reifier_in_graph(subject, quoted, Some(world));
        }
        let predicate = builder.intern_iri(&row.predicate);
        let object = builder.intern_owned_term(&row.object);
        builder.push_annotation_in_graph(subject, predicate, object, Some(world));
    }
    builder.push_owned_quad(&rows[0]);
    builder.push_owned_quad(&rows[3]);
    let witness = contradiction(&builder.freeze().unwrap());
    assert_eq!(
        witness.evidence.contextual_conflicts[0].proof.premises(),
        premises(&rows[..3])
    );
    assert_eq!(heads(&witness), BTreeSet::from([X.to_owned()]));
    assert_eq!(downstream(&witness), heads(&witness));
}

#[test]
fn selected_nil_retains_exact_literal_input_and_its_owner_as_a_source_boundary() {
    let row = RdfQuad::new(
        RdfTerm::iri(RDF_NIL),
        RDF_FIRST,
        RdfTerm::Literal(RdfLiteral::typed(
            "+001",
            "http://www.w3.org/2001/XMLSchema#integer",
        )),
    )
    .in_graph(RdfTerm::iri(W));
    let rows = [fact(U, OWL_ONE_OF, RDF_NIL), row];
    let Some(RefutationCertificate::OutOfFragment {
        reason:
            FragmentBoundary::SourceAdmission {
                world,
                issue,
                premises: actual,
            },
    }) = decide(dataset(&rows).as_ref())
    else {
        panic!("malformed grammar is not a model contradiction")
    };
    assert_eq!(world, W);
    assert_eq!(issue, RefutationSourceIssue::MalformedNil);
    assert_eq!(actual.into_iter().collect::<BTreeSet<_>>(), premises(&rows));
}

#[test]
fn truncated_selected_list_is_not_an_empty_or_exhaustive_enumeration() {
    let rows = [
        fact(X, RDF_TYPE, U),
        fact(U, OWL_ONE_OF, H),
        fact(H, RDF_FIRST, A),
    ];
    let Some(RefutationCertificate::OutOfFragment {
        reason:
            FragmentBoundary::SourceAdmission {
                issue,
                premises: actual,
                ..
            },
    }) = decide(dataset(&rows).as_ref())
    else {
        panic!("a missing tail is an input boundary")
    };
    assert_eq!(
        issue,
        RefutationSourceIssue::IncompleteList {
            owner: U.to_owned(),
            head: H.to_owned()
        }
    );
    assert_eq!(
        actual.into_iter().collect::<BTreeSet<_>>(),
        premises(&rows[1..])
    );
}

#[test]
fn conflicting_list_fields_and_multiple_expression_definitions_have_distinct_boundaries() {
    let rows = [
        fact(X, RDF_TYPE, U),
        fact(U, OWL_ONE_OF, H),
        fact(H, RDF_FIRST, A),
        fact(H, RDF_FIRST, B),
        fact(H, RDF_REST, RDF_NIL),
    ];
    let Some(RefutationCertificate::OutOfFragment {
        reason: FragmentBoundary::SourceAdmission { issue, .. },
    }) = decide(dataset(&rows).as_ref())
    else {
        panic!("ambiguous list grammar")
    };
    assert_eq!(
        issue,
        RefutationSourceIssue::ConflictingListField {
            subject: H.to_owned(),
            predicate: RDF_FIRST.to_owned()
        }
    );
    let rows = [
        fact(NOT_C, OWL_COMPLEMENT_OF, C),
        fact(NOT_C, OWL_COMPLEMENT_OF, D),
        fact(X, RDF_TYPE, NOT_C),
    ];
    let Some(RefutationCertificate::OutOfFragment {
        reason: FragmentBoundary::SourceAdmission { issue, .. },
    }) = decide(dataset(&rows).as_ref())
    else {
        panic!("multiple valid RDF definitions require conjunctive admission")
    };
    assert_eq!(
        issue,
        RefutationSourceIssue::ExpressionMultiplicity {
            subject: NOT_C.to_owned(),
            predicate: OWL_COMPLEMENT_OF.to_owned()
        }
    );
}

#[test]
fn valid_conflict_and_unadmitted_other_world_are_both_retained_without_false_local_heads() {
    let mut rows = complement(false);
    rows.push(row(OTHER, U, OWL_ONE_OF, RDF_NIL));
    rows.push(row(OTHER, RDF_NIL, RDF_FIRST, A));
    let witness = contradiction(&dataset(&rows));
    assert!(
        !decides(dataset(&rows).as_ref()),
        "a local conflict cannot certify all selected source grammar"
    );
    assert_eq!(heads(&witness), BTreeSet::from([X.to_owned()]));
    assert_eq!(witness.evidence.contextual_conflicts[0].world, W);
    assert!(
        matches!(&witness.evidence.source_boundaries[..], [FragmentBoundary::SourceAdmission { world, issue: RefutationSourceIssue::MalformedNil, .. }] if world == OTHER)
    );
}

#[test]
fn genuine_empty_nominal_has_real_owner_premises_and_no_rdf_nil_membership() {
    let rows = [
        fact(X, RDF_TYPE, U),
        fact(U, OWL_ONE_OF, RDF_NIL),
        fact(Z, RDF_TYPE, D),
    ];
    let witness = contradiction(&dataset(&rows));
    assert_eq!(heads(&witness), BTreeSet::from([X.to_owned()]));
    assert_eq!(
        witness.evidence.contextual_conflicts[0].proof.premises(),
        premises(&rows[..2])
    );
    assert!(
        witness
            .clashes
            .iter()
            .all(|clash| !clash.premises.is_empty())
    );
}

#[test]
fn disjoint_union_literal_members_cannot_be_dropped_to_fabricate_an_empty_union() {
    let mut rows = vec![
        fact(X, RDF_TYPE, U),
        fact(U, OWL_DISJOINT_UNION_OF, H),
        fact(H, RDF_REST, RDF_NIL),
    ];
    rows.push(
        RdfQuad::new(
            RdfTerm::iri(H),
            RDF_FIRST,
            RdfTerm::Literal(RdfLiteral::simple("not-a-resource")),
        )
        .in_graph(RdfTerm::iri(W)),
    );
    let Some(RefutationCertificate::OutOfFragment {
        reason: FragmentBoundary::SourceAdmission { issue, .. },
    }) = decide(dataset(&rows).as_ref())
    else {
        panic!("unsupported member cannot become an exhaustive empty union")
    };
    assert_eq!(
        issue,
        RefutationSourceIssue::UnsupportedListMember {
            owner: U.to_owned(),
            head: H.to_owned()
        }
    );
}

#[test]
fn native_contextual_proof_and_source_boundary_survive_typed_transport() {
    let mut rows = vec![
        fact(X, RDF_TYPE, U),
        fact(U, OWL_ONE_OF, H),
        fact(X, OWL_DIFFERENT_FROM, A),
        fact(X, OWL_DIFFERENT_FROM, B),
    ];
    rows.extend(list2(A, B));
    rows.push(row(OTHER, U, OWL_ONE_OF, RDF_NIL));
    rows.push(row(OTHER, RDF_NIL, RDF_FIRST, Z));
    let certificate = decide(dataset(&rows).as_ref()).unwrap();
    let encoded = serde_json::to_vec(&certificate).unwrap();
    let recovered: RefutationCertificate = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(recovered, certificate);
    let RefutationCertificate::InFragment { witness, .. } = recovered else {
        panic!("supported selected-world contradiction")
    };
    assert_eq!(heads(&witness), BTreeSet::from([X.to_owned()]));
    assert_eq!(witness.evidence.source_boundaries.len(), 1);
    assert!(matches!(
        witness.evidence.contextual_conflicts[0].proof,
        RefutationProof::Cases { .. }
    ));
}

#[test]
fn nominal_members_keep_exact_data_identity_even_when_their_iris_name_class_markers() {
    let canonical_thing = "https://blackcatinformatics.ca/logic/Thing";
    let rows = [
        fact(X, RDF_TYPE, U),
        fact(U, OWL_ONE_OF, H),
        fact(H, RDF_FIRST, canonical_thing),
        fact(H, RDF_REST, RDF_NIL),
        fact(X, OWL_DIFFERENT_FROM, OWL_THING),
    ];
    assert!(
        matches!(
            decide(dataset(&rows).as_ref()),
            Some(RefutationCertificate::OutOfFragment { .. })
        ),
        "nominal data spellings are distinct resources without an explicit equality; this fragment still withholds nominal consistency"
    );
    let mut contradicted = rows.to_vec();
    contradicted.push(fact(canonical_thing, OWL_SAME_AS, OWL_THING));
    let witness = contradiction(&dataset(&contradicted));
    assert_eq!(
        witness.evidence.contextual_conflicts[0].proof.premises(),
        premises(&contradicted)
    );
}

fn unsupported_expression_operand_is_retained(operand: RdfTerm) {
    for predicate in [
        OWL_COMPLEMENT_OF,
        OWL_INTERSECTION_OF,
        OWL_UNION_OF,
        OWL_ONE_OF,
        OWL_DISJOINT_UNION_OF,
    ] {
        for other_engagement in [false, true] {
            let definition =
                RdfQuad::new(RdfTerm::iri(U), predicate, operand.clone()).in_graph(RdfTerm::iri(W));
            let mut rows = vec![fact(X, RDF_TYPE, U), definition.clone()];
            if other_engagement {
                rows.push(fact(NOT_C, OWL_COMPLEMENT_OF, C));
            }
            let input = dataset(&rows);
            let Some(RefutationCertificate::OutOfFragment {
                reason:
                    FragmentBoundary::SourceAdmission {
                        world,
                        issue,
                        premises: actual,
                    },
            }) = decide(input.as_ref())
            else {
                panic!(
                    "an unsupported operand must neither disappear nor receive a consistency certificate: {predicate}, other engagement {other_engagement}"
                )
            };
            assert_eq!(world, W);
            assert_eq!(
                issue,
                RefutationSourceIssue::UnsupportedExpressionOperand {
                    owner: U.to_owned(),
                    predicate: predicate.to_owned(),
                    operand: crate::reason::dataset::value(&operand),
                }
            );
            assert_eq!(
                actual.into_iter().collect::<BTreeSet<_>>(),
                premises(&[definition])
            );
            assert!(!decides(input.as_ref()));
        }
    }
}

#[test]
fn literal_expression_operands_cannot_disappear_before_family_admission() {
    unsupported_expression_operand_is_retained(RdfTerm::Literal(RdfLiteral::typed(
        "+001",
        "http://www.w3.org/2001/XMLSchema#integer",
    )));
}

#[test]
fn quoted_expression_operands_cannot_disappear_before_family_admission() {
    unsupported_expression_operand_is_retained(RdfTerm::triple(purrdf::RdfTriple::new(
        RdfTerm::iri(Z),
        RDF_TYPE,
        RdfTerm::iri(OWL_NOTHING),
    )));
}

/// The same ordinary RDF data is outside this procedure in each native context.
#[test]
fn unowned_list_fields_do_not_select_refutation_in_any_native_context() {
    for graph in [
        None,
        Some(RdfTerm::iri(W)),
        Some(RdfTerm::blank_node("context")),
    ] {
        let mut rows = vec![
            fact(RDF_NIL, RDF_FIRST, A),
            fact(RDF_NIL, RDF_REST, H),
            fact(H, RDF_FIRST, A),
            fact(H, RDF_FIRST, B),
            fact(H, RDF_REST, H),
            fact(U, OWL_MEMBERS, H),
        ];
        rows.push(RdfQuad::new(
            RdfTerm::iri(U),
            OWL_DISTINCT_MEMBERS,
            RdfTerm::Literal(RdfLiteral::typed(
                "+001",
                "http://www.w3.org/2001/XMLSchema#integer",
            )),
        ));
        for row in &mut rows {
            row.graph_name = graph.clone();
        }
        assert!(
            decide(dataset(&rows).as_ref()).is_none(),
            "unselected context {graph:?}"
        );
    }
}

/// A selected complement does not acquire ownership of unrelated list-shaped rows.
#[test]
fn disconnected_list_fields_do_not_change_a_supported_world_conflict() {
    let original = complement(true);
    let expected = contradiction(&dataset(&original));
    for graph in [W, OTHER] {
        let mut rows = original.clone();
        rows.extend([
            row(graph, RDF_NIL, RDF_FIRST, A),
            row(graph, H, RDF_FIRST, A),
            row(graph, H, RDF_FIRST, B),
            row(graph, H, RDF_REST, H),
            row(graph, U, OWL_MEMBERS, H),
        ]);
        let actual = contradiction(&dataset(&rows));
        assert_eq!(
            actual, expected,
            "ordinary data in {graph} changed native proof"
        );
        assert!(actual.evidence.source_boundaries.is_empty());
        assert_eq!(downstream(&actual), BTreeSet::from([X.to_owned()]));
    }
}

/// The owner and every reached path row justify admission failure; disconnected data does not.
#[test]
fn selected_list_failure_retains_owner_prefix_and_reached_fields_only() {
    let selected = vec![
        fact(U, OWL_ONE_OF, H),
        fact(H, RDF_FIRST, A),
        fact(H, RDF_REST, T),
        fact(T, RDF_FIRST, A),
        fact(T, RDF_FIRST, B),
        fact(T, RDF_REST, RDF_NIL),
    ];
    let mut rows = selected.clone();
    rows.extend([fact(Z, RDF_FIRST, A), fact(Z, RDF_REST, Z)]);
    let Some(RefutationCertificate::OutOfFragment {
        reason:
            FragmentBoundary::SourceAdmission {
                world,
                premises: actual,
                issue,
            },
    }) = decide(dataset(&rows).as_ref())
    else {
        panic!("selected ambiguous tail")
    };
    assert_eq!(world, W);
    assert_eq!(
        issue,
        RefutationSourceIssue::ConflictingListField {
            subject: T.to_owned(),
            predicate: RDF_FIRST.to_owned(),
        }
    );
    assert_eq!(
        actual.into_iter().collect::<BTreeSet<_>>(),
        premises(&selected)
    );
}

/// Repeated definitions retain all authored list heads and distinct structural failures.
#[test]
fn all_selected_definition_and_list_failures_are_retained_deterministically() {
    let rows = vec![
        fact(U, OWL_UNION_OF, H),
        fact(U, OWL_UNION_OF, T),
        fact(H, RDF_FIRST, A),
        fact(T, RDF_FIRST, B),
        fact(T, RDF_REST, T),
        fact(C, OWL_INTERSECTION_OF, Z),
        fact(Z, RDF_FIRST, A),
        fact(Z, RDF_FIRST, B),
        fact(Z, RDF_REST, RDF_NIL),
    ];
    let expected = decide(dataset(&rows).as_ref()).unwrap();
    let RefutationCertificate::OutOfFragment {
        reason: FragmentBoundary::Combined(boundaries),
    } = &expected
    else {
        panic!("every selected issue must survive")
    };
    assert_eq!(boundaries.len(), 4);
    let issues = boundaries
        .iter()
        .map(|boundary| match boundary {
            FragmentBoundary::SourceAdmission {
                world,
                issue,
                premises: actual,
            } => {
                assert_eq!(world, W);
                let expected_support = match issue {
                    RefutationSourceIssue::ExpressionMultiplicity { .. } => premises(&rows[..2]),
                    RefutationSourceIssue::IncompleteList { .. } => {
                        premises(&[rows[0].clone(), rows[2].clone()])
                    }
                    RefutationSourceIssue::CyclicList { .. } => {
                        premises(&[rows[1].clone(), rows[3].clone(), rows[4].clone()])
                    }
                    RefutationSourceIssue::ConflictingListField { .. } => premises(&rows[5..]),
                    other => panic!("unexpected selected issue {other:?}"),
                };
                assert_eq!(
                    actual.iter().cloned().collect::<BTreeSet<_>>(),
                    expected_support
                );
                issue.clone()
            }
            other => panic!("wrong source disposition {other:?}"),
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        issues,
        BTreeSet::from([
            RefutationSourceIssue::ExpressionMultiplicity {
                subject: U.to_owned(),
                predicate: OWL_UNION_OF.to_owned()
            },
            RefutationSourceIssue::IncompleteList {
                owner: U.to_owned(),
                head: H.to_owned()
            },
            RefutationSourceIssue::CyclicList {
                owner: U.to_owned(),
                head: T.to_owned()
            },
            RefutationSourceIssue::ConflictingListField {
                subject: Z.to_owned(),
                predicate: RDF_FIRST.to_owned()
            },
        ])
    );
    let mut permuted = rows.clone();
    permuted.reverse();
    permuted.extend(rows);
    assert_eq!(decide(dataset(&permuted).as_ref()), Some(expected.clone()));
    let bytes = serde_json::to_vec(&expected).unwrap();
    assert_eq!(
        serde_json::from_slice::<RefutationCertificate>(&bytes).unwrap(),
        expected
    );
}

/// A multiple-tail refusal retains the actual paths of every source alternative.
#[test]
fn conflicting_tail_retains_each_reachable_source_failure() {
    let rows = [
        fact(U, OWL_ONE_OF, H),
        fact(H, RDF_FIRST, A),
        fact(H, RDF_REST, T),
        fact(H, RDF_REST, Z),
        fact(T, RDF_FIRST, B),
        fact(T, RDF_REST, T),
        fact(Z, RDF_FIRST, C),
    ];
    let Some(RefutationCertificate::OutOfFragment {
        reason: FragmentBoundary::Combined(boundaries),
    }) = decide(dataset(&rows).as_ref())
    else {
        panic!("three selected structural failures")
    };
    assert_eq!(boundaries.len(), 3);
    let mut issues = BTreeSet::new();
    for boundary in boundaries {
        let FragmentBoundary::SourceAdmission {
            world,
            issue,
            premises: actual,
        } = boundary
        else {
            panic!("source evidence")
        };
        assert_eq!(world, W);
        assert_eq!(actual.into_iter().collect::<BTreeSet<_>>(), premises(&rows));
        issues.insert(issue);
    }
    assert_eq!(
        issues,
        BTreeSet::from([
            RefutationSourceIssue::ConflictingListField {
                subject: H.to_owned(),
                predicate: RDF_REST.to_owned()
            },
            RefutationSourceIssue::IncompleteList {
                owner: U.to_owned(),
                head: H.to_owned()
            },
            RefutationSourceIssue::CyclicList {
                owner: U.to_owned(),
                head: H.to_owned()
            },
        ])
    );
}

/// A member-list property becomes a native distinctness source through its declaration.
#[test]
fn all_different_requires_its_actual_declaration_to_select_list_admission() {
    for predicate in [OWL_MEMBERS, OWL_DISTINCT_MEMBERS] {
        let mut rows = vec![fact(U, predicate, H), fact(H, RDF_FIRST, A)];
        assert!(decide(dataset(&rows).as_ref()).is_none());
        rows.push(fact(U, RDF_TYPE, OWL_ALL_DIFFERENT));
        let Some(RefutationCertificate::OutOfFragment {
            reason:
                FragmentBoundary::SourceAdmission {
                    world,
                    issue,
                    premises: actual,
                },
        }) = decide(dataset(&rows).as_ref())
        else {
            panic!("declared incomplete source")
        };
        assert_eq!(world, W);
        assert_eq!(
            issue,
            RefutationSourceIssue::IncompleteList {
                owner: U.to_owned(),
                head: H.to_owned()
            }
        );
        assert_eq!(actual.into_iter().collect::<BTreeSet<_>>(), premises(&rows));

        let operand = RdfTerm::Literal(RdfLiteral::typed(
            "+001",
            "http://www.w3.org/2001/XMLSchema#integer",
        ));
        let mut rows = vec![
            RdfQuad::new(RdfTerm::iri(U), predicate, operand.clone()).in_graph(RdfTerm::iri(W)),
        ];
        assert!(decide(dataset(&rows).as_ref()).is_none());
        rows.push(fact(U, RDF_TYPE, OWL_ALL_DIFFERENT));
        let Some(RefutationCertificate::OutOfFragment {
            reason:
                FragmentBoundary::SourceAdmission {
                    world,
                    issue,
                    premises: actual,
                },
        }) = decide(dataset(&rows).as_ref())
        else {
            panic!("declared unsupported list operand")
        };
        assert_eq!(world, W);
        assert_eq!(
            issue,
            RefutationSourceIssue::UnsupportedExpressionOperand {
                owner: U.to_owned(),
                predicate: predicate.to_owned(),
                operand: crate::reason::dataset::value(&operand),
            }
        );
        assert_eq!(actual.into_iter().collect::<BTreeSet<_>>(), premises(&rows));
    }
}

/// Identically named list nodes in another context cannot complete a selected path.
#[test]
fn selected_list_source_never_borrows_fields_from_other_worlds() {
    let selected = [fact(U, OWL_ONE_OF, H), fact(H, RDF_FIRST, A)];
    let mut rows = selected.to_vec();
    rows.push(row(OTHER, H, RDF_REST, RDF_NIL));
    let Some(RefutationCertificate::OutOfFragment {
        reason:
            FragmentBoundary::SourceAdmission {
                world,
                issue,
                premises: actual,
            },
    }) = decide(dataset(&rows).as_ref())
    else {
        panic!("selected missing tail")
    };
    assert_eq!(world, W);
    assert_eq!(
        issue,
        RefutationSourceIssue::IncompleteList {
            owner: U.to_owned(),
            head: H.to_owned()
        }
    );
    assert_eq!(
        actual.into_iter().collect::<BTreeSet<_>>(),
        premises(&selected)
    );
}

/// Default and scoped blank worlds retain their original native assertion context.
#[test]
fn selected_source_boundary_keeps_default_and_blank_graph_premises() {
    for graph in [None, Some(RdfTerm::blank_node("selected-context"))] {
        let mut rows = [fact(U, OWL_ONE_OF, H), fact(H, RDF_FIRST, A)];
        for row in &mut rows {
            row.graph_name = graph.clone();
        }
        let Some(RefutationCertificate::OutOfFragment {
            reason:
                FragmentBoundary::SourceAdmission {
                    world,
                    issue,
                    premises: actual,
                },
        }) = decide(dataset(&rows).as_ref())
        else {
            panic!("selected native context")
        };
        assert_eq!(world, world_key(&graph));
        assert_eq!(
            issue,
            RefutationSourceIssue::IncompleteList {
                owner: U.to_owned(),
                head: H.to_owned()
            }
        );
        assert_eq!(actual.into_iter().collect::<BTreeSet<_>>(), premises(&rows));
    }
}

/// A shared suffix belongs to both selected definitions without becoming a cycle.
#[test]
fn shared_selected_list_tail_retains_order_and_independent_owners() {
    let rows = [
        fact(U, OWL_UNION_OF, H),
        fact(C, OWL_UNION_OF, T),
        fact(H, RDF_FIRST, A),
        fact(H, RDF_REST, T),
        fact(T, RDF_FIRST, B),
        fact(T, RDF_REST, RDF_NIL),
    ];
    let input = dataset(&rows);
    let scan = Scan::of(input.as_ref());
    assert!(scan.worlds[W].source_boundary.is_none());
    assert_eq!(
        scan.worlds[W].lists[H],
        vec![RdfTerm::iri(A), RdfTerm::iri(B)]
    );
    assert_eq!(scan.worlds[W].lists[T], vec![RdfTerm::iri(B)]);
    let owners: BTreeSet<_> = scan.worlds[W]
        .selected_lists()
        .into_iter()
        .map(|(owner, head, support)| (owner.to_owned(), head.to_owned(), support.rows()))
        .collect();
    assert_eq!(
        owners,
        BTreeSet::from([
            (
                U.to_owned(),
                H.to_owned(),
                premises(&rows[..1]).into_iter().collect()
            ),
            (
                C.to_owned(),
                T.to_owned(),
                premises(&rows[1..2]).into_iter().collect()
            ),
        ])
    );
    assert_eq!(
        scan.worlds[W].list_premises[H]
            .rows()
            .into_iter()
            .collect::<BTreeSet<_>>(),
        premises(&rows[2..])
    );
    assert_eq!(
        scan.worlds[W].list_premises[T]
            .rows()
            .into_iter()
            .collect::<BTreeSet<_>>(),
        premises(&rows[4..])
    );
    assert!(matches!(
        decide(input.as_ref()),
        Some(RefutationCertificate::InFragment {
            decision: Decision::Consistent,
            ..
        })
    ));
}

/// Multiple authored tails can reconverge; ambiguity alone does not establish a cycle.
#[test]
fn reconverging_selected_tails_are_ambiguous_without_a_cyclic_path() {
    let rows = [
        fact(U, OWL_ONE_OF, H),
        fact(H, RDF_FIRST, A),
        fact(H, RDF_REST, X),
        fact(H, RDF_REST, Y),
        fact(X, RDF_FIRST, A),
        fact(X, RDF_REST, T),
        fact(Y, RDF_FIRST, B),
        fact(Y, RDF_REST, T),
        fact(T, RDF_FIRST, C),
        fact(T, RDF_REST, RDF_NIL),
    ];
    let Some(RefutationCertificate::OutOfFragment {
        reason:
            FragmentBoundary::SourceAdmission {
                issue,
                premises: actual,
                ..
            },
    }) = decide(dataset(&rows).as_ref())
    else {
        panic!("one ambiguous selected field")
    };
    assert_eq!(
        issue,
        RefutationSourceIssue::ConflictingListField {
            subject: H.to_owned(),
            predicate: RDF_REST.to_owned(),
        }
    );
    assert_eq!(actual.into_iter().collect::<BTreeSet<_>>(), premises(&rows));
}

/// A quoted owner is an explicit unsupported selected expression, never its inner subject.
/// Non-owning RDF data and empty graph declarations are retained without selecting a calculus.
#[test]
fn unrelated_quoted_data_does_not_select_class_admission_and_empty_graphs_survive() {
    let owner = RdfTerm::triple(purrdf::RdfTriple::new(
        RdfTerm::iri(Z),
        RDF_TYPE,
        RdfTerm::iri(OWL_NOTHING),
    ));
    let mut builder = RdfDatasetBuilder::new();
    builder.push_owned_quad(
        &RdfQuad::new(owner, "urn:ordinary-data", RdfTerm::iri(C)).in_graph(RdfTerm::iri(W)),
    );
    let empty = builder.intern_iri(OTHER);
    builder.declare_named_graph(empty);
    let input = builder.freeze().unwrap();
    let prepared = PreparedClassAnalysis::new(input.as_ref()).unwrap();
    assert!(prepared.admission().outside_selection());
    assert_eq!(prepared.admission().source_worlds[W].assertions, 1);
    assert_eq!(prepared.admission().source_worlds[OTHER].assertions, 0);
    assert_eq!(
        prepared.admission().source_worlds[OTHER].graph,
        Some(TermValue::iri(OTHER))
    );
    assert!(decide(input.as_ref()).is_none());
}

/// Distinct native graph identities may not be aliased through an execution spelling.
#[test]
fn source_admission_rejects_default_named_world_aliases() {
    let rows = [
        RdfQuad::new(RdfTerm::iri(U), OWL_UNION_OF, RdfTerm::iri(RDF_NIL)),
        RdfQuad::new(RdfTerm::iri(C), OWL_UNION_OF, RdfTerm::iri(RDF_NIL))
            .in_graph(RdfTerm::iri(crate::reason::rl::DEFAULT_WORLD)),
    ];
    let input = dataset(&rows);
    assert!(PreparedClassAnalysis::new(input.as_ref()).is_err());
}

/// Invalid selected grammar dominates classification while every sibling cause remains explicit.
#[test]
fn malformed_and_unsupported_source_causes_keep_the_complete_typed_admission() {
    let rows = [
        RdfQuad::new(
            RdfTerm::iri(Z),
            OWL_COMPLEMENT_OF,
            RdfTerm::Literal(RdfLiteral::typed(
                "+001",
                "http://www.w3.org/2001/XMLSchema#integer",
            )),
        )
        .in_graph(RdfTerm::iri(W)),
        fact(C, OWL_UNION_OF, H),
    ];
    let input = dataset(&rows);
    let prepared = PreparedClassAnalysis::new(input.as_ref()).unwrap();
    let observation = prepared.admission();
    assert_eq!(
        observation.refusal_class().unwrap(),
        Some(super::ClassSourceRefusal::Invalid)
    );
    let Some(FragmentBoundary::Combined(causes)) = &observation.selected_worlds[W].refusal else {
        panic!("all selected source causes must survive the aggregate");
    };
    assert_eq!(causes.len(), 2);
    assert!(causes.iter().any(|cause| matches!(
        cause,
        FragmentBoundary::SourceAdmission {
            issue: RefutationSourceIssue::UnsupportedExpressionOperand { .. },
            ..
        }
    )));
    assert!(causes.iter().any(|cause| matches!(
        cause,
        FragmentBoundary::SourceAdmission {
            issue: RefutationSourceIssue::IncompleteList { .. },
            ..
        }
    )));
}
