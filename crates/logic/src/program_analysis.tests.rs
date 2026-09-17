// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_logic_compile::frontend::parse_logic_str;

/// A real typed rule input, independently parsed for cache identity checks.
fn program() -> LogicProgram {
    parse_logic_str(
            "@prefix logic: <https://blackcatinformatics.ca/logic/> .
             @prefix ex: <https://example.test/> .
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
             ex:rule logic:instanceOf logic:Rule ;
                 logic:head [ rdf:subject \"?x\" ; rdf:predicate ex:derived ; rdf:object \"?y\" ] ;
                 logic:body [ rdf:subject \"?x\" ; rdf:predicate ex:asserted ; rdf:object \"?y\" ] .",
            None,
        )
        .expect("synthetic rule parses")
        .0
}

/// Repeated lowering of the same canonical program returns the same immutable IR;
/// changing contextual provenance must not reuse its predecessor's analysis.
/// This verifies cache identity, not evaluation of a scoped rule.
#[test]
fn preparation_reuses_ir_and_distinguishes_complete_context() {
    let program = program();
    assert_eq!(program.rules.len(), 1);
    let first = prepare_program(&program).expect("prepare");
    let again = prepare_program(&program).expect("reuse");
    assert!(Arc::ptr_eq(&first, &again));
    assert_eq!(first.rules.len(), 1);
    let facts = std::collections::BTreeMap::new();
    let sources = crate::reason::source_existentials::Collector::default()
        .prepare()
        .unwrap();
    let first_input = first
        .reasoning_input(
            &[],
            &sources,
            &crate::physical::SelectedDomains::new([]).unwrap(),
            &facts,
            Arc::from([]),
            &[],
        )
        .unwrap();
    let again_input = again
        .reasoning_input(
            &[],
            &sources,
            &crate::physical::SelectedDomains::new([]).unwrap(),
            &facts,
            Arc::from([]),
            &[],
        )
        .unwrap();
    let NativeOutcome::Decided(reasoning) = first.reasoning_schema_program(&first_input).unwrap()
    else {
        panic!("finite reasoning program must be admitted");
    };
    let NativeOutcome::Decided(reused_reasoning) =
        again.reasoning_schema_program(&again_input).unwrap()
    else {
        panic!("cached reasoning program must retain admission");
    };
    assert!(Arc::ptr_eq(&reasoning, &reused_reasoning));
    let NativeOutcome::Decided(materialization) = first.joint_program().unwrap() else {
        panic!("finite materialization program must be admitted");
    };
    assert!(
        !Arc::ptr_eq(&reasoning, &materialization),
        "fixed DL rules belong only to the reasoning plan"
    );
    assert_eq!(
        first.rules[0].head.predicate,
        "https://example.test/derived"
    );
    let mut changed = program.clone();
    changed.rules[0].scope.standpoint = Some("https://example.test/standpoint".into());
    let other = prepare_program(&changed).expect("changed context");
    assert!(!Arc::ptr_eq(&first, &other));
    changed.rules[0].scope.time = Some("2026-09-08".into());
    assert!(!Arc::ptr_eq(
        &other,
        &prepare_program(&changed).expect("changed time")
    ));
}

/// Unsupported formulas retain their exact boundary on a hit; absence of a lowered
/// head must never erase the reason no negative proof is available.
#[test]
fn cache_hit_preserves_formula_residue() {
    use gmeow_logic_compile::ir::{Formula, Term};
    let mut program = program();
    let atom = |predicate: &str| {
        Formula::atom(
            Term::iri(predicate.to_owned()).expect("predicate IRI"),
            vec![
                Term::var("x").expect("variable"),
                Term::var("y").expect("variable"),
            ],
        )
        .expect("binary atom")
    };
    program
        .formulas
        .push(Formula::Or(vec![atom("urn:a"), atom("urn:b")]));
    let first = prepare_program(&program).expect("prepare residue");
    assert!(!first.preservation.unsupported_constructs.is_empty());
    let again = prepare_program(&program).expect("reuse residue");
    assert!(Arc::ptr_eq(&first, &again));
    assert_eq!(first.preservation, again.preservation);
}

/// Cache eviction releases only its ownership; in-flight consumers retain usable
/// immutable preparations, and oversized payloads are never retained.
#[test]
fn cache_has_bounded_retention_without_invalidating_live_consumers() {
    let program = program();
    let retained = Arc::new(PreparedProgram::lower(&program).expect("lower"));
    let mut cache = PreparationCache::default();
    for index in 0..=MAX_ENTRIES {
        let mut key = [0; 32];
        key[..8].copy_from_slice(&(index as u64).to_le_bytes());
        cache.insert(key, Arc::clone(&retained));
    }
    assert_eq!(cache.entries.len(), MAX_ENTRIES);
    assert!(cache.get(&[0; 32]).is_none());
    assert_eq!(retained.rules.len(), 1);
    let mut oversized = PreparedProgram::lower(&program).expect("lower large");
    oversized.rules[0].rule_iri = "x".repeat(MAX_RENDERED_BYTES + 1);
    assert!(!oversized.cacheable());
}

/// Large source metadata is hashed without retention, so it does not prevent
/// reuse of a small executable. Only the data actually held by the cache is bounded.
#[test]
fn large_source_identity_does_not_evict_a_small_preparation() {
    let mut program = program();
    program.source_iri = Some(format!("urn:source:{}", "x".repeat(300 * 1024)));
    let first = prepare_program(&program).expect("prepare large source identity");
    let again = prepare_program(&program).expect("reuse small lowered IR");
    assert!(Arc::ptr_eq(&first, &again));
    assert!(first.cacheable());
}
