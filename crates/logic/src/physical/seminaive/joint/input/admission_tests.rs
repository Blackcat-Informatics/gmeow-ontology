// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Closed witness-free relations: membership, writer refusal and exact closure.

use std::sync::Arc;

use purrdf::TermValue;

use super::*;
use crate::physical::seminaive::joint::input::JointTemplate;

const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const TRANSITIVE: &str = "http://www.w3.org/2002/07/owl#TransitiveProperty";
const OWL: &str = "http://www.w3.org/2002/07/owl#";
const WORLD: &str = "urn:world";
const SEMANTICS: SemanticVocabulary = SemanticVocabulary::GroundedLogicV1;

#[test]
fn inconclusive_refinement_retains_rule_and_counts_on_the_shared_diagnostic() {
    let gap = EvidenceGap::SpecializationBound {
        rule: "urn:selected-rule".into(),
        settled_matches: 17,
        distinct_specializations: 11,
        total_statements: 23,
    };
    let diagnostic = gmeow_errors::Diag::of_kind(gap.clone());
    assert_eq!(diagnostic.code(), crate::error::Physical::register());
    assert_eq!(diagnostic.grade().severity, gmeow_errors::Severity::Error);
    assert_eq!(diagnostic.downcast_ref::<EvidenceGap>(), Some(&gap));
    assert!(diagnostic.message().contains("urn:selected-rule (17 settled matches, 11 distinct specializations; 23 statements in total)"));
}

fn fact(subject: &str, predicate: &str, object: &str) -> Fact {
    Fact {
        subject: TermValue::iri(subject),
        predicate: predicate.to_owned(),
        object: TermValue::iri(object),
    }
}

#[test]
fn list_cell_bindings_match_reachability_for_every_three_node_graph() {
    let nodes = ["urn:list:a", "urn:list:b", "urn:list:c"];
    for edges in 0..(1usize << 9) {
        for members in 0..(1usize << 3) {
            let mut facts = Vec::new();
            for (i, node) in nodes.iter().enumerate() {
                if members & (1 << i) != 0 {
                    facts.push(fact(node, LIST_FIRST, "urn:member"));
                }
                for (j, tail) in nodes.iter().enumerate() {
                    if edges & (1 << (3 * i + j)) != 0 {
                        facts.push(fact(node, LIST_REST, tail));
                    }
                }
            }
            let facts = facts.iter().collect::<Vec<_>>();
            let mut rel = RelationStore::with_semantics(SEMANTICS);
            assert!(bind_list_cells(&mut rel, &facts, SEMANTICS));
            for (i, &head) in nodes.iter().enumerate() {
                // Independent Boolean transitive closure includes the zero-length
                // path and passes through nodes with no rdf:first of their own.
                let mut reachable = [false; 3];
                reachable[i] = true;
                for _ in 0..3 {
                    for from in 0..3 {
                        for to in 0..3 {
                            reachable[to] |= reachable[from] && edges & (1 << (3 * from + to)) != 0;
                        }
                    }
                }
                for (j, &cell) in nodes.iter().enumerate() {
                    assert_eq!(
                        rel.contains(LIST_CELL, &TermValue::iri(head), &TermValue::iri(cell)),
                        members & (1 << i) != 0 && members & (1 << j) != 0 && reachable[j],
                        "edges={edges}, members={members}, head={i}, cell={j}"
                    );
                }
            }
        }
    }
}

#[test]
fn list_cell_bindings_share_a_branching_rest_only_cycle() {
    let mut facts = vec![fact("urn:tail", LIST_FIRST, "urn:tail-member")];
    for head in 0..512 {
        let head = format!("urn:head:{head}");
        facts.push(fact(&head, LIST_FIRST, "urn:head-member"));
        facts.push(fact(&head, LIST_REST, "urn:rest:0"));
    }
    for cell in 0..1024 {
        for offset in [1, 7] {
            facts.push(fact(
                &format!("urn:rest:{cell}"),
                LIST_REST,
                &format!("urn:rest:{}", (cell + offset) % 1024),
            ));
        }
    }
    facts.push(fact("urn:rest:1023", LIST_REST, "urn:tail"));
    let facts = facts.iter().collect::<Vec<_>>();
    let mut rel = RelationStore::with_semantics(SEMANTICS);
    assert!(bind_list_cells(&mut rel, &facts, SEMANTICS));
    assert_eq!(rel.row_count(), 1025);
    for head in 0..512 {
        let head = TermValue::iri(format!("urn:head:{head}"));
        assert!(rel.contains(LIST_CELL, &head, &head));
        assert!(rel.contains(LIST_CELL, &head, &TermValue::iri("urn:tail")));
        assert!(!rel.contains(LIST_CELL, &head, &TermValue::iri("urn:rest:0")));
    }
}

#[test]
fn list_cell_bindings_do_not_follow_incoming_rest_only_components() {
    let mut facts = Vec::new();
    for head in 0..64 {
        facts.push(fact(&format!("urn:head:{head}"), LIST_FIRST, "urn:member"));
    }
    for cell in 0..512 {
        let subject = format!("urn:incoming:{cell}");
        facts.push(fact(
            &subject,
            LIST_REST,
            &format!("urn:incoming:{}", (cell + 1) % 512),
        ));
        facts.push(fact(
            &subject,
            LIST_REST,
            &format!("urn:head:{}", cell % 64),
        ));
    }
    let facts = facts.iter().collect::<Vec<_>>();
    let mut rel = RelationStore::with_semantics(SEMANTICS);
    assert!(bind_list_cells(&mut rel, &facts, SEMANTICS));
    assert_eq!(rel.row_count(), 64);
    for head in 0..64 {
        let head = TermValue::iri(format!("urn:head:{head}"));
        assert!(rel.contains(LIST_CELL, &head, &head));
    }
}

#[test]
fn list_cell_binding_limit_is_inclusive_and_publication_is_atomic() {
    for count in [512usize, 513] {
        let mut facts = Vec::new();
        for cell in 0..count {
            let cell_name = format!("urn:cell:{cell}");
            facts.push(fact(&cell_name, LIST_FIRST, "urn:member"));
            facts.push(fact(
                &cell_name,
                LIST_REST,
                &format!("urn:cell:{}", (cell + 1) % count),
            ));
        }
        let facts = facts.iter().collect::<Vec<_>>();
        let mut rel = RelationStore::with_semantics(SEMANTICS);
        rel.insert(
            "urn:existing",
            &TermValue::iri("urn:s"),
            &TermValue::iri("urn:o"),
        );
        let admitted = count * count <= MAX_BINDINGS;
        assert_eq!(bind_list_cells(&mut rel, &facts, SEMANTICS), admitted);
        assert_eq!(
            rel.row_count(),
            if admitted { 1 + count * count } else { 1 }
        );
        assert!(rel.contains(
            "urn:existing",
            &TermValue::iri("urn:s"),
            &TermValue::iri("urn:o")
        ));
    }
}

/// `R ⊑ ≥1 urn:property.class` with one member.
fn restriction(class: &str) -> Vec<Fact> {
    vec![
        fact(
            "urn:restriction",
            &format!("{OWL}onProperty"),
            "urn:property",
        ),
        Fact {
            subject: TermValue::iri("urn:restriction"),
            predicate: format!("{OWL}minQualifiedCardinality"),
            object: TermValue::simple_literal("1"),
        },
        fact("urn:restriction", &format!("{OWL}onClass"), class),
        fact("urn:individual", TYPE, "urn:restriction"),
    ]
}

fn template(rules: &[EvalRule]) -> Arc<JointTemplate> {
    Arc::new(
        JointTemplate::with_sources(
            rules,
            &[],
            crate::reason::schema_laws(),
            SEMANTICS,
            &[],
            None,
            &crate::physical::SelectedDomains::new([]).unwrap(),
        )
        .unwrap(),
    )
}

/// The closed relations of one input and their computed extension.
fn closed(rules: &[EvalRule], rows: Vec<Fact>) -> (Closure, RelationStore) {
    let template = template(rules);
    let data = BTreeMap::from([(WORLD.to_owned(), rows)]);
    let input = template.input(&data, Arc::from([]), &[]).unwrap();
    let Some(Ok(evidence)) = &input.evidence else {
        panic!("no input-specific evidence was observed");
    };
    let closable = evidence.closable.as_ref().expect("candidates within bound");
    let (closure, store, _) = template
        .termination
        .as_ref()
        .expect("a native termination template")
        .close(evidence, closable, SEMANTICS)
        .unwrap()
        .expect("closure within bound");
    (closure, store)
}

fn transitive_marker() -> (String, String) {
    (
        SEMANTICS.predicate(TYPE).to_owned(),
        SEMANTICS.analysis_symbol(TRANSITIVE).to_owned(),
    )
}

#[test]
fn a_marker_no_writer_can_produce_is_closed() {
    // Every `instanceOf` writer takes its class from a settled column (onClass,
    // domain, onProperty, ...) that never holds the transitive marker, so the pair
    // `(?, instanceOf, owl:TransitiveProperty)` is closed with its source rows.
    let mut rows = restriction("urn:witness-class");
    rows.push(fact("urn:chain", TYPE, TRANSITIVE));
    let (closure, store) = closed(&[], rows);
    assert!(
        closure.pairs.contains(&transitive_marker()),
        "{:?}",
        closure.pairs
    );
    assert!(store.contains(
        TYPE,
        &TermValue::iri("urn:chain"),
        &TermValue::iri(TRANSITIVE)
    ));
}

#[test]
fn a_marker_a_witness_may_carry_is_open() {
    // `R ⊑ ≥1 p.owl:TransitiveProperty` types every witness with the marker. The
    // minimum family mints witnesses, so it is never an eligible writer: the pair
    // must stay open, and the transitive law keeps its variable predicate.
    let mut rows = restriction(TRANSITIVE);
    rows.push(fact("urn:chain", TYPE, TRANSITIVE));
    let (closure, _) = closed(&[], rows);
    assert!(
        !closure.pairs.contains(&transitive_marker()),
        "{:?}",
        closure.pairs
    );
}

#[test]
fn the_closure_of_a_witness_free_relation_is_complete() {
    // EL subclass transitivity writes `subClassOf` from `subClassOf` alone: the
    // relation is closed, and its extension holds the derived `a ⊑ c`.
    let mut rows = restriction("urn:witness-class");
    rows.push(fact("urn:a", SUBCLASS, "urn:b"));
    rows.push(fact("urn:b", SUBCLASS, "urn:c"));
    let (closure, store) = closed(&crate::reason::el::structured_el_rules(), rows);
    assert!(
        closure.predicates.contains(SEMANTICS.predicate(SUBCLASS)),
        "{:?}",
        closure.predicates
    );
    assert!(store.contains(SUBCLASS, &TermValue::iri("urn:a"), &TermValue::iri("urn:c")));
}

#[test]
fn a_relation_written_from_an_open_relation_is_open() {
    // `type(x, Meta) → subClassOf(x, Top)` writes `subClassOf` from `instanceOf
    // Meta`, which the witnesses of `R ⊑ ≥1 p.Meta` enter: a witness would become a
    // subclass. That extension is no source closure, so `subClassOf` must stay
    // open. Without the promoter the same rows close it, so the refusal is not
    // vacuous.
    let promote = EvalRule::positive(
        "urn:rule:meta-subclass",
        EvalAtom::positive(EvalTerm::var("?x"), SUBCLASS, EvalTerm::named("urn:Top")),
        vec![EvalAtom::positive(
            EvalTerm::var("?x"),
            TYPE,
            EvalTerm::named("urn:Meta"),
        )],
    );
    let mut rows = restriction("urn:Meta");
    rows.push(fact("urn:a", SUBCLASS, "urn:b"));
    rows.push(fact("urn:b", SUBCLASS, "urn:c"));
    let subclass = SEMANTICS.predicate(SUBCLASS);
    let el = crate::reason::el::structured_el_rules();
    let (control, _) = closed(&el, rows.clone());
    assert!(
        control.predicates.contains(subclass),
        "{:?}",
        control.predicates
    );
    let mut rules = el;
    rules.push(promote);
    let (closure, _) = closed(&rules, rows);
    assert!(
        !closure.predicates.contains(subclass),
        "{:?}",
        closure.predicates
    );
}
