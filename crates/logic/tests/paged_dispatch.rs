// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Direct GMEOW dispatch over PurRDF resident, paged, and succinct-pack views.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

use gmeow_logic::annotation::{AnnotationContract, AnnotationRequest};
use gmeow_logic::dispatch::{
    FallibleViewQueryError, dispatch_query_annotated_fallible_view, dispatch_query_fallible_view,
    dispatch_query_view,
};
use gmeow_logic::materialize::{
    MaterializationLimits, materialize_program, materialize_program_fallible_view,
    materialize_program_source,
};
use gmeow_logic::provenance::ZWeightSemiring;
use gmeow_logic::query_ir::{Budget, parse_query_program};
use gmeow_logic::seam::{
    BudgetStatus, DerivationId, DerivationRecord, DerivedQuad, RdfViewFactSource, WorldFactPattern,
    WorldFactSource, WorldSourceIdentity,
};
use gmeow_logic_compile::ir::{ContextualScope, LogicAxiom, LogicProgram, LogicRule};
use purrdf::ir::{CountingDemandProvider, InMemoryPageProvider};
use purrdf::{
    PackBuilder, PackView, PageFault, PageGeneration, PageId, PageMaterialization, PageProvider,
    PagedDataset, PagedQueryError, PagedQueryLimits, RdfAnnotation, RdfDataset, RdfDatasetBuilder,
    RdfQuad, RdfReifier, RdfTerm, RdfTriple,
};

const WORLD: &str = "https://example.org/world";
const PROFILE: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";
const SOURCE_CONTRACT: &str = "https://example.org/contract/test-pages-v1";
const EX: &str = "https://example.org/";

fn identity(generation: u64) -> WorldSourceIdentity {
    WorldSourceIdentity::new(
        format!("https://example.org/generation/{generation}"),
        SOURCE_CONTRACT,
    )
}

fn page(quads: &[(&str, &str, &str)]) -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for (subject, predicate, object) in quads {
        builder.push_owned_quad(
            &RdfQuad::new(RdfTerm::iri(*subject), *predicate, RdfTerm::iri(*object))
                .in_graph(RdfTerm::iri(WORLD)),
        );
    }
    builder.freeze().expect("valid page")
}

fn graph_pages() -> Vec<Arc<RdfDataset>> {
    vec![
        page(&[(
            "https://example.org/a",
            "https://example.org/edge",
            "https://example.org/b",
        )]),
        page(&[(
            "https://example.org/b",
            "https://example.org/edge",
            "https://example.org/c",
        )]),
        page(&[(
            "https://example.org/noise-s",
            "https://example.org/noise",
            "https://example.org/noise-o",
        )]),
    ]
}

fn ground_selective_pages() -> Vec<Arc<RdfDataset>> {
    vec![
        page(&[(
            "https://example.org/a",
            "https://example.org/edge",
            "https://example.org/b",
        )]),
        page(&[(
            "https://example.org/unrelated-s",
            "https://example.org/edge",
            "https://example.org/unrelated-o",
        )]),
        page(&[(
            "https://example.org/noise-s",
            "https://example.org/noise",
            "https://example.org/noise-o",
        )]),
    ]
}

fn reachability_program() -> gmeow_logic::query_ir::QProgram {
    parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
         ex:path(X, Y) :- ex:edge(X, Y).\n\
         ex:path(X, Z) :- ex:edge(X, Y), ex:path(Y, Z).\n\
         ?- ex:path(ex:a, Y).\n",
    )
    .expect("valid reachability query")
}

#[test]
fn resident_paged_and_pack_views_produce_identical_recursive_answers() {
    let pages = graph_pages();
    let resident = RdfDataset::union(&pages.iter().map(AsRef::as_ref).collect::<Vec<_>>());
    let paged = PagedDataset::from_provider(Arc::new(InMemoryPageProvider::with_generation(
        pages,
        PageGeneration(7),
    )))
    .expect("seal paged dataset");
    let pack_bytes = PackBuilder::build_bytes(&resident).expect("build succinct pack");
    let pack = PackView::from_bytes(&pack_bytes).expect("open succinct pack");
    let program = reachability_program();
    let budget = Budget::default();

    let resident_answer =
        dispatch_query_view(&resident, identity(7), WORLD, &program, PROFILE, &budget)
            .expect("resident dispatch");
    let paged_answer = dispatch_query_view(&paged, identity(7), WORLD, &program, PROFILE, &budget)
        .expect("infallible paged dispatch");
    let pack_answer = dispatch_query_view(&pack, identity(7), WORLD, &program, PROFILE, &budget)
        .expect("pack dispatch");

    assert_eq!(resident_answer.answer, paged_answer.answer);
    assert_eq!(resident_answer.answer, pack_answer.answer);
    assert_eq!(resident_answer.answer.bindings.len(), 2);
    assert_eq!(resident_answer.evidence.source.delivered_quads(), 2);
    assert_eq!(paged_answer.evidence.source.delivered_quads(), 2);
    assert_eq!(pack_answer.evidence.source.delivered_quads(), 2);
    assert_eq!(resident_answer.identity.source, identity(7));
    assert_eq!(resident_answer.identity.engine_descriptor_hash.len(), 64);
    assert_eq!(resident_answer.identity.query_contract_hash.len(), 64);
}

#[test]
fn selective_recursive_query_admits_only_relevant_pages() {
    let pages = graph_pages();
    let thunks = pages
        .into_iter()
        .map(|page| {
            Box::new(move || page.clone()) as Box<dyn Fn() -> Arc<RdfDataset> + Send + Sync>
        })
        .collect();
    let provider = Arc::new(CountingDemandProvider::with_generation(
        thunks,
        PageGeneration(11),
    ));
    let paged = PagedDataset::from_provider(provider.clone()).expect("seal counted pages");
    let seal_hits = provider.hits();
    assert_eq!(
        seal_hits, 3,
        "sealing certifies all three page dictionaries"
    );

    let view = paged.query_view(PagedQueryLimits::UNBOUNDED);
    let result = dispatch_query_fallible_view(
        &view,
        identity(11),
        WORLD,
        &reachability_program(),
        PROFILE,
        &Budget::default(),
    )
    .expect("fallible paged dispatch");

    assert_eq!(result.answer.bindings.len(), 2);
    assert_eq!(provider.hits() - seal_hits, 2);
    assert_eq!(
        result.evidence.backend.requested_pages,
        vec![PageId(0), PageId(1)]
    );
    assert_eq!(result.evidence.backend.consumed_pages, 2);
    assert_eq!(result.evidence.source.delivered_quads(), 2);
    assert_eq!(result.evidence.source.cardinality_probes, 1);
    assert_eq!(result.evidence.source.estimated_primary_quads, 2);
}

#[test]
fn annotated_paged_dispatch_preserves_source_lineage_and_completeness() {
    let paged = PagedDataset::from_provider(Arc::new(InMemoryPageProvider::with_generation(
        graph_pages(),
        PageGeneration(12),
    )))
    .expect("seal pages");
    let view = paged.query_view(PagedQueryLimits::UNBOUNDED);
    let contract = AnnotationContract::exact();
    let result = dispatch_query_annotated_fallible_view(
        &view,
        identity(12),
        WORLD,
        &reachability_program(),
        PROFILE,
        &Budget::default(),
        AnnotationRequest::new(
            &ZWeightSemiring,
            &contract,
            |fact: gmeow_logic::annotation::AnnotationFactRef<'_>| {
                (fact.predicate == format!("{EX}edge")).then_some(1)
            },
        ),
    )
    .expect("annotated paged dispatch");

    assert_eq!(
        result.evidence.backend.requested_pages,
        vec![PageId(0), PageId(1)]
    );
    assert_eq!(result.answer.answers.len(), 2);
    let b = result
        .answer
        .answers
        .iter()
        .find(|answer| answer.binding["Y"] == format!("<{EX}b>"))
        .expect("direct b answer");
    assert!(b.derivations.iter().any(|derivation| {
        derivation.sources.iter().any(|source| {
            source.graph == WORLD
                && source.predicate == format!("{EX}edge")
                && source.subject == format!("<{EX}a>")
                && source.object == format!("<{EX}b>")
        })
    }));
    let c = result
        .answer
        .answers
        .iter()
        .find(|answer| answer.binding["Y"] == format!("<{EX}c>"))
        .expect("recursive c answer");
    assert_eq!(c.annotation, 1);
    assert!(c.derivations.iter().any(|derivation| {
        derivation
            .sources
            .iter()
            .all(|source| source.graph == WORLD)
            && derivation
                .sources
                .iter()
                .any(|source| source.predicate == format!("{EX}edge"))
            && derivation
                .sources
                .iter()
                .any(|source| source.predicate == format!("{EX}path"))
    }));
    assert_eq!(result.identity.source, identity(12));
    assert_eq!(
        result.identity.query_contract_hash,
        gmeow_logic::runtime::EngineContract::annotated_query_contract_hash(
            PROFILE,
            &Budget::default(),
            &contract,
        )
    );
}

fn forward_program() -> LogicProgram {
    let head = LogicAxiom::new(
        "?x",
        format!("{EX}reach"),
        "?y",
        false,
        false,
        ContextualScope::default(),
    )
    .expect("valid head");
    let body = LogicAxiom::new(
        "?x",
        format!("{EX}edge"),
        "?y",
        false,
        false,
        ContextualScope::default(),
    )
    .expect("valid body");
    LogicProgram::new(
        vec![],
        vec![LogicRule::new(
            head,
            vec![body],
            vec![],
            ContextualScope {
                provenance: Some(format!("{EX}rule/reach")),
                ..ContextualScope::default()
            },
        )],
        vec![],
        None,
    )
}

fn ground_forward_program() -> LogicProgram {
    let head = LogicAxiom::new(
        format!("{EX}a"),
        format!("{EX}reach"),
        format!("{EX}b"),
        false,
        false,
        ContextualScope::default(),
    )
    .expect("valid ground head");
    let body = LogicAxiom::new(
        format!("{EX}a"),
        format!("{EX}edge"),
        format!("{EX}b"),
        false,
        false,
        ContextualScope::default(),
    )
    .expect("valid ground body");
    LogicProgram::new(
        vec![],
        vec![LogicRule::new(
            head,
            vec![body],
            vec![],
            ContextualScope {
                provenance: Some(format!("{EX}rule/ground-reach")),
                ..ContextualScope::default()
            },
        )],
        vec![],
        None,
    )
}

#[test]
fn selected_materialization_pushes_ground_subject_and_object_into_one_page() {
    let paged = PagedDataset::from_provider(Arc::new(InMemoryPageProvider::with_generation(
        ground_selective_pages(),
        PageGeneration(24),
    )))
    .expect("seal pages");
    let view = paged.query_view(PagedQueryLimits::new(1, u64::MAX));
    let selected = materialize_program_fallible_view(
        &ground_forward_program(),
        &view,
        identity(24),
        &[WORLD.to_owned()],
        MaterializationLimits::default(),
        None,
    )
    .expect("ground source pattern must stay within one page");

    assert_eq!(selected.evidence.backend.requested_pages, vec![PageId(0)]);
    assert_eq!(selected.evidence.backend.consumed_pages, 1);
    assert_eq!(selected.evidence.source.delivered_quads(), 1);
    assert!(selected.materialization.quads.iter().any(|quad| {
        quad.predicate == format!("{EX}reach")
            && gmeow_logic::provenance::term_display(&quad.subject) == format!("<{EX}a>")
            && gmeow_logic::provenance::term_display(&quad.object) == format!("<{EX}b>")
    }));
}

#[test]
fn generic_triple_dispatch_pushes_predicate_as_data_and_bound_terms() {
    let paged = PagedDataset::from_provider(Arc::new(InMemoryPageProvider::with_generation(
        ground_selective_pages(),
        PageGeneration(25),
    )))
    .expect("seal pages");
    let view = paged.query_view(PagedQueryLimits::new(1, u64::MAX));
    let program = parse_query_program(&format!("?- triple(<{EX}a>, <{EX}edge>, O, <{WORLD}>).\n"))
        .expect("valid generic triple query");
    let result = dispatch_query_fallible_view(
        &view,
        identity(25),
        WORLD,
        &program,
        PROFILE,
        &Budget::default(),
    )
    .expect("generic source pattern must stay within one page");

    assert_eq!(result.answer.bindings.len(), 1);
    assert_eq!(result.answer.bindings[0]["O"], format!("<{EX}b>"));
    assert_eq!(result.evidence.backend.requested_pages, vec![PageId(0)]);
    assert_eq!(result.evidence.backend.consumed_pages, 1);
    assert_eq!(result.evidence.source.delivered_quads(), 1);
}

#[test]
fn selected_materialization_matches_resident_semantics_without_noise_page() {
    let pages = graph_pages();
    let resident = RdfDataset::union(&pages.iter().map(AsRef::as_ref).collect::<Vec<_>>());
    let paged = PagedDataset::from_provider(Arc::new(InMemoryPageProvider::with_generation(
        pages,
        PageGeneration(14),
    )))
    .expect("seal pages");
    let view = paged.query_view(PagedQueryLimits::UNBOUNDED);
    let program = forward_program();
    let selected = materialize_program_fallible_view(
        &program,
        &view,
        identity(14),
        &[WORLD.to_owned()],
        MaterializationLimits::default(),
        None,
    )
    .expect("selected paged materialization");
    let whole = materialize_program(&program, &resident, MaterializationLimits::default(), None)
        .expect("resident whole materialization");

    let relevant = |rows: &[DerivedQuad]| {
        rows.iter()
            .filter(|row| {
                row.predicate == format!("{EX}edge") || row.predicate == format!("{EX}reach")
            })
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(
        relevant(&selected.materialization.quads),
        relevant(&whole.quads)
    );
    assert!(
        selected
            .materialization
            .quads
            .iter()
            .all(|row| row.predicate != format!("{EX}noise"))
    );
    assert_eq!(
        selected.evidence.backend.requested_pages,
        vec![PageId(0), PageId(1)]
    );
    assert_eq!(selected.evidence.source.delivered_quads(), 2);
    assert_eq!(selected.identity.source, identity(14));
    assert_eq!(
        selected.identity.query_contract_hash,
        gmeow_logic::runtime::EngineContract::materialization_contract_hash(
            &program,
            &[WORLD.to_owned()],
            MaterializationLimits::default(),
            None,
        )
    );
}

#[test]
fn selected_materialization_discards_partial_rows_on_page_budget_failure() {
    let paged = PagedDataset::from_provider(Arc::new(InMemoryPageProvider::with_generation(
        graph_pages(),
        PageGeneration(15),
    )))
    .expect("seal pages");
    let view = paged.query_view(PagedQueryLimits::new(1, u64::MAX));
    let error = materialize_program_fallible_view(
        &forward_program(),
        &view,
        identity(15),
        &[WORLD.to_owned()],
        MaterializationLimits::default(),
        None,
    )
    .expect_err("second edge page must exceed the materialization page budget");

    assert!(matches!(
        error.operational_error(),
        Some(PagedQueryError::PageBudgetExceeded {
            page: PageId(1),
            limit: 1,
            consumed: 1,
        })
    ));
    assert_eq!(error.evidence().backend.consumed_pages, 1);
}

#[test]
fn page_budget_failure_is_typed_and_partial_rows_do_not_escape() {
    let paged = PagedDataset::from_provider(Arc::new(InMemoryPageProvider::with_generation(
        graph_pages(),
        PageGeneration(13),
    )))
    .expect("seal pages");
    let view = paged.query_view(PagedQueryLimits::new(1, u64::MAX));
    let error = dispatch_query_fallible_view(
        &view,
        identity(13),
        WORLD,
        &reachability_program(),
        PROFILE,
        &Budget::default(),
    )
    .expect_err("second relevant page must exceed the page budget");

    assert!(matches!(
        error.operational_error(),
        Some(PagedQueryError::PageBudgetExceeded {
            page: PageId(1),
            limit: 1,
            consumed: 1,
        })
    ));
    assert_eq!(
        error.evidence().backend.requested_pages,
        vec![PageId(0), PageId(1)]
    );
    assert_eq!(error.evidence().backend.consumed_pages, 1);
}

struct SwitchingProvider {
    page: Arc<RdfDataset>,
    generation: AtomicU64,
    mode: AtomicU8,
}

impl SwitchingProvider {
    fn new(page: Arc<RdfDataset>, generation: u64) -> Self {
        Self {
            page,
            generation: AtomicU64::new(generation),
            mode: AtomicU8::new(0),
        }
    }
}

impl PageProvider for SwitchingProvider {
    fn page_count(&self) -> usize {
        1
    }

    fn generation(&self) -> PageGeneration {
        PageGeneration(self.generation.load(Ordering::Relaxed))
    }

    fn materialize(&self, page: PageId) -> Result<PageMaterialization, PageFault> {
        match self.mode.load(Ordering::Relaxed) {
            1 => Err(PageFault::provider(page, "test provider failure")),
            2 => Err(PageFault::cancelled(page, "test cancellation")),
            _ => Ok(PageMaterialization::in_memory(
                self.page.clone(),
                self.generation(),
            )),
        }
    }
}

fn direct_edge_program() -> gmeow_logic::query_ir::QProgram {
    parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
         ?- ex:edge(ex:a, Y).\n",
    )
    .expect("valid direct query")
}

fn switching_error(
    mode: u8,
) -> Box<FallibleViewQueryError<PagedQueryError, purrdf::PagedQueryEvidence>> {
    let provider = Arc::new(SwitchingProvider::new(
        page(&[(
            "https://example.org/a",
            "https://example.org/edge",
            "https://example.org/b",
        )]),
        17,
    ));
    let paged = PagedDataset::from_provider(provider.clone()).expect("seal switchable page");
    provider.mode.store(mode, Ordering::Relaxed);
    let view = paged.query_view(PagedQueryLimits::UNBOUNDED);
    dispatch_query_fallible_view(
        &view,
        identity(17),
        WORLD,
        &direct_edge_program(),
        PROFILE,
        &Budget::default(),
    )
    .expect_err("switching provider must fail")
}

#[test]
fn provider_and_cancellation_failures_preserve_their_typed_root_causes() {
    assert!(matches!(
        switching_error(1).operational_error(),
        Some(PagedQueryError::Provider {
            page: PageId(0),
            ..
        })
    ));
    assert!(matches!(
        switching_error(2).operational_error(),
        Some(PagedQueryError::Cancelled {
            page: PageId(0),
            ..
        })
    ));
}

#[test]
fn stale_generation_is_refused_at_the_preflight_checkpoint() {
    let provider = Arc::new(SwitchingProvider::new(
        page(&[(
            "https://example.org/a",
            "https://example.org/edge",
            "https://example.org/b",
        )]),
        19,
    ));
    let paged = PagedDataset::from_provider(provider.clone()).expect("seal generation 19");
    provider.generation.store(20, Ordering::Relaxed);
    let view = paged.query_view(PagedQueryLimits::UNBOUNDED);
    let error = dispatch_query_fallible_view(
        &view,
        identity(19),
        WORLD,
        &direct_edge_program(),
        PROFILE,
        &Budget::default(),
    )
    .expect_err("generation drift must fail before evaluation");
    assert!(matches!(
        error.operational_error(),
        Some(PagedQueryError::StaleGeneration {
            page: None,
            expected: PageGeneration(19),
            actual: PageGeneration(20),
        })
    ));
    assert_eq!(error.evidence().source.delivered_quads(), 0);
}

fn rdf12_dataset() -> Arc<RdfDataset> {
    let embedded = RdfTriple::new(
        RdfTerm::iri(format!("{EX}s")),
        format!("{EX}p"),
        RdfTerm::iri(format!("{EX}o")),
    );
    let reifier = RdfTerm::iri(format!("{EX}statement"));
    let mut builder = RdfDatasetBuilder::new();
    builder.push_owned_quad(
        &RdfQuad::new(
            RdfTerm::iri(format!("{EX}holder")),
            format!("{EX}mentions"),
            RdfTerm::triple(embedded.clone()),
        )
        .in_graph(RdfTerm::iri(WORLD)),
    );
    builder.push_owned_reifier(
        &RdfReifier::new(reifier.clone(), embedded).in_graph(Some(RdfTerm::iri(WORLD))),
    );
    builder.push_owned_annotation(
        &RdfAnnotation::new(
            reifier,
            format!("{EX}confidence"),
            RdfTerm::iri(format!("{EX}high")),
        )
        .in_graph(Some(RdfTerm::iri(WORLD))),
    );
    builder.freeze().expect("valid RDF 1.2 dataset")
}

#[test]
fn rdf12_triple_terms_reifiers_annotations_and_provenance_cross_the_view_seam() {
    let dataset = rdf12_dataset();
    let source = RdfViewFactSource::new(&*dataset, PROFILE, identity(23));
    let mentions = source
        .in_world(WORLD, None, Some(&format!("{EX}mentions")), None)
        .expect("scan triple-term row");
    assert_eq!(mentions.len(), 1);
    assert!(matches!(
        mentions[0].object,
        purrdf::TermValue::Triple { .. }
    ));
    assert_eq!(mentions[0].source_quad_ids.len(), 1);
    assert!(mentions[0].source_quad_ids[0].contains("/reifier/"));

    let lineage = source
        .derived_by(None, None, None)
        .expect("owned view provenance scan");
    assert_eq!(lineage.len(), 3);
    for row in source
        .in_world(WORLD, None, None, None)
        .expect("scan primary and virtual RDF 1.2 rows")
    {
        assert_eq!(
            source
                .derived_by(Some(&row.derivation_id), None, Some(&row.source_quad_ids))
                .expect("lookup explicit reifier lineage"),
            vec![(row.derivation_id, row.rule_iri, row.source_quad_ids)]
        );
    }

    for (predicate, expected_kind) in [
        (
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies",
            "reifier",
        ),
        ("https://example.org/confidence", "annotation"),
    ] {
        let program = parse_query_program(&format!("?- <{predicate}>(S, O).\n"))
            .expect("valid RDF 1.2 virtual-quad query");
        let result = dispatch_query_view(
            &*dataset,
            identity(23),
            WORLD,
            &program,
            PROFILE,
            &Budget::default(),
        )
        .unwrap_or_else(|error| panic!("{expected_kind} dispatch failed: {error}"));
        assert_eq!(
            result.answer.bindings.len(),
            1,
            "{expected_kind} virtual quad must be queryable"
        );
    }
}

struct ProvenanceSource {
    row: DerivedQuad,
    identity: WorldSourceIdentity,
}

impl WorldFactSource for ProvenanceSource {
    fn identity(&self) -> &WorldSourceIdentity {
        &self.identity
    }

    fn visit_world(
        &self,
        world: &str,
        pattern: &WorldFactPattern,
        visitor: &mut dyn FnMut(&DerivedQuad) -> gmeow_errors::Result<()>,
    ) -> gmeow_errors::Result<()> {
        if self.row.graph == world
            && pattern
                .subject
                .as_ref()
                .is_none_or(|subject| subject == &self.row.subject)
            && pattern
                .predicate
                .as_ref()
                .is_none_or(|predicate| predicate == &self.row.predicate)
            && pattern
                .object
                .as_ref()
                .is_none_or(|object| object == &self.row.object)
        {
            visitor(&self.row)?;
        }
        Ok(())
    }

    fn derived_by(
        &self,
        quad_id: Option<&DerivationId>,
        rule: Option<&str>,
        sources: Option<&[String]>,
    ) -> gmeow_errors::Result<Vec<DerivationRecord>> {
        if quad_id.is_none_or(|candidate| candidate == &self.row.derivation_id)
            && rule.is_none_or(|candidate| candidate == self.row.rule_iri)
            && sources.is_none_or(|candidate| candidate == self.row.source_quad_ids)
        {
            return Ok(vec![(
                self.row.derivation_id.clone(),
                self.row.rule_iri.clone(),
                self.row.source_quad_ids.clone(),
            )]);
        }
        Ok(Vec::new())
    }
}

#[test]
fn selected_materialization_preserves_source_provenance_exactly() {
    let source_row = DerivedQuad {
        graph: WORLD.to_owned(),
        subject: purrdf::TermValue::iri(format!("{EX}a")),
        predicate: format!("{EX}edge"),
        object: purrdf::TermValue::iri(format!("{EX}b")),
        graph_component: WORLD.to_owned(),
        derivation_id: DerivationId(format!("{EX}derivation/source")),
        rule_iri: format!("{EX}rule/import"),
        source_quad_ids: vec![format!("{EX}statement/source")],
        profile: format!("{EX}profile/source"),
        budget_status: BudgetStatus::Partial,
    };
    let source = ProvenanceSource {
        row: source_row.clone(),
        identity: identity(26),
    };
    let materialized = materialize_program_source(
        &ground_forward_program(),
        &source,
        &[WORLD.to_owned()],
        MaterializationLimits::default(),
        None,
    )
    .expect("selected source materialization");

    assert_eq!(
        materialized
            .quads
            .iter()
            .find(|row| row.predicate == source_row.predicate)
            .expect("source row survives materialization"),
        &source_row
    );
}
