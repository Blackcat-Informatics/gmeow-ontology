// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

use super::*;
use gmeow_logic_compile::ir::PreservationKind;

use crate::provenance::ZWeightSemiring;

struct NoopProvider;

impl ExternalRelationProvider<i64> for NoopProvider {
    fn call(
        &self,
        call: &RelationCall,
        _cancellation: &dyn RelationCancellation,
    ) -> Result<RelationBatch<i64>, RelationProviderError> {
        Ok(RelationBatch {
            artifact_generation: "https://example.org/index/generation/1".to_owned(),
            rows: Vec::with_capacity(call.limit),
        })
    }
}

struct StaticProvider {
    response: Result<RelationBatch<i64>, RelationProviderError>,
    calls: Mutex<Vec<RelationCall>>,
}

impl StaticProvider {
    fn complete(rows: Vec<RelationTuple<i64>>) -> Self {
        Self {
            response: Ok(RelationBatch {
                artifact_generation: "https://example.org/index/generation/1".to_owned(),
                rows,
            }),
            calls: Mutex::new(Vec::new()),
        }
    }
}

impl ExternalRelationProvider<i64> for StaticProvider {
    fn call(
        &self,
        call: &RelationCall,
        _cancellation: &dyn RelationCancellation,
    ) -> Result<RelationBatch<i64>, RelationProviderError> {
        self.calls.lock().unwrap().push(call.clone());
        self.response.clone()
    }
}

fn descriptor(relation: &str) -> RelationProviderDescriptor {
    RelationProviderDescriptor::new(
        "https://example.org/provider/lexical",
        "https://example.org/index/generation/1",
        "https://example.org/model/bm25-v1",
        relation,
        vec![ColumnKind::Literal { datatype: None }, ColumnKind::Iri],
        RelationAnnotationDimension::Similarity,
        "https://blackcatinformatics.ca/logic/algebra/z-weight-v1",
        PreservationClaim::exact(),
        RelationOrdering::new(
            "https://example.org/order/lexical-rank",
            RelationOrderDirection::Ascending,
        )
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn descriptor_is_typed_content_addressable_and_rdf12_complete() {
    let mut with_triple = descriptor("https://example.org/relation/mentions");
    with_triple.argument_schema.push(ColumnKind::TripleTerm);
    let key = with_triple.canonical_key();
    assert!(key.contains("triple-term"));
    assert_eq!(with_triple.arity(), 3);
    assert!(
        with_triple
            .preservation
            .polarities
            .contains(&PreservationKind::Exact)
    );
}

#[test]
fn query_set_rejects_duplicate_relation_ownership() {
    let provider = NoopProvider;
    let first = RelationProviderRegistration::new(
        descriptor("https://example.org/relation/name-like"),
        8,
        &provider,
    )
    .unwrap();
    let second = RelationProviderRegistration::new(
        descriptor("https://example.org/relation/name-like"),
        8,
        &provider,
    )
    .unwrap();
    let error = QueryRelationProviders::new(
        vec![first, second],
        RelationProviderBudget::new(4, 32).unwrap(),
        &NeverCancelled,
    )
    .err()
    .expect("duplicate registration must fail");
    assert!(error.detail.contains("registered more than once"));
}

#[test]
fn dimension_identities_never_collapse_to_confidence() {
    let distinct = [
        RelationAnnotationDimension::Similarity,
        RelationAnnotationDimension::Rank,
        RelationAnnotationDimension::Distance,
        RelationAnnotationDimension::Persistence,
        RelationAnnotationDimension::EpistemicConfidence,
    ]
    .into_iter()
    .map(|dimension| dimension.iri().to_owned())
    .collect::<BTreeSet<_>>();
    assert_eq!(distinct.len(), 5);
}

#[test]
fn malformed_iris_empty_schema_and_zero_budgets_are_rejected() {
    let bad_provider = RelationProviderDescriptor::new(
        "relative-provider",
        "https://example.org/index/generation/1",
        "https://example.org/model/bm25-v1",
        "https://example.org/relation/name-like",
        vec![ColumnKind::Iri],
        RelationAnnotationDimension::Similarity,
        "https://blackcatinformatics.ca/logic/algebra/z-weight-v1",
        PreservationClaim::exact(),
        RelationOrdering::new(
            "https://example.org/order/lexical-rank",
            RelationOrderDirection::Ascending,
        )
        .unwrap(),
    )
    .expect_err("relative provider identity must fail");
    assert!(bad_provider.detail.contains("absolute IRI"));

    let empty_schema = RelationProviderDescriptor::new(
        "https://example.org/provider/lexical",
        "https://example.org/index/generation/1",
        "https://example.org/model/bm25-v1",
        "https://example.org/relation/name-like",
        Vec::new(),
        RelationAnnotationDimension::Similarity,
        "https://blackcatinformatics.ca/logic/algebra/z-weight-v1",
        PreservationClaim::exact(),
        RelationOrdering::new(
            "https://example.org/order/lexical-rank",
            RelationOrderDirection::Ascending,
        )
        .unwrap(),
    )
    .expect_err("zero-arity provider relation must fail");
    assert!(empty_schema.detail.contains("at least one argument"));

    assert!(RelationProviderBudget::new(0, 1).is_err());
    assert!(RelationProviderBudget::new(1, 0).is_err());
}

fn relation_row(query: &str, document: &str, score: i64, order: &str) -> RelationTuple<i64> {
    RelationTuple {
        arguments: vec![
            TermValue::simple_literal(query),
            TermValue::iri(format!("https://example.org/document/{document}")),
        ],
        annotation: score,
        order_key: order.to_owned(),
    }
}

fn expect_batch_rejection(
    batch: RelationBatch<i64>,
    per_call_limit: usize,
    bounds: Vec<Option<TermValue>>,
) -> RelationExecutionError {
    let provider = StaticProvider {
        response: Ok(batch),
        calls: Mutex::new(Vec::new()),
    };
    let registration = RelationProviderRegistration::new(
        descriptor("https://example.org/relation/name-like"),
        per_call_limit,
        &provider,
    )
    .unwrap();
    let providers = QueryRelationProviders::new(
        vec![registration],
        RelationProviderBudget::new(1, 16).unwrap(),
        &NeverCancelled,
    )
    .unwrap();
    let mut execution =
        RelationExecution::new(&providers, &ZWeightSemiring, "query-contract").unwrap();
    execution
        .resolve("https://example.org/relation/name-like", bounds)
        .expect_err("malformed complete batch must be rejected atomically")
}

#[test]
fn complete_batches_are_validated_hashed_and_cached_before_budget_charge() {
    let provider = StaticProvider::complete(vec![
        relation_row("cat", "one", 7, "001"),
        relation_row("cat", "two", 5, "002"),
    ]);
    let registration = RelationProviderRegistration::new(
        descriptor("https://example.org/relation/name-like"),
        4,
        &provider,
    )
    .unwrap();
    let providers = QueryRelationProviders::new(
        vec![registration],
        RelationProviderBudget::new(1, 2).unwrap(),
        &NeverCancelled,
    )
    .unwrap();
    let mut execution =
        RelationExecution::new(&providers, &ZWeightSemiring, "query-contract").unwrap();
    let bounds = vec![Some(TermValue::simple_literal("cat")), None];
    let first = execution
        .resolve("https://example.org/relation/name-like", bounds.clone())
        .unwrap();
    let second = execution
        .resolve("https://example.org/relation/name-like", bounds)
        .unwrap();

    assert_eq!(first, second);
    assert_eq!(provider.calls.lock().unwrap().len(), 1);
    assert_eq!(execution.metrics.provider_calls, 1);
    assert_eq!(execution.metrics.cache_hits, 1);
    assert_eq!(execution.metrics.delivered_rows, 2);
    assert_eq!(execution.metrics.admitted_rows, 2);
    assert_eq!(execution.metrics.bound_calls, 1);
    assert_eq!(execution.invocations.len(), 2);
    assert_eq!(
        execution.invocations[0].status,
        RelationInvocationStatus::Complete
    );
    assert_eq!(
        execution.invocations[1].status,
        RelationInvocationStatus::CacheHit
    );
    assert_eq!(
        execution.invocations[0].response_hash,
        execution.invocations[1].response_hash
    );
    assert!(
        execution.invocations[0]
            .response_hash
            .as_ref()
            .is_some_and(|hash| hash.len() == 64)
    );
}

#[test]
fn malformed_provider_rows_are_typed_non_results_and_never_cached() {
    let provider = StaticProvider::complete(vec![
        relation_row("cat", "two", 5, "002"),
        relation_row("cat", "one", 7, "001"),
    ]);
    let registration = RelationProviderRegistration::new(
        descriptor("https://example.org/relation/name-like"),
        4,
        &provider,
    )
    .unwrap();
    let providers = QueryRelationProviders::new(
        vec![registration],
        RelationProviderBudget::new(2, 8).unwrap(),
        &NeverCancelled,
    )
    .unwrap();
    let mut execution =
        RelationExecution::new(&providers, &ZWeightSemiring, "query-contract").unwrap();
    let error = execution
        .resolve(
            "https://example.org/relation/name-like",
            vec![Some(TermValue::simple_literal("cat")), None],
        )
        .expect_err("unordered rows must fail");
    assert_eq!(error.kind, RelationExecutionFailureKind::ContractViolation);
    assert_eq!(
        error.invocation.status,
        RelationInvocationStatus::ContractViolation
    );
    assert_eq!(execution.metrics.admitted_rows, 0);
    assert_eq!(execution.metrics.cache_hits, 0);
}

#[test]
fn bounds_limits_uniqueness_schema_arity_and_generation_are_enforced() {
    let expected_generation = "https://example.org/index/generation/1".to_owned();
    let cat_bound = vec![Some(TermValue::simple_literal("cat")), None];
    let cases = [
        (
            RelationBatch {
                artifact_generation: expected_generation.clone(),
                rows: vec![relation_row("dog", "one", 7, "001")],
            },
            4,
            "violates pushed bound",
        ),
        (
            RelationBatch {
                artifact_generation: expected_generation.clone(),
                rows: vec![
                    relation_row("cat", "one", 7, "001"),
                    relation_row("cat", "two", 5, "002"),
                ],
            },
            1,
            "beyond pushed limit",
        ),
        (
            RelationBatch {
                artifact_generation: expected_generation.clone(),
                rows: vec![
                    relation_row("cat", "one", 7, "001"),
                    relation_row("cat", "one", 7, "002"),
                ],
            },
            4,
            "duplicates an earlier tuple",
        ),
        (
            RelationBatch {
                artifact_generation: expected_generation.clone(),
                rows: vec![RelationTuple {
                    arguments: vec![
                        TermValue::simple_literal("cat"),
                        TermValue::simple_literal("not-an-iri"),
                    ],
                    annotation: 7,
                    order_key: "001".to_owned(),
                }],
            },
            4,
            "does not conform",
        ),
        (
            RelationBatch {
                artifact_generation: expected_generation,
                rows: vec![RelationTuple {
                    arguments: vec![TermValue::simple_literal("cat")],
                    annotation: 7,
                    order_key: "001".to_owned(),
                }],
            },
            4,
            "has arity",
        ),
        (
            RelationBatch {
                artifact_generation: "https://example.org/index/generation/stale".to_owned(),
                rows: vec![relation_row("cat", "one", 7, "001")],
            },
            4,
            "expected",
        ),
    ];

    for (batch, limit, detail) in cases {
        let error = expect_batch_rejection(batch, limit, cat_bound.clone());
        assert_eq!(error.kind, RelationExecutionFailureKind::ContractViolation);
        assert_eq!(
            error.invocation.status,
            RelationInvocationStatus::ContractViolation
        );
        assert!(
            error
                .invocation
                .detail
                .as_deref()
                .is_some_and(|value| value.contains(detail)),
            "expected rejection detail containing {detail:?}, got {:?}",
            error.invocation.detail
        );
        assert_eq!(error.invocation.admitted_rows, 0);
    }
}

struct AtomicCancellation(AtomicBool);

impl RelationCancellation for AtomicCancellation {
    fn is_cancelled(&self) -> bool {
        self.0.load(AtomicOrdering::SeqCst)
    }
}

struct CancelAfterProvider<'cancellation> {
    cancellation: &'cancellation AtomicCancellation,
}

impl ExternalRelationProvider<i64> for CancelAfterProvider<'_> {
    fn call(
        &self,
        _call: &RelationCall,
        _cancellation: &dyn RelationCancellation,
    ) -> Result<RelationBatch<i64>, RelationProviderError> {
        self.cancellation.0.store(true, AtomicOrdering::SeqCst);
        Ok(RelationBatch {
            artifact_generation: "https://example.org/index/generation/1".to_owned(),
            rows: vec![relation_row("cat", "one", 7, "001")],
        })
    }
}

#[test]
fn cancellation_is_checked_before_and_after_calls_without_admitting_rows() {
    let cancelled = AtomicCancellation(AtomicBool::new(true));
    let registration = RelationProviderRegistration::new(
        descriptor("https://example.org/relation/name-like"),
        4,
        &NoopProvider,
    )
    .unwrap();
    let providers = QueryRelationProviders::new(
        vec![registration],
        RelationProviderBudget::new(1, 4).unwrap(),
        &cancelled,
    )
    .unwrap();
    let mut execution =
        RelationExecution::new(&providers, &ZWeightSemiring, "query-contract").unwrap();
    let before = execution
        .resolve("https://example.org/relation/name-like", vec![None, None])
        .expect_err("pre-call cancellation must terminate the query");
    assert_eq!(before.kind, RelationExecutionFailureKind::Cancelled);
    assert_eq!(before.invocation.delivered_rows, 0);
    assert_eq!(execution.metrics.provider_calls, 0);

    let cancellation = AtomicCancellation(AtomicBool::new(false));
    let provider = CancelAfterProvider {
        cancellation: &cancellation,
    };
    let registration = RelationProviderRegistration::new(
        descriptor("https://example.org/relation/name-like"),
        4,
        &provider,
    )
    .unwrap();
    let providers = QueryRelationProviders::new(
        vec![registration],
        RelationProviderBudget::new(1, 4).unwrap(),
        &cancellation,
    )
    .unwrap();
    let mut execution =
        RelationExecution::new(&providers, &ZWeightSemiring, "query-contract").unwrap();
    let after = execution
        .resolve(
            "https://example.org/relation/name-like",
            vec![Some(TermValue::simple_literal("cat")), None],
        )
        .expect_err("post-call cancellation must discard the complete batch");
    assert_eq!(after.kind, RelationExecutionFailureKind::Cancelled);
    assert_eq!(after.invocation.delivered_rows, 1);
    assert_eq!(after.invocation.admitted_rows, 0);
    assert_eq!(execution.metrics.admitted_rows, 0);
}

#[test]
fn call_budget_and_stale_generation_incompleteness_are_typed() {
    let registration = RelationProviderRegistration::new(
        descriptor("https://example.org/relation/name-like"),
        4,
        &NoopProvider,
    )
    .unwrap();
    let providers = QueryRelationProviders::new(
        vec![registration],
        RelationProviderBudget::new(1, 4).unwrap(),
        &NeverCancelled,
    )
    .unwrap();
    let mut execution =
        RelationExecution::new(&providers, &ZWeightSemiring, "query-contract").unwrap();
    execution
        .resolve(
            "https://example.org/relation/name-like",
            vec![Some(TermValue::simple_literal("cat")), None],
        )
        .expect("first distinct request is admitted");
    let exhausted = execution
        .resolve(
            "https://example.org/relation/name-like",
            vec![Some(TermValue::simple_literal("dog")), None],
        )
        .expect_err("second distinct request exceeds the call governor");
    assert_eq!(
        exhausted.kind,
        RelationExecutionFailureKind::BudgetExhausted
    );
    assert_eq!(execution.metrics.provider_calls, 1);

    let stale = StaticProvider {
        response: Err(RelationProviderError::Incomplete {
            kind: RelationProviderIncompletenessKind::StaleGeneration,
            detail: "index generation changed during the call".to_owned(),
        }),
        calls: Mutex::new(Vec::new()),
    };
    let registration = RelationProviderRegistration::new(
        descriptor("https://example.org/relation/name-like"),
        4,
        &stale,
    )
    .unwrap();
    let providers = QueryRelationProviders::new(
        vec![registration],
        RelationProviderBudget::new(1, 4).unwrap(),
        &NeverCancelled,
    )
    .unwrap();
    let mut execution =
        RelationExecution::new(&providers, &ZWeightSemiring, "query-contract").unwrap();
    let stale = execution
        .resolve("https://example.org/relation/name-like", vec![None, None])
        .expect_err("stale generation cannot become semantic absence");
    assert_eq!(
        stale.kind,
        RelationExecutionFailureKind::ProviderIncomplete(
            RelationProviderIncompletenessKind::StaleGeneration
        )
    );
    assert_eq!(
        stale.invocation.status,
        RelationInvocationStatus::Incomplete
    );
}

#[test]
fn provider_failure_and_row_budget_exhaustion_are_not_empty_complete_relations() {
    let failed = StaticProvider {
        response: Err(RelationProviderError::Failure {
            kind: RelationProviderFailureKind::Unavailable,
            detail: "lexical index offline".to_owned(),
        }),
        calls: Mutex::new(Vec::new()),
    };
    let failed_registration = RelationProviderRegistration::new(
        descriptor("https://example.org/relation/name-like"),
        4,
        &failed,
    )
    .unwrap();
    let failed_set = QueryRelationProviders::new(
        vec![failed_registration],
        RelationProviderBudget::new(1, 8).unwrap(),
        &NeverCancelled,
    )
    .unwrap();
    let mut execution =
        RelationExecution::new(&failed_set, &ZWeightSemiring, "query-contract").unwrap();
    let error = execution
        .resolve("https://example.org/relation/name-like", vec![None, None])
        .expect_err("provider failure must cross the boundary");
    assert_eq!(
        error.kind,
        RelationExecutionFailureKind::ProviderFailure(RelationProviderFailureKind::Unavailable)
    );
    assert_eq!(error.invocation.status, RelationInvocationStatus::Failed);

    let oversized = StaticProvider::complete(vec![
        relation_row("cat", "one", 7, "001"),
        relation_row("cat", "two", 5, "002"),
    ]);
    let oversized_registration = RelationProviderRegistration::new(
        descriptor("https://example.org/relation/name-like"),
        4,
        &oversized,
    )
    .unwrap();
    let oversized_set = QueryRelationProviders::new(
        vec![oversized_registration],
        RelationProviderBudget::new(1, 1).unwrap(),
        &NeverCancelled,
    )
    .unwrap();
    let mut execution =
        RelationExecution::new(&oversized_set, &ZWeightSemiring, "query-contract").unwrap();
    let error = execution
        .resolve(
            "https://example.org/relation/name-like",
            vec![Some(TermValue::simple_literal("cat")), None],
        )
        .expect_err("row governor must reject the complete batch atomically");
    assert_eq!(error.kind, RelationExecutionFailureKind::BudgetExhausted);
    assert_eq!(
        error.invocation.status,
        RelationInvocationStatus::BudgetExhausted
    );
    assert_eq!(execution.metrics.admitted_rows, 0);
}
