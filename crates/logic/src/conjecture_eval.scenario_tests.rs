// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use purrdf::{DatasetView, TermRef, TermValue};

use super::{CONJECTURE_SCENARIO_WORLD, rehome_kb_into_scenario};

#[test]
fn scenario_retains_statement_evidence_and_replaces_source_graph_ownership() {
    let view = rehome_kb_into_scenario(
        r#"
            @prefix logic: <https://blackcatinformatics.ca/logic/> .
            @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
            <urn:conjecture:source-world> {
                _:subject logic:subClassOf _:parent .
                _:claim rdf:reifies <<( _:subject logic:subClassOf _:parent )>> ;
                    logic:standpoint <urn:conjecture:alice> ;
                    logic:provenance <urn:conjecture:source-world> .
            }
            <urn:conjecture:empty-source-world> {}
            "#,
        "application/trig",
    )
    .expect("place the caller's scenario with its statement evidence");
    let world = view
        .term_id_by_value(&TermValue::iri(CONJECTURE_SCENARIO_WORLD))
        .expect("the scenario graph is declared");
    assert_eq!(view.named_graphs().collect::<Vec<_>>(), [world]);
    let assertions: Vec<_> = view.quads().collect();
    let bindings: Vec<_> = view.reifier_quads().collect();
    let annotations: Vec<_> = view.annotation_quads().collect();
    assert_eq!(assertions.len(), 1);
    assert_eq!(bindings.len(), 1);
    assert_eq!(annotations.len(), 2);
    assert!(
        assertions
            .iter()
            .chain(&bindings)
            .chain(&annotations)
            .all(|row| row.g == Some(world)),
        "every statement layer belongs to the selected conjecture scenario"
    );
    let assertion = assertions[0];
    let binding = bindings[0];
    assert!(matches!(
        view.resolve(binding.o),
        TermRef::Triple { s, p, o }
            if (s, p, o) == (assertion.s, assertion.p, assertion.o)
    ));
    assert_ne!(
        assertion.s, assertion.o,
        "distinct source blanks stay distinct"
    );
    assert!(annotations.iter().all(|row| row.s == binding.s));
    for (predicate, object) in [
        ("standpoint", "urn:conjecture:alice"),
        ("provenance", "urn:conjecture:source-world"),
    ] {
        let predicate = view
            .term_id_by_value(&TermValue::iri(format!(
                "https://blackcatinformatics.ca/logic/{predicate}"
            )))
            .unwrap();
        let object = view.term_id_by_value(&TermValue::iri(object)).unwrap();
        assert!(
            annotations
                .iter()
                .any(|row| row.p == predicate && row.o == object)
        );
    }
    let source = view.sources()[0]
        .dataset()
        .expect("retain the parsed native source");
    let original = source.quads().next().unwrap();
    assert!(matches!(
        (source.resolve(original.s), view.resolve(assertion.s)),
        (TermRef::Blank { label: a, scope: x }, TermRef::Blank { label: b, scope: y })
            if a == b && x == y
    ));
    assert!(
        source.named_graphs().any(|graph| matches!(
            source.resolve(graph),
            TermRef::Iri("urn:conjecture:empty-source-world")
        )),
        "the retained input still owns its original declaration"
    );
    assert_eq!(view.stats().work.materializations, 0);
    assert_eq!(view.stats().work.copied_rows, 0);
}

#[test]
fn empty_scenario_is_declared_without_retaining_departed_graphs() {
    for source in ["", "<urn:conjecture:empty-source-world> {}"] {
        let view = rehome_kb_into_scenario(source, "application/trig")
            .expect("an empty caller KB still selects the scenario world");
        let world = view
            .term_id_by_value(&TermValue::iri(CONJECTURE_SCENARIO_WORLD))
            .expect("empty scenario declaration");
        assert_eq!(view.named_graphs().collect::<Vec<_>>(), [world]);
        assert_eq!(view.quads().count(), 0);
        assert_eq!(view.reifier_quads().count(), 0);
        assert_eq!(view.annotation_quads().count(), 0);
        assert_eq!(view.stats().work.materializations, 0);
    }
}
