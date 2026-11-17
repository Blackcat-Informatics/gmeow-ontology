// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Source edits replace contextual preparation atomically, without an RDF rebuild.

use super::*;

const SOURCE: &str = r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix ex: <urn:prepared-context:> .
ex:context a logic:AttributedContext ; logic:contextWorld ex:world ;
    logic:contextStandpoint ex:standpoint ; logic:evidenceClosure logic:OpenWorldClosure .
ex:request a logic:ContextualEvaluationRequest ; logic:queryContext ex:context ; logic:queryFormula ex:formula .
ex:formula a logic:Formula ; logic:relation ex:predicate ;
    logic:argument [ logic:termIndex 0 ; logic:termIri ex:item ],
                   [ logic:termIndex 1 ; logic:termIri ex:value ] .
ex:item ex:predicate ex:value .
"#;

/// Source edits are atomic and preserve distinct original assertions even when
/// their blank and skolem subjects share one executable fact.
#[test]
fn contextual_preparation_is_shared_until_a_successful_source_edit() {
    let dataset = purrdf::parse_dataset(SOURCE.as_bytes(), "text/turtle", None).unwrap();
    let input = prepare_reasoning_input(dataset.as_ref()).unwrap();
    let mut edited = input.clone();
    assert!(Arc::ptr_eq(&input.contextual, &edited.contextual));
    let conflict = Fact {
        subject: TermValue::iri("urn:prepared-context:request"),
        predicate: "https://blackcatinformatics.ca/logic/queryContext".into(),
        object: TermValue::iri("urn:prepared-context:missing"),
    };
    assert!(
        edited
            .assert_fact(crate::physical::LogicalGraph::Default, conflict)
            .is_err()
    );
    assert_eq!(edited.ingress_contract(), input.ingress_contract());
    assert_eq!(edited.occurrences, input.occurrences);
    assert_eq!(edited.facts, input.facts);
    assert!(Arc::ptr_eq(&input.contextual, &edited.contextual));

    let required = super::super::LeaveOneOutAxiom {
        subject: "urn:prepared-context:request".into(),
        predicate: "https://blackcatinformatics.ca/logic/queryContext".into(),
        object: "urn:prepared-context:context".into(),
    };
    assert!(edited.retract_axiom(&required).is_err());
    assert_eq!(edited.ingress_contract(), input.ingress_contract());
    assert_eq!(edited.occurrences, input.occurrences);
    assert!(Arc::ptr_eq(&input.contextual, &edited.contextual));

    let ordinary = super::super::LeaveOneOutAxiom {
        subject: "urn:prepared-context:item".into(),
        predicate: "urn:prepared-context:predicate".into(),
        object: "urn:prepared-context:value".into(),
    };
    assert!(edited.retract_axiom(&ordinary).unwrap());
    assert_ne!(edited.ingress_contract(), input.ingress_contract());
    assert!(!Arc::ptr_eq(&input.contextual, &edited.contextual));
    let replacement = Arc::clone(&edited.contextual);
    assert!(!edited.retract_axiom(&ordinary).unwrap());
    assert!(Arc::ptr_eq(&replacement, &edited.contextual));

    let world = crate::reason::rl::DEFAULT_WORLD;
    let blank = input.occurrences[world]
        .iter()
        .find(|source| matches!(source.subject, TermValue::Blank { .. }))
        .expect("the contextual formula has blank argument nodes")
        .subject
        .clone();
    let original = Fact {
        subject: blank,
        predicate: "urn:prepared-context:evidence".to_owned(),
        object: TermValue::iri("urn:prepared-context:value"),
    };
    let executable = Fact {
        subject: crate::facts::skolemize(&original.subject).into_owned(),
        ..original.clone()
    };
    let source_count = edited.occurrences[world].len();
    let fact_count = edited.facts[world].len();
    let graph = crate::physical::LogicalGraph::Default;
    assert!(
        edited
            .assert_facts(&graph, [original.clone(), executable.clone()])
            .unwrap()
    );
    assert_eq!(edited.occurrences[world].len(), source_count + 2);
    assert_eq!(edited.facts[world].len(), fact_count + 1);
    assert!(edited.facts[world].contains(&executable));
    let contract = *edited.ingress_contract();
    let replacement = Arc::clone(&edited.contextual);
    assert!(!edited.assert_facts(&graph, [original, executable]).unwrap());
    assert_eq!(*edited.ingress_contract(), contract);
    assert!(Arc::ptr_eq(&replacement, &edited.contextual));
}
