// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_logic_compile::frontend::parse_logic_str;

/// Observe the actual cache-hit event without timing concurrent requests. A
/// subscriber must be called after the template-cache lock has been released.
struct TemplateCacheObserver {
    program: Arc<PreparedProgram>,
    hits: Arc<std::sync::atomic::AtomicUsize>,
}

impl tracing::Subscriber for TemplateCacheObserver {
    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
        metadata.is_event() && metadata.target() == "pipeline_reasoning_detail"
    }

    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }

    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}

    fn event(&self, event: &tracing::Event<'_>) {
        struct CacheHit(bool);
        impl tracing::field::Visit for CacheHit {
            fn record_debug(&mut self, _: &tracing::field::Field, _: &dyn std::fmt::Debug) {}

            fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
                if field.name() == "phase" && value == "reuse-native-reasoning-template" {
                    self.0 = true;
                }
            }
        }
        let mut hit = CacheHit(false);
        event.record(&mut hit);
        if hit.0 {
            assert!(
                self.program.schema_templates.try_lock().is_ok(),
                "cached input preparation still holds the shared template-cache lock"
            );
            self.hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

#[test]
fn cached_native_input_releases_template_cache_before_observation_and_preparation() {
    let prepared = prepare_program(&program()).unwrap();
    let sources = crate::reason::source_existentials::Collector::default()
        .prepare()
        .unwrap();
    let domains = crate::physical::SelectedDomains::new([]).unwrap();
    // Repeat one synthetic row to reach the existing large-input event boundary
    // without introducing a corpus or a large abstract value domain.
    let facts = std::collections::BTreeMap::from([(
        "urn:world".to_owned(),
        vec![
            crate::rule_ir::Fact {
                subject: purrdf::TermValue::iri("urn:s"),
                predicate: "https://example.test/asserted".to_owned(),
                object: purrdf::TermValue::iri("urn:o"),
            };
            100_000
        ],
    )]);
    let first = prepared
        .reasoning_input(&[], &sources, &domains, &facts, Arc::from([]), &[])
        .unwrap();
    let hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observer = TemplateCacheObserver {
        program: Arc::clone(&prepared),
        hits: Arc::clone(&hits),
    };
    let again = tracing::subscriber::with_default(observer, || {
        prepared.reasoning_input(&[], &sources, &domains, &facts, Arc::from([]), &[])
    })
    .unwrap();
    assert_eq!(hits.load(std::sync::atomic::Ordering::Relaxed), 1);
    assert_eq!(first.identity(), again.identity());
}

/// A real typed rule input, independently parsed for cache identity checks.
fn program() -> LogicProgram {
    parse_logic_str(
            r#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
             @prefix ex: <https://example.test/> .
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
             _:rulec982f48bS3_x a logic:TermCarrier ; logic:termVariable "x" .
_:rulec982f48bS3_y a logic:TermCarrier ; logic:termVariable "y" .
ex:rule logic:instanceOf logic:Rule ;
                 logic:head [ rdf:reifies <<( _:rulec982f48bS3_x ex:derived _:rulec982f48bS3_y )>> ] ;
                 logic:body [ rdf:reifies <<( _:rulec982f48bS3_x ex:asserted _:rulec982f48bS3_y )>> ] ."#,
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

#[test]
fn native_input_binds_axiom_content_even_when_executable_rules_are_identical() {
    use gmeow_logic_compile::ir::{Formula, Term};

    let with_axiom = |value| {
        let mut program = program();
        program.axioms.push(
            Formula::atom(
                Term::iri("urn:metadata").unwrap(),
                vec![
                    Term::iri("urn:source").unwrap(),
                    Term::literal(value, None).unwrap(),
                ],
            )
            .unwrap()
            .as_horn_axiom()
            .unwrap(),
        );
        program
    };
    let first = prepare_program(&with_axiom("first")).unwrap();
    let other = prepare_program(&with_axiom("other")).unwrap();
    assert_eq!(first.rules.len(), other.rules.len());
    assert_eq!(first.rules[0].head, other.rules[0].head);
    assert_ne!(first.identity, other.identity);
    assert_eq!(first.publication().identity, first.identity);
    let facts = std::collections::BTreeMap::new();
    let sources = crate::reason::source_existentials::Collector::default()
        .prepare()
        .unwrap();
    let domains = crate::physical::SelectedDomains::new([]).unwrap();
    let input = |prepared: &PreparedProgram| {
        prepared
            .reasoning_input(&[], &sources, &domains, &facts, Arc::from([]), &[])
            .unwrap()
    };
    assert_ne!(input(&first).identity(), input(&other).identity());
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
    let retained = prepare_program(&program).expect("lower");
    let mut cache = PreparationCache::default();
    for index in 0..=MAX_ENTRIES {
        let mut key = [0; 32];
        key[..8].copy_from_slice(&(index as u64).to_le_bytes());
        cache.insert(key, Arc::clone(&retained));
    }
    assert_eq!(cache.entries.len(), MAX_ENTRIES);
    assert!(cache.get(&[0; 32]).is_none());
    assert_eq!(retained.rules.len(), 1);
    let mut oversized = retained.publication();
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
