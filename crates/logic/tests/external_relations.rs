// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Acceptance coverage for query-scoped annotated external relations.

use std::collections::BTreeSet;
use std::sync::Mutex;

use gmeow_logic::annotation::{AnnotationContract, AnnotationRequest};
use gmeow_logic::dispatch::{
    RelationAnnotationRequest, RelationQueryError, dispatch_query_annotated,
    dispatch_query_annotated_with_relations,
};
use gmeow_logic::external_relation::{
    ExternalRelationProvider, NeverCancelled, QueryRelationProviders, RelationAnnotationDimension,
    RelationBatch, RelationCall, RelationCancellation, RelationExecutionFailureKind,
    RelationInvocationStatus, RelationOrderDirection, RelationOrdering, RelationProviderBudget,
    RelationProviderDescriptor, RelationProviderError, RelationProviderIncompletenessKind,
    RelationProviderRegistration, RelationTuple,
};
use gmeow_logic::provenance::{ZWeightSemiring, term_display};
use gmeow_logic::query_ir::{Budget, parse_query_program};
use gmeow_logic::result::PreservationClaim;
use gmeow_logic::seam::{BudgetStatus, WorldFactSnapshot};
use gmeow_logic::store::WorldStore;
use gmeow_logic_compile::result_shape::ColumnKind;
use purrdf::TermValue;

const WORLD: &str = "https://example.org/world/hybrid";
const PROFILE: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";
const EX: &str = "https://example.org/";
const LEXICAL: &str = "https://example.org/relation/lexical";
const VECTOR: &str = "https://example.org/relation/vector";
const GENERATION: &str = "https://example.org/index/generation/1";
const ALGEBRA: &str = "https://blackcatinformatics.ca/logic/algebra/z-weight-v1";

static NEVER_CANCELLED: NeverCancelled = NeverCancelled;

fn ex(local: &str) -> String {
    format!("{EX}{local}")
}

fn snapshot(store: &WorldStore) -> WorldFactSnapshot {
    WorldFactSnapshot::from_world(store, WORLD, PROFILE).expect("snapshot")
}

fn unscored(_: gmeow_logic::annotation::AnnotationFactRef<'_>) -> Option<i64> {
    None
}

fn scratch_score(fact: gmeow_logic::annotation::AnnotationFactRef<'_>) -> Option<i64> {
    (fact.predicate == LEXICAL).then(|| match term_display(fact.object).as_str() {
        "<https://example.org/doc/one>" => 7,
        "<https://example.org/doc/two>" => 5,
        "<https://example.org/doc/three>" => 3,
        _ => unreachable!("scratch fixture owns exactly three rows"),
    })
}

fn row(query: &str, document: &str, annotation: i64, order_key: &str) -> RelationTuple<i64> {
    RelationTuple {
        arguments: vec![TermValue::iri(ex(query)), TermValue::iri(ex(document))],
        annotation,
        order_key: order_key.to_owned(),
    }
}

struct TableProvider {
    generation: String,
    rows: Vec<RelationTuple<i64>>,
    calls: Mutex<Vec<RelationCall>>,
}

impl TableProvider {
    fn new(rows: Vec<RelationTuple<i64>>) -> Self {
        Self {
            generation: GENERATION.to_owned(),
            rows,
            calls: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> Vec<RelationCall> {
        self.calls.lock().unwrap().clone()
    }
}

impl ExternalRelationProvider<i64> for TableProvider {
    fn call(
        &self,
        call: &RelationCall,
        cancellation: &dyn RelationCancellation,
    ) -> Result<RelationBatch<i64>, RelationProviderError> {
        self.calls.lock().unwrap().push(call.clone());
        if cancellation.is_cancelled() {
            return Err(RelationProviderError::Failure {
                kind: gmeow_logic::external_relation::RelationProviderFailureKind::Cancelled,
                detail: "cancelled".to_owned(),
            });
        }
        let mut rows = self
            .rows
            .iter()
            .filter(|row| {
                call.bounds
                    .iter()
                    .zip(&row.arguments)
                    .all(|(bound, argument)| bound.as_ref().is_none_or(|bound| bound == argument))
            })
            .cloned()
            .collect::<Vec<_>>();
        rows.sort_by(|left, right| call.ordering.compare_rows(left, right));
        rows.truncate(call.limit);
        Ok(RelationBatch {
            artifact_generation: self.generation.clone(),
            rows,
        })
    }
}

struct IncompleteProvider;

impl ExternalRelationProvider<i64> for IncompleteProvider {
    fn call(
        &self,
        _call: &RelationCall,
        _cancellation: &dyn RelationCancellation,
    ) -> Result<RelationBatch<i64>, RelationProviderError> {
        Err(RelationProviderError::Incomplete {
            kind: RelationProviderIncompletenessKind::UncertifiedUniverse,
            detail: "candidate universe cannot be certified".to_owned(),
        })
    }
}

fn descriptor(
    relation: &str,
    provider: &str,
    model: &str,
    dimension: RelationAnnotationDimension,
    schema: Vec<ColumnKind>,
) -> RelationProviderDescriptor {
    RelationProviderDescriptor::new(
        provider,
        GENERATION,
        model,
        relation,
        schema,
        dimension,
        ALGEBRA,
        PreservationClaim::exact(),
        RelationOrdering::new(
            format!("{relation}/order"),
            RelationOrderDirection::Ascending,
        )
        .unwrap(),
    )
    .unwrap()
}

fn registration<'a>(
    relation: &str,
    provider_iri: &str,
    model: &str,
    dimension: RelationAnnotationDimension,
    provider: &'a dyn ExternalRelationProvider<i64>,
) -> RelationProviderRegistration<'a, i64> {
    RelationProviderRegistration::new(
        descriptor(
            relation,
            provider_iri,
            model,
            dimension,
            vec![ColumnKind::Iri, ColumnKind::Iri],
        ),
        16,
        provider,
    )
    .unwrap()
}

fn query<'a>(
    snapshot: &WorldFactSnapshot,
    program: &gmeow_logic::query_ir::QProgram,
    providers: &QueryRelationProviders<'a, i64>,
) -> Result<gmeow_logic::external_relation::RelationQueryResult<i64>, RelationQueryError> {
    let contract = AnnotationContract::exact();
    dispatch_query_annotated_with_relations(
        snapshot,
        WORLD,
        program,
        PROFILE,
        &Budget::default(),
        RelationAnnotationRequest::new(
            AnnotationRequest::new(&ZWeightSemiring, &contract, unscored),
            providers,
        ),
    )
}

#[test]
fn bound_lexical_candidates_join_hard_rdf_constraints_in_one_fixpoint() {
    let provider = TableProvider::new(vec![
        row("cat", "doc/one", 7, "001"),
        row("cat", "doc/two", 5, "002"),
        row("dog", "doc/three", 3, "003"),
    ]);
    let providers = QueryRelationProviders::new(
        vec![registration(
            LEXICAL,
            "https://example.org/provider/lexical",
            "https://example.org/model/bm25-v1",
            RelationAnnotationDimension::Similarity,
            &provider,
        )],
        RelationProviderBudget::new(8, 32).unwrap(),
        &NEVER_CANCELLED,
    )
    .unwrap();
    let store = WorldStore::new();
    store.insert_quad(WORLD, &ex("doc/one"), &ex("status"), &ex("active"));
    store.insert_quad(WORLD, &ex("doc/two"), &ex("status"), &ex("inactive"));
    let program = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
         ex:eligible(Q, D) :- ex:relation/lexical(Q, D), ex:status(D, ex:active).\n\
         ?- ex:eligible(ex:cat, D).\n",
    )
    .expect("provider/RDF join program");

    let result = query(&snapshot(&store), &program, &providers).expect("complete hybrid query");
    assert_eq!(result.answer.status, BudgetStatus::Ok);
    assert_eq!(result.answer.answers.len(), 1);
    assert_eq!(
        result.answer.answers[0].binding["D"],
        format!("<{}>", ex("doc/one"))
    );
    assert_eq!(result.answer.answers[0].annotation, 7);
    assert!(
        result.answer.answers[0]
            .derivations
            .iter()
            .any(|derivation| {
                derivation.provider_sources.iter().any(|source| {
                    source.provider_iri == "https://example.org/provider/lexical"
                        && source.artifact_generation == GENERATION
                        && source.annotation_dimension_iri
                            == RelationAnnotationDimension::Similarity.iri()
                })
            })
    );
    assert_eq!(
        provider.calls().len(),
        1,
        "identical recursive rounds hit cache"
    );
    assert_eq!(
        provider.calls()[0].bounds[0],
        Some(TermValue::iri(ex("cat")))
    );
    assert_eq!(result.receipt.metrics.provider_calls, 1);
    assert!(result.receipt.metrics.cache_hits >= 1);
    assert_eq!(result.receipt.metrics.delivered_rows, 2);
    assert_eq!(result.receipt.metrics.admitted_rows, 2);
    assert_eq!(result.receipt.metrics.bound_calls, 1);
    assert_eq!(
        result.receipt.contributing_providers,
        BTreeSet::from([(
            "https://example.org/provider/lexical".to_owned(),
            GENERATION.to_owned(),
        )])
    );
}

#[test]
fn lexical_and_vector_alternatives_aggregate_and_repeat_deterministically() {
    let lexical = TableProvider::new(vec![row("cat", "doc/one", 7, "001")]);
    let vector = TableProvider::new(vec![row("cat", "doc/one", 3, "001")]);
    let providers = QueryRelationProviders::new(
        vec![
            registration(
                LEXICAL,
                "https://example.org/provider/lexical",
                "https://example.org/model/bm25-v1",
                RelationAnnotationDimension::Similarity,
                &lexical,
            ),
            registration(
                VECTOR,
                "https://example.org/provider/vector",
                "https://example.org/model/embedding-v2",
                RelationAnnotationDimension::Distance,
                &vector,
            ),
        ],
        RelationProviderBudget::new(16, 32).unwrap(),
        &NEVER_CANCELLED,
    )
    .unwrap();
    let store = WorldStore::new();
    store.insert_quad(WORLD, &ex("anchor"), &ex("present"), &ex("yes"));
    let source = snapshot(&store);
    let program = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
         ex:hybrid(Q, D) :- ex:relation/lexical(Q, D).\n\
         ex:hybrid(Q, D) :- ex:relation/vector(Q, D).\n\
         ?- ex:hybrid(ex:cat, D).\n",
    )
    .expect("hybrid alternatives program");

    let first = query(&source, &program, &providers).expect("first query");
    let second = query(&source, &program, &providers).expect("repeat query");
    assert_eq!(first, second);
    assert_eq!(first.answer.answers.len(), 1);
    assert_eq!(
        first.answer.answers[0].annotation, 10,
        "alternative scores use oplus"
    );
    assert_eq!(first.receipt.contributing_providers.len(), 2);
    let dimensions = first
        .answer
        .answers
        .iter()
        .flat_map(|answer| &answer.derivations)
        .flat_map(|derivation| &derivation.provider_sources)
        .map(|source| source.annotation_dimension_iri.as_str())
        .collect::<BTreeSet<_>>();
    assert!(dimensions.contains(RelationAnnotationDimension::Similarity.iri()));
    assert!(dimensions.contains(RelationAnnotationDimension::Distance.iri()));
    assert!(!dimensions.contains(RelationAnnotationDimension::EpistemicConfidence.iri()));
    assert_eq!(first.receipt.receipt_hash.len(), 64);
}

#[test]
fn provider_seed_participates_in_positive_recursion_with_transitive_lineage() {
    let provider = TableProvider::new(vec![row("cat", "doc/one", 7, "001")]);
    let providers = QueryRelationProviders::new(
        vec![registration(
            LEXICAL,
            "https://example.org/provider/lexical",
            "https://example.org/model/bm25-v1",
            RelationAnnotationDimension::Similarity,
            &provider,
        )],
        RelationProviderBudget::new(8, 32).unwrap(),
        &NEVER_CANCELLED,
    )
    .unwrap();
    let store = WorldStore::new();
    store.insert_quad(WORLD, &ex("doc/one"), &ex("link"), &ex("doc/four"));
    store.insert_quad(WORLD, &ex("doc/four"), &ex("link"), &ex("doc/five"));
    let program = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
         ex:reach(Q, D) :- ex:relation/lexical(Q, D).\n\
         ex:reach(Q, D) :- ex:reach(Q, M), ex:link(M, D).\n\
         ?- ex:reach(ex:cat, D).\n",
    )
    .expect("recursive provider program");

    let result = query(&snapshot(&store), &program, &providers).expect("recursive query");
    let rows = result
        .answer
        .answers
        .iter()
        .map(|answer| (answer.binding["D"].clone(), answer.annotation))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        rows,
        BTreeSet::from([
            (format!("<{}>", ex("doc/one")), 7),
            (format!("<{}>", ex("doc/four")), 7),
            (format!("<{}>", ex("doc/five")), 7),
        ])
    );
    let deepest = result
        .answer
        .answers
        .iter()
        .find(|answer| answer.binding["D"] == format!("<{}>", ex("doc/five")))
        .unwrap();
    assert!(deepest.derivations.iter().any(|derivation| {
        derivation
            .provider_sources
            .iter()
            .any(|source| source.provider_iri == "https://example.org/provider/lexical")
    }));
    assert_eq!(result.receipt.metrics.provider_calls, 1);
    assert!(result.receipt.metrics.cache_hits >= 1);
}

#[test]
fn bounded_provider_matches_equivalent_scratch_world_without_materializing_noise() {
    let provider = TableProvider::new(vec![
        row("cat", "doc/one", 7, "001"),
        row("cat", "doc/two", 5, "002"),
        row("dog", "doc/three", 3, "003"),
    ]);
    let providers = QueryRelationProviders::new(
        vec![registration(
            LEXICAL,
            "https://example.org/provider/lexical",
            "https://example.org/model/bm25-v1",
            RelationAnnotationDimension::Similarity,
            &provider,
        )],
        RelationProviderBudget::new(8, 32).unwrap(),
        &NEVER_CANCELLED,
    )
    .unwrap();
    let program = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
         ?- ex:relation/lexical(ex:cat, D).\n",
    )
    .expect("direct provider goal");

    let provider_store = WorldStore::new();
    provider_store.insert_quad(WORLD, &ex("anchor"), &ex("present"), &ex("yes"));
    let provider_result =
        query(&snapshot(&provider_store), &program, &providers).expect("provider query");

    let scratch = WorldStore::new();
    for (query, document) in [("cat", "doc/one"), ("cat", "doc/two"), ("dog", "doc/three")] {
        scratch.insert_quad(WORLD, &ex(query), LEXICAL, &ex(document));
    }
    let contract = AnnotationContract::exact();
    let scratch_result = dispatch_query_annotated(
        &snapshot(&scratch),
        WORLD,
        &program,
        PROFILE,
        &Budget::default(),
        AnnotationRequest::new(&ZWeightSemiring, &contract, scratch_score),
    )
    .expect("scratch-world query");

    let provider_rows = provider_result
        .answer
        .answers
        .iter()
        .map(|answer| (answer.binding.clone(), answer.annotation))
        .collect::<Vec<_>>();
    let scratch_rows = scratch_result
        .answers
        .iter()
        .map(|answer| (answer.binding.clone(), answer.annotation))
        .collect::<Vec<_>>();
    assert_eq!(provider_rows, scratch_rows);
    assert_eq!(
        provider_result.answer.preservation,
        scratch_result.preservation
    );
    assert_eq!(provider_result.receipt.metrics.delivered_rows, 2);
    assert!(provider_result.receipt.metrics.delivered_rows < 3);
}

#[test]
fn incomplete_provider_is_typed_and_registration_does_not_escape_the_query() {
    let providers = QueryRelationProviders::new(
        vec![registration(
            LEXICAL,
            "https://example.org/provider/incomplete",
            "https://example.org/model/incomplete",
            RelationAnnotationDimension::Similarity,
            &IncompleteProvider,
        )],
        RelationProviderBudget::new(2, 8).unwrap(),
        &NEVER_CANCELLED,
    )
    .unwrap();
    let store = WorldStore::new();
    store.insert_quad(WORLD, &ex("anchor"), &ex("present"), &ex("yes"));
    let source = snapshot(&store);
    let program = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
         ?- ex:relation/lexical(ex:cat, D).\n",
    )
    .unwrap();

    let error = query(&source, &program, &providers).expect_err("incomplete is not empty");
    let RelationQueryError::Provider { error, receipt } = error else {
        panic!("expected typed provider error");
    };
    assert_eq!(
        error.kind,
        RelationExecutionFailureKind::ProviderIncomplete(
            RelationProviderIncompletenessKind::UncertifiedUniverse
        )
    );
    assert_eq!(
        error.invocation.status,
        RelationInvocationStatus::Incomplete
    );
    assert_eq!(receipt.invocations.len(), 1);
    assert_eq!(receipt.receipt_hash.len(), 64);

    let contract = AnnotationContract::exact();
    let ordinary = dispatch_query_annotated(
        &source,
        WORLD,
        &program,
        PROFILE,
        &Budget::default(),
        AnnotationRequest::new(&ZWeightSemiring, &contract, unscored),
    )
    .expect("no ambient provider registry exists");
    assert!(ordinary.answers.is_empty());
}

#[test]
fn provider_head_collision_wrong_algebra_and_rdf12_triple_terms_are_explicit() {
    let provider = TableProvider::new(vec![row("cat", "doc/one", 7, "001")]);
    let mut wrong = descriptor(
        LEXICAL,
        "https://example.org/provider/lexical",
        "https://example.org/model/bm25-v1",
        RelationAnnotationDimension::Similarity,
        vec![ColumnKind::Iri, ColumnKind::Iri],
    );
    wrong.annotation_algebra = "https://example.org/algebra/wrong".to_owned();
    let wrong_registration =
        RelationProviderRegistration::new(wrong, 8, &provider).expect("valid wrong identity IRI");
    let wrong_set = QueryRelationProviders::new(
        vec![wrong_registration],
        RelationProviderBudget::new(2, 8).unwrap(),
        &NEVER_CANCELLED,
    )
    .unwrap();
    let store = WorldStore::new();
    store.insert_quad(WORLD, &ex("anchor"), &ex("present"), &ex("yes"));
    let source = snapshot(&store);
    let direct = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
         ?- ex:relation/lexical(ex:cat, D).\n",
    )
    .unwrap();
    assert!(matches!(
        query(&source, &direct, &wrong_set),
        Err(RelationQueryError::Contract(_))
    ));

    let correct_set = QueryRelationProviders::new(
        vec![registration(
            LEXICAL,
            "https://example.org/provider/lexical",
            "https://example.org/model/bm25-v1",
            RelationAnnotationDimension::Similarity,
            &provider,
        )],
        RelationProviderBudget::new(2, 8).unwrap(),
        &NEVER_CANCELLED,
    )
    .unwrap();
    let collision = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
         ex:relation/lexical(Q, D) :- ex:present(Q, D).\n\
         ?- ex:relation/lexical(ex:cat, D).\n",
    )
    .unwrap();
    assert!(matches!(
        query(&source, &collision, &correct_set),
        Err(RelationQueryError::Query { .. })
    ));

    struct TripleProvider(TermValue);
    impl ExternalRelationProvider<i64> for TripleProvider {
        fn call(
            &self,
            _call: &RelationCall,
            _cancellation: &dyn RelationCancellation,
        ) -> Result<RelationBatch<i64>, RelationProviderError> {
            Ok(RelationBatch {
                artifact_generation: GENERATION.to_owned(),
                rows: vec![RelationTuple {
                    arguments: vec![self.0.clone()],
                    annotation: 11,
                    order_key: "001".to_owned(),
                }],
            })
        }
    }
    let triple = TermValue::Triple {
        s: Box::new(TermValue::iri(ex("alice"))),
        p: Box::new(TermValue::iri(ex("knows"))),
        o: Box::new(TermValue::iri(ex("bob"))),
    };
    let triple_provider = TripleProvider(triple.clone());
    let triple_descriptor = descriptor(
        "https://example.org/quoted",
        "https://example.org/provider/triple",
        "https://example.org/model/triple-v1",
        RelationAnnotationDimension::Persistence,
        vec![ColumnKind::TripleTerm],
    );
    let triple_set = QueryRelationProviders::new(
        vec![RelationProviderRegistration::new(triple_descriptor, 4, &triple_provider).unwrap()],
        RelationProviderBudget::new(2, 8).unwrap(),
        &NEVER_CANCELLED,
    )
    .unwrap();
    let triple_program = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
         ?- ex:quoted(T).\n",
    )
    .unwrap();
    let result = query(&source, &triple_program, &triple_set).expect("triple-term provider");
    assert_eq!(result.answer.answers[0].binding["T"], term_display(&triple));
    assert_eq!(result.answer.answers[0].annotation, 11);
    assert_eq!(
        result.answer.answers[0].derivations[0].provider_sources[0].arguments,
        vec![term_display(&triple)]
    );
}
