// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_errors::GateVerdict;
use purrdf::{DatasetView, RdfDataset, RdfDatasetBuilder};

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

fn empty_edb() -> std::sync::Arc<RdfDataset> {
    RdfDatasetBuilder::new()
        .freeze()
        .expect("an empty dataset is valid")
}

/// Build a tiny typed witness value for isolated summary/codec controls.
fn counted_witness() -> RefutationCertificate {
    let clash = NothingClash {
        individual: "http://ex/i".to_owned(),
        world: "http://ex/w".to_owned(),
        rule_name: "refute:toy-counting".to_owned(),
        premises: vec![(
            "http://ex/i".to_owned(),
            RDF_TYPE.to_owned(),
            "http://ex/A".to_owned(),
        )],
    };
    certify_membership(FragmentFamily::Counting, BTreeSet::new(), || {
        (
            Decision::Inconsistent,
            Witness {
                family: FragmentFamily::Counting,
                clashes: [clash].into_iter().collect(),
                evidence: WitnessEvidence {
                    contextual_conflicts: Vec::new(),
                    source_boundaries: Vec::new(),
                    counted_individuals: ["http://ex/a".to_owned(), "http://ex/b".to_owned()]
                        .into_iter()
                        .collect(),
                    violated_bound: Some(CountBound {
                        kind: BoundKind::Max,
                        value: 1,
                        on_property: "http://ex/p".to_owned(),
                    }),
                    closed_branch: None,
                },
            },
        )
    })
}

/// Build a typed capability boundary without invoking an execution dispatcher.
fn counting_boundary() -> RefutationCertificate {
    let obstructions: BTreeSet<String> = [
        "unbounded max cardinality on <http://ex/p>".to_owned(),
        "min 2 > max 1 on <http://ex/q>".to_owned(),
    ]
    .into_iter()
    .collect();
    certify_membership(FragmentFamily::Counting, obstructions, || {
        unreachable!("a withhold never decides")
    })
}

// (4a) A hand-built in-fragment case yields `InFragment` with the correct
// decision and a deterministic structured witness.
#[test]
fn in_fragment_case_yields_decision_and_structured_witness() {
    let certificate = counted_witness();
    let RefutationCertificate::InFragment { decision, witness } = certificate else {
        panic!("the typed witness is in-fragment: {certificate:?}");
    };
    assert_eq!(decision, Decision::Inconsistent);
    assert_eq!(witness.family, FragmentFamily::Counting);
    // The structured witness carries the counted individuals, the violated
    // bound, and the clash — never a rendered string.
    assert_eq!(witness.clashes.len(), 1);
    let clash = witness.clashes.iter().next().expect("one clash");
    assert_eq!(clash.individual, "http://ex/i");
    assert_eq!(clash.world, "http://ex/w");
    assert_eq!(
        witness.evidence.counted_individuals,
        ["http://ex/a".to_owned(), "http://ex/b".to_owned()]
            .into_iter()
            .collect()
    );
    assert_eq!(
        witness.evidence.violated_bound,
        Some(CountBound {
            kind: BoundKind::Max,
            value: 1,
            on_property: "http://ex/p".to_owned(),
        })
    );
}

/// A valid unsupported capability remains an explicit non-error finding.
#[test]
fn out_of_fragment_case_yields_ledger_identified_boundary() {
    let certificate = counting_boundary();
    let RefutationCertificate::OutOfFragment { reason } = &certificate else {
        panic!("a withhold must never be a decision: {certificate:?}");
    };
    assert!(matches!(
        reason,
        FragmentBoundary::Uncertified {
            family: FragmentFamily::Counting,
            ..
        }
    ));

    // The boundary derives a ledger-identified finding stamped with the
    // disjoint kernel category, at the Coherent UnsupportedSemanticFeature
    // grade, so it can NEVER gate the DL/EL crosscheck.
    let ledger = boundary_diag_ledger(reason);
    let findings = ledger.findings("reason");
    assert_eq!(findings.len(), 1, "one boundary finding: {findings:?}");
    let finding = &findings[0];
    assert_eq!(
        finding.category,
        Some(FindingCategory::UnsupportedSemanticFeature)
    );
    assert!(
        finding.code.contains(REFUTATION_KERNEL_CATEGORY),
        "code carries the disjoint kernel category: {}",
        finding.code
    );
    assert_eq!(
        ledger.verdict(),
        GateVerdict::Collected,
        "a valid unsupported capability remains distinct from malformed input"
    );

    // The kernel category is disjoint from every DL/EL crosscheck category.
    assert_ne!(REFUTATION_KERNEL_CATEGORY, "consistency");
    assert_ne!(REFUTATION_KERNEL_CATEGORY, "subsumption");
    assert_ne!(REFUTATION_KERNEL_CATEGORY, "external-corpus");
    assert_ne!(
        REFUTATION_KERNEL_CATEGORY,
        crate::reason::ledger::EXISTENTIAL_CHASE_CATEGORY
    );
}

/// No source owner selects the class grammar in an empty input; this is not a
/// claim about any other native fragment or world consistency.
#[test]
fn empty_source_is_outside_class_admission_selection() {
    let edb = empty_edb();
    let prepared = PreparedClassAnalysis::new(edb.as_ref()).unwrap();
    let admission = prepared.admission();
    admission.validate().unwrap();
    assert!(admission.outside_selection());
    assert!(admission.selected_worlds.is_empty());
    assert_eq!(admission.refusal_class().unwrap(), None);
    assert_eq!(casesplit::decide(edb.as_ref()), None);
}

/// Every distinct family cause survives deterministic combined projection.
#[test]
fn multiple_withholds_combine_deterministically() {
    let RefutationCertificate::OutOfFragment { reason: counting } = counting_boundary() else {
        unreachable!("the synthetic boundary carries explicit obstructions")
    };
    let class = FragmentBoundary::Uncertified {
        family: FragmentFamily::CaseSplit,
        obstructions: ["unbounded disjunction".to_owned()].into_iter().collect(),
    };
    let boundaries: BTreeSet<_> = [counting.clone(), class.clone()].into_iter().collect();
    assert_eq!(boundaries.len(), 2, "one boundary per withholding family");
    let forward = FragmentBoundary::Combined(boundaries);
    let reverse = FragmentBoundary::Combined([class, counting].into_iter().collect());
    let findings = boundary_diag_ledger(&forward).findings("reason");
    assert_eq!(findings, boundary_diag_ledger(&reverse).findings("reason"));
    assert_eq!(
        findings.len(),
        2,
        "each disjoint cause retains its own finding"
    );
    assert_ne!(findings[0].finding_iri, findings[1].finding_iri);
}

// (4c) Determinism: the same input yields byte-identical certificate output
// across two runs (canonical `BTreeSet`/sorted ordering makes the Debug
// rendering byte-stable).
#[test]
fn certificate_output_is_byte_identical_across_runs() {
    let first = format!("{:?}", counted_witness());
    let second = format!("{:?}", counted_witness());
    assert_eq!(first, second, "in-fragment certificate must be byte-stable");

    let first_boundary = format!("{:?}", counting_boundary());
    let second_boundary = format!("{:?}", counting_boundary());
    assert_eq!(
        first_boundary, second_boundary,
        "out-of-fragment boundary must be byte-stable"
    );
}

#[test]
fn typed_certificate_transport_retains_large_bounds_worlds_and_boundaries() {
    let mut counted = counted_witness();
    let RefutationCertificate::InFragment { witness, .. } = &mut counted else {
        unreachable!("the synthetic counting certificate decides")
    };
    witness.evidence.violated_bound.as_mut().unwrap().value = 1u128 << 64;
    for certificate in [counted, counting_boundary()] {
        let bytes = serde_json::to_vec(&certificate).unwrap();
        let recovered: RefutationCertificate = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(recovered, certificate);
        assert_eq!(format!("{recovered:?}"), format!("{certificate:?}"));
    }
}

#[test]
fn typed_refutation_transport_keeps_native_source_and_context_proofs() {
    use purrdf::{BlankScope, RdfLiteral, RdfQuad, RdfTerm, RdfTextDirection, RdfTriple};

    let subject = RdfTerm::blank_node(BlankScope(17).qualify_label("subject"));
    let graph = RdfTerm::blank_node(BlankScope(23).qualify_label("world"));
    let literal = RdfTerm::literal(RdfLiteral {
        lexical_form: "retained source".to_owned(),
        datatype: Some("http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString".to_owned()),
        language: Some("ar".to_owned()),
        direction: Some(RdfTextDirection::Rtl),
    });
    let quoted = RdfTerm::triple(RdfTriple::new(
        subject.clone(),
        "urn:source:predicate",
        literal.clone(),
    ));
    for graph in [None, Some(graph)] {
        for operand in [
            RdfTerm::iri("urn:source:C"),
            literal.clone(),
            quoted.clone(),
        ] {
            let mut builder = RdfDatasetBuilder::new();
            for (s, p, o) in [
                (subject.clone(), RDF_TYPE, RdfTerm::iri("urn:source:C")),
                (subject.clone(), RDF_TYPE, RdfTerm::iri("urn:source:not-C")),
                (
                    RdfTerm::iri("urn:source:not-C"),
                    "https://blackcatinformatics.ca/logic/complementOf",
                    operand.clone(),
                ),
            ] {
                let mut row = RdfQuad::new(s, p, o);
                row.graph_name = graph.clone();
                builder.push_owned_quad(&row);
            }
            let source = builder.freeze().unwrap();
            let certificate = casesplit::decide(source.as_ref()).expect("selected complement");
            match &certificate {
                RefutationCertificate::InFragment { decision, witness } => {
                    assert_eq!(*decision, Decision::Inconsistent);
                    let [conflict] = &witness.evidence.contextual_conflicts[..] else {
                        panic!("the actual complement source must retain its contextual proof")
                    };
                    assert_eq!(conflict.proof.premises().len(), 3);
                }
                RefutationCertificate::OutOfFragment { reason } => {
                    assert!(matches!(
                        reason,
                        FragmentBoundary::SourceAdmission {
                            issue: RefutationSourceIssue::UnsupportedExpressionOperand { .. },
                            ..
                        }
                    ));
                }
            }
            let bytes = serde_json::to_vec(&certificate).unwrap();
            let recovered: RefutationCertificate = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                recovered, certificate,
                "native source fields must not be erased"
            );
            if let RefutationCertificate::OutOfFragment { .. } = certificate {
                let mut corrupt: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                corrupt["OutOfFragment"]["reason"]["SourceAdmission"]["premises"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("graph")
                    .expect("explicit graph field");
                assert!(
                    serde_json::from_value::<RefutationCertificate>(corrupt).is_err(),
                    "a missing source graph is not an explicitly selected default graph"
                );
            }
        }
    }
}

/// Joint native execution retains the original cardinality bound and the real
/// existential witness, while a zero-budget run cannot claim completion.
#[test]
fn entangled_cardinality_and_existential_share_native_proofs_and_budget() {
    use purrdf::{RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm};

    const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
    const OWL_RESTRICTION: &str = "http://www.w3.org/2002/07/owl#Restriction";
    const OWL_ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
    const OWL_MIN_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#minCardinality";
    const OWL_SOME_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#someValuesFrom";
    const RDFS_SUBCLASSOF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
    const XSD_NNI: &str = "http://www.w3.org/2001/XMLSchema#nonNegativeInteger";
    const W: &str = "http://ex/w";

    let iri_q = |s: &str, p: &str, o: &str| {
        RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(W))
    };

    let mut b = RdfDatasetBuilder::new();
    for q in [
        // A populated class C with a min-1 cardinality restriction on p …
        iri_q("http://ex/C", RDF_TYPE, OWL_CLASS),
        iri_q("http://ex/i", RDF_TYPE, "http://ex/C"),
        iri_q("http://ex/C", RDFS_SUBCLASSOF, "http://ex/r1"),
        iri_q("http://ex/r1", RDF_TYPE, OWL_RESTRICTION),
        iri_q("http://ex/r1", OWL_ON_PROPERTY, "http://ex/p"),
        // … ENTANGLED with a someValuesFrom existential on the SAME property.
        iri_q("http://ex/C", RDFS_SUBCLASSOF, "http://ex/r2"),
        iri_q("http://ex/r2", RDF_TYPE, OWL_RESTRICTION),
        iri_q("http://ex/r2", OWL_ON_PROPERTY, "http://ex/p"),
        iri_q("http://ex/r2", OWL_SOME_VALUES_FROM, "http://ex/D"),
    ] {
        b.push_owned_quad(&q);
    }
    b.push_owned_quad(
        &RdfQuad::new(
            RdfTerm::iri("http://ex/r1"),
            OWL_MIN_CARDINALITY,
            RdfTerm::Literal(RdfLiteral::typed("1", XSD_NNI)),
        )
        .in_graph(RdfTerm::iri(W)),
    );
    let edb = b.freeze().expect("freeze the entangled edb");

    // This explicit synthetic operation selects each authored source context as
    // a logical world. The prepared ingress commitment binds that test authority.
    fn prepare(
        source: &impl DatasetView,
    ) -> (
        crate::reason::PreparedReasoningInput,
        crate::physical::SelectedDomains,
    ) {
        use crate::physical::{DomainProfile, LogicalGraph, SelectedDomains, SelectedLogicalWorld};
        let input = crate::reason::prepare_reasoning_input(source).unwrap();
        let worlds = input
            .source_contexts()
            .values()
            .map(|graph| {
                SelectedLogicalWorld::new(
                    LogicalGraph::from_graph(graph.clone()),
                    DomainProfile::NonemptyObjectDomainV1,
                    "urn:gmeow:test:entangled-class-theory".to_owned(),
                    *input.ingress_contract(),
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        (input, SelectedDomains::new(worlds).unwrap())
    }
    let (input, domains) = prepare(edb.as_ref());
    let result = crate::reason::reason_all(input, &domains).unwrap();
    result.validate_native_closure().unwrap();
    let execution = result.native_execution().unwrap();
    let ledger = execution
        .families
        .iter()
        .find(|ledger| ledger.world == W)
        .unwrap();
    let bound = ledger
        .outcomes
        .iter()
        .flat_map(|outcome| &outcome.bounds)
        .find(|bound| {
            bound.owner == purrdf::TermValue::iri("http://ex/r1")
                && bound.predicate == OWL_MIN_CARDINALITY
        })
        .expect("the exact selected min bound remains in the joint family outcome");
    assert_eq!(bound.kind, BoundKind::Min);
    assert_eq!(bound.interpreted, Some(1));
    assert_eq!(
        bound.value,
        purrdf::TermValue::Literal {
            lexical_form: "1".to_owned(),
            datatype: XSD_NNI.to_owned(),
            language: None,
            direction: None
        }
    );
    assert!(!bound.support.is_empty());
    let source = ledger.source_leaves(&bound.support).unwrap();
    assert!(source.iter().any(|row| row.subject == bound.owner
        && row.predicate == OWL_MIN_CARDINALITY
        && row.object == bound.value
        && row.graph == Some(purrdf::TermValue::iri(W))));
    let witness = execution
        .witness_derivations
        .iter()
        .find(|witness| {
            witness.scope.world == W
                && witness.heads.iter().any(|edge| {
                    edge.statement.subject == purrdf::TermValue::iri("http://ex/i")
                        && edge.statement.predicate == "http://ex/p"
                        && witness.heads.iter().any(|membership| {
                            membership.statement.subject == edge.statement.object
                                && membership.statement.predicate
                                    == "https://blackcatinformatics.ca/logic/instanceOf"
                                && membership.statement.object
                                    == purrdf::TermValue::iri("http://ex/D")
                        })
                })
        })
        .expect("the existing someValuesFrom obligation commits its property and class witness");
    witness.validate().unwrap();
    let edge = witness
        .heads
        .iter()
        .find(|head| {
            head.statement.subject == purrdf::TermValue::iri("http://ex/i")
                && head.statement.predicate == "http://ex/p"
        })
        .unwrap();
    assert!(!edge.premises.is_empty());
    assert!(!edge.derivation_id.is_empty());
    assert!(
        witness
            .heads
            .iter()
            .any(|head| head.statement.subject == edge.statement.object
                && head.statement.predicate == "https://blackcatinformatics.ca/logic/instanceOf"
                && head.statement.object == purrdf::TermValue::iri("http://ex/D"))
    );

    let (input, domains) = prepare(edb.as_ref());
    let limited = crate::reason::reason_all_budgeted(
        input,
        &domains,
        &crate::query_ir::Budget {
            max_steps: Some(0),
            max_answers: None,
        },
    )
    .unwrap();
    limited.validate_native_closure().unwrap();
    let limited_execution = limited.native_execution().unwrap();
    assert_eq!(limited_execution.frontier.consumed_steps, 0);
    assert_eq!(
        limited.completeness,
        crate::result::CompletenessStatus::Incomplete
    );
    assert_ne!(limited_execution.status, NativeClosureStatus::Completed);
    assert!(limited_execution.witness_derivations.is_empty());
    assert!(
        limited_execution
            .families
            .iter()
            .flat_map(|ledger| &ledger.outcomes)
            .all(|outcome| outcome
                .conclusions
                .iter()
                .all(|conclusion| conclusion.committed.is_none()))
    );
}

// ── No process references in the kernel registry itself (R: acceptance) ───────

/// The kernel registry's OWN technical strings — every `decided_fragments()`
/// completeness bound and every `retained_boundaries()` reason — are free of any
/// PROCESS REFERENCE (`#<digit>`, `issue`, a bare `PR` token, or `per #`,
/// case-insensitive). The conformance gate proves the same over the shipped
/// `module.ttl` projection; this pins the source registry directly, so a process
/// reference can enter neither the kernel nor its manifest.
#[test]
fn kernel_registry_strings_carry_no_process_reference() {
    fn process_reference(text: &str) -> Option<&'static str> {
        let lower = text.to_ascii_lowercase();
        let bytes = lower.as_bytes();
        for i in 0..bytes.len() {
            if bytes[i] == b'#' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit) {
                return Some("#<digit>");
            }
        }
        if lower.contains("issue") {
            return Some("issue");
        }
        if lower.contains("per #") {
            return Some("per #");
        }
        let is_word = |c: u8| c.is_ascii_alphanumeric();
        for i in 0..bytes.len().saturating_sub(1) {
            if bytes[i] == b'p'
                && bytes[i + 1] == b'r'
                && (i == 0 || !is_word(bytes[i - 1]))
                && (i + 2 >= bytes.len() || !is_word(bytes[i + 2]))
            {
                return Some("PR");
            }
        }
        None
    }

    let mut failures: Vec<String> = Vec::new();
    for f in decided_fragments() {
        if let Some(pat) = process_reference(f.bound) {
            failures.push(format!("decided fragment {:?} bound carries {pat:?}", f.id));
        }
    }
    for boundary in retained_boundaries() {
        if let Some(pat) = process_reference(boundary.reason) {
            failures.push(format!(
                "retained boundary {:?} reason carries {pat:?}",
                boundary.id
            ));
        }
    }
    for contract in source_admission_contracts() {
        if let Some(pat) = process_reference(contract.requirement) {
            failures.push(format!(
                "source admission {:?} requirement carries {pat:?}",
                contract.id
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "kernel registry technical strings must be free of process references:\n  • {}",
        failures.join("\n  • ")
    );
}

/// Every source cause has one typed classification shared by admission and reports.
#[test]
fn source_issue_classification_distinguishes_invalid_and_unsupported() {
    use RefutationSourceIssue::*;
    let invalid = [
        MalformedNil,
        ConflictingListField {
            subject: "urn:list:head".to_owned(),
            predicate: "urn:list:first".to_owned(),
        },
        CyclicList {
            owner: "urn:class:C".to_owned(),
            head: "urn:list:head".to_owned(),
        },
        IncompleteList {
            owner: "urn:class:C".to_owned(),
            head: "urn:list:head".to_owned(),
        },
    ];
    let unsupported = [
        ExpressionMultiplicity {
            subject: "urn:class:C".to_owned(),
            predicate: "urn:class:complement".to_owned(),
        },
        UnsupportedExpressionOwner {
            owner: purrdf::TermValue::Triple {
                s: Box::new(purrdf::TermValue::iri("urn:quoted:s")),
                p: Box::new(purrdf::TermValue::iri("urn:quoted:p")),
                o: Box::new(purrdf::TermValue::iri("urn:quoted:o")),
            },
            predicate: "urn:class:complement".to_owned(),
        },
        UnsupportedExpressionOperand {
            owner: "urn:class:C".to_owned(),
            predicate: "urn:class:complement".to_owned(),
            operand: purrdf::TermValue::Literal {
                lexical_form: "C".to_owned(),
                datatype: "http://www.w3.org/2001/XMLSchema#string".to_owned(),
                language: None,
                direction: None,
            },
        },
        UnsupportedListMember {
            owner: "urn:class:C".to_owned(),
            head: "urn:list:head".to_owned(),
        },
    ];
    for issue in invalid {
        assert_eq!(issue.refusal_class(), ClassSourceRefusal::Invalid);
    }
    for issue in unsupported {
        assert_eq!(issue.refusal_class(), ClassSourceRefusal::Unsupported);
    }
}

/// A malformed selected list fails the operation while a sibling unsupported
/// expression keeps its independent capability finding and exact source owner.
#[test]
fn combined_source_refusals_preserve_invalid_and_unsupported_grades() {
    use purrdf::{RdfLiteral, RdfQuad, RdfTerm, TermValue};
    const WORLD: &str = "urn:test:class-admission-world";
    const UNION: &str = "https://blackcatinformatics.ca/logic/unionOf";
    const COMPLEMENT: &str = "https://blackcatinformatics.ca/logic/complementOf";
    const FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
    let mut builder = RdfDatasetBuilder::new();
    for row in [
        RdfQuad::new(
            RdfTerm::iri("urn:class:union"),
            UNION,
            RdfTerm::iri("urn:list:head"),
        ),
        RdfQuad::new(
            RdfTerm::iri("urn:list:head"),
            FIRST,
            RdfTerm::iri("urn:class:A"),
        ),
        RdfQuad::new(
            RdfTerm::iri("urn:class:complement"),
            COMPLEMENT,
            RdfTerm::Literal(RdfLiteral::typed(
                "not a class resource",
                "http://www.w3.org/2001/XMLSchema#string",
            )),
        ),
    ] {
        builder.push_owned_quad(&row.in_graph(RdfTerm::iri(WORLD)));
    }
    let source = builder.freeze().unwrap();
    let prepared = PreparedClassAnalysis::new(source.as_ref()).unwrap();
    let admission = prepared.admission();
    assert_eq!(
        admission.refusal_class().unwrap(),
        Some(ClassSourceRefusal::Invalid)
    );
    let selected = &admission.selected_worlds[WORLD];
    assert_eq!(selected.graph, Some(TermValue::iri(WORLD)));
    assert_eq!(selected.definitions.len(), 2);
    let boundary = selected.refusal.as_ref().unwrap();
    let FragmentBoundary::Combined(causes) = boundary else {
        panic!("both selected source causes must remain: {boundary:?}")
    };
    assert_eq!(causes.len(), 2);
    let mut grades = BTreeSet::new();
    for cause in causes {
        let FragmentBoundary::SourceAdmission {
            world,
            premises,
            issue,
        } = cause
        else {
            panic!("source admission cannot invent semantic clashes")
        };
        assert_eq!(world, WORLD);
        assert!(!premises.is_empty());
        assert!(
            premises
                .iter()
                .all(|row| row.graph == Some(TermValue::iri(WORLD)))
        );
        let ledger = boundary_diag_ledger(cause);
        let findings = ledger.findings("reason");
        assert_eq!(findings.len(), 1);
        let finding = &findings[0];
        match issue.refusal_class() {
            ClassSourceRefusal::Invalid => {
                assert_eq!(finding.severity, Severity::Error);
                assert_eq!(
                    finding.category,
                    Some(FindingCategory::ModelingDisciplineViolation)
                );
                assert_eq!(ledger.verdict(), GateVerdict::Fatal);
                assert!(
                    premises
                        .iter()
                        .any(|row| row.subject == TermValue::iri("urn:class:union")
                            && row.predicate == UNION)
                );
                assert!(
                    premises
                        .iter()
                        .any(|row| row.subject == TermValue::iri("urn:list:head")
                            && row.predicate == FIRST)
                );
                grades.insert("invalid");
            }
            ClassSourceRefusal::Unsupported => {
                assert_eq!(finding.severity, Severity::Info);
                assert_eq!(
                    finding.category,
                    Some(FindingCategory::UnsupportedSemanticFeature)
                );
                assert_eq!(ledger.verdict(), GateVerdict::Collected);
                assert!(
                    premises
                        .iter()
                        .any(|row| row.subject == TermValue::iri("urn:class:complement")
                            && row.predicate == COMPLEMENT)
                );
                grades.insert("unsupported");
            }
        }
    }
    assert_eq!(grades, ["invalid", "unsupported"].into_iter().collect());
    let ledger = boundary_diag_ledger(boundary);
    assert_eq!(ledger.verdict(), GateVerdict::Fatal);
    let findings = ledger.findings("reason");
    assert_eq!(findings.len(), 2);
    assert_ne!(findings[0].finding_iri, findings[1].finding_iri);
    let nested = FragmentBoundary::Combined(
        [boundary.clone(), causes.iter().next().unwrap().clone()]
            .into_iter()
            .collect(),
    );
    assert_eq!(
        boundary_diag_ledger(&nested).findings("reason"),
        findings,
        "the ledger coalesces repeated identical causes without losing their independent grades"
    );
}
