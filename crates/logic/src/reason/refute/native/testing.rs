// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Tiny synthetic native-store controls. This module never reads authored corpus.
use super::*;
use purrdf::DatasetView;

pub(crate) fn fixtures(edb: &impl DatasetView, schema: bool) -> Vec<NativeFamilyLedger> {
    let native = crate::reason::program::prepare_reasoning_input(edb).unwrap();
    let domains = crate::physical::SelectedDomains::new([]).unwrap();
    if schema {
        let source =
            gmeow_logic_compile::ir::LogicProgram::new(Vec::new(), Vec::new(), Vec::new(), None);
        let prepared = crate::program_analysis::prepare_program(&source).unwrap();
        return crate::reason::program::execute(&prepared, native, &domains, None)
            .unwrap()
            .native_families;
    }
    let mut ledgers = Vec::new();
    for (world, facts) in native.facts {
        let graph = native.graphs[&world].clone();
        let origins = Arc::clone(&native.occurrences[&world]);
        let contract = metadata_identity(
            "synthetic-native-family-input-v2",
            &(&world, &graph, &origins, NativeSourceTerms::RdfSkolem),
        );
        let mut store = FactStore::new();
        let mut rel = RelationStore::with_semantics(
            crate::native_semantics::SemanticVocabulary::GroundedLogicV1,
        );
        for fact in facts {
            store.insert(fact.clone());
            rel.insert(&fact.predicate, &fact.subject, &fact.object);
        }
        let evidence = NativeEvidenceIndex::new(
            world.clone(),
            graph.clone(),
            contract,
            origins,
            &store,
            &domains,
        )
        .unwrap();
        let completed = BTreeSet::from([NativeRead {
            marker: None,
            predicate: None,
            kind: NativeReadKind::Completed,
        }]);
        let input = NativeFamilyInput::new(&store, &rel, &[], &evidence, &completed).unwrap();
        let mut ledger = NativeFamilyLedger::new(world, graph.clone(), contract, None);
        let mut coverage = crate::reason::dl::SourceCoverageWorld {
            graph,
            constructs: Vec::new(),
            admissions: Vec::new(),
        };
        let mut values = SchemaValues::default();
        let mut lists = LogicalListCache::default();
        for fact in store.facts() {
            crate::reason::dl::observe_construct(fact, &input, &mut ledger, &mut coverage).unwrap();
        }
        crate::reason::dl::admit_source_constructs(
            &input,
            &mut values,
            &mut lists,
            &mut ledger,
            &mut coverage,
        )
        .unwrap();
        analyze(
            &input.with_admissions(&coverage),
            &mut values,
            &mut lists,
            &mut ledger,
        )
        .unwrap();
        ledgers.push(ledger);
    }
    ledgers
}

pub(crate) fn decision(
    ledgers: &[NativeFamilyLedger],
    family: impl Fn(NativeRefutationFamily) -> bool,
) -> Option<super::super::Decision> {
    let outcomes: Vec<_> = ledgers
        .iter()
        .flat_map(|ledger| &ledger.outcomes)
        .filter(|outcome| family(outcome.family))
        .collect();
    if outcomes
        .iter()
        .any(|outcome| !outcome.conclusions.is_empty())
    {
        return Some(super::super::Decision::Inconsistent);
    }
    let engaged = outcomes
        .iter()
        .any(|outcome| !matches!(outcome.completion, NativeFamilyCompletion::NotEngaged));
    (engaged
        && outcomes.iter().all(|outcome| {
            matches!(
                outcome.completion,
                NativeFamilyCompletion::NotEngaged | NativeFamilyCompletion::Complete
            )
        }))
    .then_some(super::super::Decision::Consistent)
}
