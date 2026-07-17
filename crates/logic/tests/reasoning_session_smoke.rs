// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Runtime smoke coverage for the stable `ReasoningSession` façade CORE: the three-way
//! fragment disposition (A), the surfaced derived provenance (B), and the paged
//! world-source composition (C). Full AC1–AC6 parity lives in the dedicated
//! `reasoning_session_*` suites; this file proves the mechanisms are wired and behave.

use std::sync::Arc;

use gmeow_logic::annotation::AnnotationContract;
use gmeow_logic::runtime::{
    FragmentDisposition, OperationOutcome, ReasoningSession, RebuildReason, SessionDelta,
    Suppression,
};
use gmeow_logic::seam::WorldSourceIdentity;
use gmeow_logic_compile::ir::{ContextualScope, LogicAxiom, LogicProgram, ReasoningContract};
use purrdf::ir::InMemoryPageProvider;
use purrdf::{
    PageGeneration, PagedDataset, PagedQueryLimits, RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm,
};

const WORLD: &str = "https://example.org/world";
const EX: &str = "https://example.org/";

fn atom(subject: &str, predicate: &str, object: &str, negated: bool) -> LogicAxiom {
    LogicAxiom::new(
        subject.to_owned(),
        predicate.to_owned(),
        object.to_owned(),
        false,
        negated,
        ContextualScope::default(),
    )
    .expect("valid axiom")
}

fn rule(head: LogicAxiom, body: Vec<LogicAxiom>, name: &str) -> gmeow_logic_compile::ir::LogicRule {
    gmeow_logic_compile::ir::LogicRule::new(
        head,
        body,
        vec![],
        ContextualScope {
            provenance: Some(format!("{EX}rule/{name}")),
            ..ContextualScope::default()
        },
    )
}

/// Certified: transitive closure `reach` over `edge` — finite positive binary Datalog.
fn transitive_program() -> LogicProgram {
    LogicProgram::new(
        vec![],
        vec![
            rule(
                atom("?x", &format!("{EX}reach"), "?y", false),
                vec![atom("?x", &format!("{EX}edge"), "?y", false)],
                "base",
            ),
            rule(
                atom("?x", &format!("{EX}reach"), "?z", false),
                vec![
                    atom("?x", &format!("{EX}reach"), "?y", false),
                    atom("?y", &format!("{EX}edge"), "?z", false),
                ],
                "step",
            ),
        ],
        vec![],
        None,
    )
}

/// Stratified NAF: `q` depends on `¬p`, `p` in a lower stratum — decidable by the full
/// reasoner but NOT incrementally maintainable → Tier 2 (RequiresFullRebuild).
fn stratified_naf_program() -> LogicProgram {
    LogicProgram::new(
        vec![],
        vec![
            rule(
                atom("?x", &format!("{EX}p"), "?y", false),
                vec![atom("?x", &format!("{EX}edge"), "?y", false)],
                "p",
            ),
            rule(
                atom("?x", &format!("{EX}q"), "?y", false),
                vec![
                    atom("?x", &format!("{EX}edge"), "?y", false),
                    atom("?x", &format!("{EX}p"), "?y", true),
                ],
                "q",
            ),
        ],
        vec![],
        None,
    )
}

fn edge_page(edges: &[(&str, &str)]) -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for (s, o) in edges {
        builder.push_owned_quad(
            &RdfQuad::new(
                RdfTerm::iri(format!("{EX}{s}")),
                format!("{EX}edge"),
                RdfTerm::iri(format!("{EX}{o}")),
            )
            .in_graph(RdfTerm::iri(WORLD)),
        );
    }
    builder.freeze().expect("valid page")
}

fn edge_dataset(edges: &[(&str, &str)]) -> RdfDataset {
    Arc::try_unwrap(edge_page(edges)).expect("freshly built page has a single owner")
}

#[test]
fn certified_program_is_incremental_and_surfaces_provenance() {
    let program = transitive_program();
    let contract = ReasoningContract::new();
    let annotation = AnnotationContract::exact();
    let edb = edge_dataset(&[("a", "b"), ("b", "c")]);

    let mut session =
        ReasoningSession::open(&edb, &program, &contract, &annotation).expect("open certified");
    assert_eq!(
        session.fragment_disposition(),
        &FragmentDisposition::Incremental
    );
    assert!(session.fragment_supported());

    // Base closure holds the transitive reach a→c.
    let base_rows = session.facts().rows.len();
    assert!(base_rows > 0, "base closure is non-empty");
    let genesis_head = session.head().to_owned();

    // Insert edge c→d; new reach a→d, b→d, c→d appear.
    let delta = SessionDelta::new(
        session.identity().data_generation.clone(),
        session.head(),
        edge_dataset(&[("c", "d")]),
        vec![],
        None,
    )
    .expect("valid delta");

    match session.apply(&delta) {
        OperationOutcome::Applied { new_state_hash, .. } => {
            assert_ne!(new_state_hash, genesis_head, "head advanced");
        }
        other => panic!("expected Applied, got {other:?}"),
    }
    assert!(session.facts().rows.len() > base_rows, "closure grew");
    assert_ne!(session.head(), genesis_head, "session head advanced");

    // (B) Provenance is surfaced: newly-derived reach facts carry a firing rule +
    // premises + a genuine +1 Z-weight.
    let provenance = session.provenance();
    assert!(!provenance.is_empty(), "derived provenance is surfaced");
    assert!(provenance.iter().all(|p| !p.rule_iri.is_empty()));
    assert!(provenance.iter().all(|p| p.weight == 1));
    assert!(
        provenance
            .iter()
            .any(|p| p.predicate == format!("{EX}reach") && !p.premises.is_empty()),
        "a derived reach fact cites non-empty premises"
    );

    // Double-apply of the already-committed delta is structurally refused.
    match session.apply(&delta) {
        OperationOutcome::Invalid { .. } => {}
        other => panic!("expected Invalid double-apply refusal, got {other:?}"),
    }
}

#[test]
fn stratified_naf_routes_to_full_rebuild() {
    let program = stratified_naf_program();
    let contract = ReasoningContract::new();
    let annotation = AnnotationContract::exact();
    let edb = edge_dataset(&[("a", "b"), ("b", "c")]);

    let mut session =
        ReasoningSession::open(&edb, &program, &contract, &annotation).expect("open naf");
    assert_eq!(
        session.fragment_disposition(),
        &FragmentDisposition::RequiresFullRebuild(
            RebuildReason::AdditionsOutsideIncrementalFragment
        ),
        "stratified NAF is decidable but not incrementally maintainable"
    );
    assert!(!session.fragment_supported());

    let delta = SessionDelta::new(
        session.identity().data_generation.clone(),
        session.head(),
        edge_dataset(&[("c", "d")]),
        vec![Suppression::new(edge_dataset(&[("a", "b")]))],
        None,
    )
    .expect("valid delta");

    match session.apply(&delta) {
        OperationOutcome::RequiresFullRebuild {
            reason: RebuildReason::AdditionsOutsideIncrementalFragment,
        } => {}
        other => panic!("expected RequiresFullRebuild, got {other:?}"),
    }
}

#[test]
fn paged_composition_matches_resident_closure() {
    let program = transitive_program();
    let contract = ReasoningContract::new();
    let annotation = AnnotationContract::exact();

    // Resident session over the same EDB.
    let resident_edb = edge_dataset(&[("a", "b"), ("b", "c")]);
    let resident = ReasoningSession::open(&resident_edb, &program, &contract, &annotation)
        .expect("resident open");

    // Paged session: the same two edges delivered as two demand pages.
    let pages = vec![edge_page(&[("a", "b")]), edge_page(&[("b", "c")])];
    let provider = Arc::new(InMemoryPageProvider::with_generation(
        pages,
        PageGeneration(7),
    ));
    let paged = PagedDataset::from_provider(provider).expect("seal pages");
    let identity = WorldSourceIdentity::new("urn:blake3:paged-7", "https://example.org/pages-v1");

    let paged_session = ReasoningSession::open_paged(
        &paged,
        identity.clone(),
        WORLD,
        &program,
        &contract,
        &annotation,
        PagedQueryLimits::UNBOUNDED,
    )
    .expect("paged open");

    // (C) Cross-view closure equality (resident == paged).
    assert_eq!(
        resident.facts().rows,
        paged_session.facts().rows,
        "paged closure equals resident closure"
    );
    // The paged source identity threaded into the data-generation axis.
    assert_eq!(paged_session.identity().data_generation, identity);
    // Page-fault / source metrics are exposed and non-trivial (quads were delivered).
    let metrics = paged_session
        .paged_metrics()
        .expect("paged metrics present");
    assert!(
        metrics.source.delivered_quads() >= 2,
        "both edge pages were paged in"
    );
    assert_eq!(
        paged_session.fragment_disposition(),
        &FragmentDisposition::Incremental
    );
}
