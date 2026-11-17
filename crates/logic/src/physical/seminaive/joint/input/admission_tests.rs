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
            assert!(bind_list_cells(&mut rel, &facts, SEMANTICS).is_ok());
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
    assert!(bind_list_cells(&mut rel, &facts, SEMANTICS).is_ok());
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
    assert!(bind_list_cells(&mut rel, &facts, SEMANTICS).is_ok());
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
        assert_eq!(
            bind_list_cells(&mut rel, &facts, SEMANTICS).is_ok(),
            admitted
        );
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

/// Protects the admission closure's pending-row ceiling in both its initial pass
/// and a semi-naive round. Two rules queue the same output for every Cartesian
/// match: only one distinct row would survive the drain, so bounding distinct
/// committed rows cannot protect this allocation. Exactly 2^18 candidates fit;
/// the next candidate must stop the analysis without being retained.
#[test]
fn closure_pending_rows_are_bounded_before_the_drain_in_both_passes() {
    const LEFT: &str = "urn:left";
    const RIGHT: &str = "urn:right";
    const READY: &str = "urn:ready";
    for delayed in [false, true] {
        for right_count in [256, 257] {
            let mut rows: Vec<_> = (0..512)
                .map(|index| fact(&format!("urn:left:{index}"), LEFT, "urn:input"))
                .collect();
            rows.extend(
                (0..right_count)
                    .map(|index| fact(&format!("urn:right:{index}"), RIGHT, "urn:input")),
            );
            let mut body = vec![
                EvalAtom::positive(EvalTerm::var("?left"), LEFT, EvalTerm::named("urn:input")),
                EvalAtom::positive(EvalTerm::var("?right"), RIGHT, EvalTerm::named("urn:input")),
            ];
            let mut rules = Vec::new();
            if delayed {
                rules.push(EvalRule::positive(
                    "urn:seed",
                    EvalAtom::positive(
                        EvalTerm::named("urn:seed"),
                        READY,
                        EvalTerm::named("urn:ready"),
                    ),
                    vec![EvalAtom::positive(
                        EvalTerm::named("urn:left:0"),
                        LEFT,
                        EvalTerm::named("urn:input"),
                    )],
                ));
                body.insert(
                    0,
                    EvalAtom::positive(
                        EvalTerm::named("urn:seed"),
                        READY,
                        EvalTerm::named("urn:ready"),
                    ),
                );
            }
            for name in ["urn:expand:a", "urn:expand:b"] {
                rules.push(EvalRule::positive(
                    name,
                    EvalAtom::positive(
                        EvalTerm::named("urn:output"),
                        LEFT,
                        EvalTerm::named("urn:output"),
                    ),
                    body.clone(),
                ));
            }
            let template = template(&rules);
            let data = BTreeMap::from([(WORLD.to_owned(), rows)]);
            let input = template
                .input(&data, Arc::from([]), &[])
                .expect("synthetic source and producer selection is valid");
            let Some(Ok(evidence)) = &input.evidence else {
                panic!("the bounded source inventory supplies input-specific evidence");
            };
            let result = template
                .termination
                .as_ref()
                .expect("native rules have a termination template")
                .close(
                    evidence,
                    evidence
                        .closable
                        .as_ref()
                        .expect("the small source inventory is within its own bound"),
                    SEMANTICS,
                )
                .expect("closure analysis has no operational failure");
            if right_count == 256 {
                let Ok((_, store, _)) = result else {
                    panic!("exactly {MAX_BINDINGS} pending rows fit the inclusive bound");
                };
                assert!(store.contains(
                    LEFT,
                    &TermValue::iri("urn:output"),
                    &TermValue::iri("urn:output")
                ));
            } else {
                let Err(EvidenceGap::ClosurePendingBound {
                    rule,
                    committed_rows,
                    pending_rows,
                }) = result
                else {
                    panic!("the first candidate past the pending bound must be named");
                };
                assert_eq!(rule, "urn:expand:b");
                assert_eq!(committed_rows, usize::from(delayed));
                assert_eq!(pending_rows, MAX_BINDINGS);
            }
        }
    }
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

fn cell_fact(index: usize) -> Fact {
    Fact {
        subject: TermValue::iri(format!("urn:cell:s{index}")),
        predicate: "urn:cell:p".to_owned(),
        object: TermValue::iri(format!("urn:cell:o{index}")),
    }
}

#[test]
fn interning_cut_measures_the_cells_and_the_rows_interned() {
    let flow = ValueFlow::new(&[], &[], SEMANTICS);
    let own = flow.with_source_constants(std::iter::empty(), usize::MAX);
    assert!(own.is_ok());
    let facts: Vec<_> = (0..10).map(cell_fact).collect();
    // The bound is checked after every term. The flow holds one cell, the first fact adds
    // three, each later fact two (the predicate is shared): 4, 6 and 8 cells after the
    // first three facts, then the fourth fact's subject makes 9 and passes the bound.
    let cut = flow
        .with_source_constants(facts.iter(), 8)
        .expect_err("ten facts pass an eight-cell bound");
    assert_eq!((cut.rows_interned, cut.cells), (4, 9), "{cut:?}");
    assert!(cut.cells > 8 && cut.cells > cut.flow_cells, "{cut:?}");
    // A bound already passed by the cells the flow holds interns no supplied row at all.
    let own_cells = cut.flow_cells;
    let cut = flow
        .with_source_constants(facts.iter(), own_cells - 1)
        .expect_err("the flow's own cells pass the bound");
    assert_eq!((cut.rows_interned, cut.cells), (0, own_cells), "{cut:?}");
}

#[test]
fn the_interning_note_states_cells_and_inputs_not_a_row_count_as_cells() {
    let cut = InterningCut {
        flow_cells: 600,
        cells: 600,
        rows_interned: 0,
    };
    let note = interning_note(&cut, false, 7, 3);
    assert!(
        note.contains("512-value-cell interning bound at 600 cells"),
        "{note}"
    );
    assert!(
        note.contains("600 were already held by the flow before any supplied row"),
        "{note}"
    );
    assert!(note.contains("the interner stopped before beginning any of the 10 rows supplied (7 proved-immutable rows, then 3 possible fact(s))"), "{note}");
    let note = interning_note(&cut, true, 7, 0);
    assert!(note.contains("7 selector rows, then 0 possible"), "{note}");
}

#[test]
fn the_enrichment_rows_note_states_the_measured_rows_and_bytes() {
    let note = enrichment_rows_note(5000, 123_456);
    assert!(
        note.contains("5000 selector rows (123456 bytes)")
            && note.contains("4096-row / 1048576-byte bound"),
        "{note}"
    );
}

/// Protects the interning note's row count: it states the rows handed to the interner (every
/// selector row when all were kept), not the smaller proved-immutable subset. `urn:property
/// rdfs:range urn:restriction` with `restriction ⊑ ≥1 property` is a genuine cycle, refused
/// whatever the member count; 600 members make the value universe pass its cell bound before
/// any supplied row is interned, so that refusal also carries a note reporting that the interner
/// stopped before beginning any of the rows supplied.
#[test]
fn a_refusal_counts_the_selector_rows_supplied_to_the_interner() {
    let mut rows = restriction("urn:restriction");
    rows.push(fact(
        "urn:property",
        "http://www.w3.org/2000/01/rdf-schema#range",
        "urn:restriction",
    ));
    rows.extend(
        (0..600).map(|index| fact(&format!("urn:member:{index}"), TYPE, "urn:restriction")),
    );
    let template = template(&[]);
    let data = BTreeMap::from([(WORLD.to_owned(), rows)]);
    let input = template.input(&data, Arc::from([]), &[]).unwrap();
    let Some(Ok(evidence)) = &input.evidence else {
        panic!("no input-specific evidence was observed");
    };
    let supplied = evidence
        .enrichment
        .as_ref()
        .expect("every selector row is within the enrichment bound")
        .len();
    assert!(
        supplied > evidence.facts.len(),
        "the rows supplied ({supplied}) must differ from the immutable subset ({})",
        evidence.facts.len()
    );
    let crate::physical::seminaive::NativeOutcome::Decided(plan) = input.prepare().unwrap() else {
        panic!("a decided refusal")
    };
    let ChaseAdmission::Uncertified { bounds, violations } = &plan.admission else {
        panic!(
            "a cyclic range restriction is refused: {:?}",
            plan.admission
        )
    };
    let note = bounds
        .iter()
        .chain(violations.iter())
        .find(|note| note.contains("interning bound"))
        .unwrap_or_else(|| panic!("no interning note: {bounds:?} {violations:?}"));
    assert!(
        note.contains(&format!(
            "the interner stopped before beginning any of the {supplied} rows supplied ({supplied} selector rows, then 0 possible fact(s))"
        )),
        "{note}"
    );
}

/// The analysis-bound notes the settled first pass attaches to the certificate of `rows`.
fn settled_notes(rows: Vec<Fact>) -> Vec<String> {
    let template = template(&[]);
    let data = BTreeMap::from([(WORLD.to_owned(), rows)]);
    let input = template.input(&data, Arc::from([]), &[]).unwrap();
    let Some(Ok(evidence)) = &input.evidence else {
        panic!("no input-specific evidence was observed");
    };
    template
        .termination
        .as_ref()
        .expect("a native termination template")
        .certify_settled(
            evidence,
            input.facts,
            &input.possible,
            &input.contextual_effects,
            SEMANTICS,
        )
        .unwrap()
        .expect("the settled pass completes")
        .notes
}

/// One list of `cells` members, `urn:cell:0 .. urn:cell:(cells - 1)`, ending in `rdf:nil`.
/// Each cell reads itself and every later cell, so the head/cell association count is
/// `cells * (cells + 1) / 2`.
fn chain_rows(cells: usize) -> Vec<Fact> {
    let mut rows = restriction("urn:witness-class");
    for cell in 0..cells {
        let name = format!("urn:cell:{cell}");
        rows.push(fact(&name, LIST_FIRST, "urn:member"));
        let next = if cell + 1 == cells {
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil".to_owned()
        } else {
            format!("urn:cell:{}", cell + 1)
        };
        rows.push(fact(&name, LIST_REST, &next));
    }
    rows
}

/// Protects the report of the list-cell binding bound: when binding every list head to the
/// cells of its own list reaches its association bound, the member cells are left
/// unconstrained, which coarsens the analysis, so the settled pass names that bound and the
/// association count measured where it stopped. The bound is 2^18 = 262,144 associations; a
/// 724-cell chain crosses it at its last cell (262,450), a 723-cell chain stays under
/// (261,726) and carries no note.
#[test]
fn a_list_cell_binding_past_its_bound_is_named_with_the_measured_count() {
    let under = settled_notes(chain_rows(723));
    assert!(
        !under.iter().any(|note| note.contains("list member cells")),
        "{under:?}"
    );

    let over = settled_notes(chain_rows(724));
    let note = over
        .iter()
        .find(|note| note.contains("list member cells"))
        .unwrap_or_else(|| panic!("no list-cell note: {over:?}"));
    assert!(
        note.contains(
            "stopped at 262450 head/cell associations, past its 262144-association bound"
        ),
        "{note}"
    );
}
