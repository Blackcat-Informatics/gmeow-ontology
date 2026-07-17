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
use gmeow_logic_compile::ir::{
    ContextualScope, Formula, LogicAxiom, LogicProgram, ReasoningContract, Term,
};
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

/// The certified transitive program PLUS a non-empty `program.formulas`: a Horn TGD
/// `∀x,y. edge(x,y) → derived(x,y)` the full reasoner would honor but the incremental
/// maintainer (rules-only) would silently drop. Its `rules` are still finite positive
/// binary Datalog, so ONLY the formula makes it non-incremental.
fn transitive_program_with_formula() -> LogicProgram {
    let var = |name: &str| Term::var(name).expect("var");
    let edge = Formula::atom(
        Term::iri(format!("{EX}edge")).expect("iri"),
        vec![var("x"), var("y")],
    )
    .expect("edge atom");
    let derived = Formula::atom(
        Term::iri(format!("{EX}derived")).expect("iri"),
        vec![var("x"), var("y")],
    )
    .expect("derived atom");
    let tgd = Formula::Forall {
        vars: vec!["x".into(), "y".into()],
        body: Box::new(Formula::Implies(Box::new(edge), Box::new(derived))),
    };
    transitive_program().with_formulas(vec![tgd])
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

    // (B) Provenance covers the FULL maintained closure at OPEN — before any delta —
    // one witness per derived `reach` fact (a→b, b→c, a→c).
    let base_prov = session.provenance();
    assert!(
        !base_prov.is_empty(),
        "full-closure provenance is available at open, before any delta"
    );
    assert!(
        base_prov
            .iter()
            .all(|p| p.predicate == format!("{EX}reach") && !p.rule_iri.is_empty()),
        "every base-closure witness is a derived reach fact with a firing rule"
    );
    // Subjects/objects render through `term_display`, which angle-brackets IRIs.
    assert!(
        base_prov
            .iter()
            .any(|p| p.subject == format!("<{EX}a>") && p.object == format!("<{EX}c>")),
        "the transitive a→c fact carries a witness at open"
    );
    let base_prov_len = base_prov.len();

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

    // (B) Provenance now covers the FULL closure (base + delta-derived): more witnesses
    // than at open, one per derived reach fact, each with rule + premises + +1 weight.
    let provenance = session.provenance();
    assert!(
        provenance.len() > base_prov_len,
        "full-closure provenance grew with the new derived facts"
    );
    assert!(provenance.iter().all(|p| !p.rule_iri.is_empty()));
    assert!(provenance.iter().all(|p| p.weight == 1));
    assert!(
        provenance
            .iter()
            .all(|p| p.predicate == format!("{EX}reach")),
        "every witness is for a derived reach fact"
    );
    assert!(
        provenance
            .iter()
            .any(|p| p.predicate == format!("{EX}reach") && !p.premises.is_empty()),
        "a derived reach fact cites non-empty premises"
    );
    // Provenance covers exactly the derived facts in facts() (reach rows).
    let derived_reach_rows = session
        .facts()
        .rows
        .iter()
        .filter(|row| row.predicate == format!("{EX}reach"))
        .count();
    assert_eq!(
        provenance.len(),
        derived_reach_rows,
        "one witness per derived fact currently in the closure"
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
fn program_carrying_formulas_is_never_certified_incremental() {
    let contract = ReasoningContract::new();
    let annotation = AnnotationContract::exact();
    let edb = edge_dataset(&[("a", "b"), ("b", "c")]);

    // Control: the pure-rules transitive program is STILL Incremental (no false positive).
    let pure =
        ReasoningSession::open(&edb, &transitive_program(), &contract, &annotation).expect("open");
    assert_eq!(
        pure.fragment_disposition(),
        &FragmentDisposition::Incremental,
        "pure finite-positive-binary-Datalog rules remain Incremental"
    );

    // The SAME rules + a non-empty `program.formulas` must NOT be certified Incremental
    // (the maintainer would drop the formula semantics — a silent approximation).
    let mut session = ReasoningSession::open(
        &edb,
        &transitive_program_with_formula(),
        &contract,
        &annotation,
    )
    .expect("open formula-carrying");
    assert_ne!(
        session.fragment_disposition(),
        &FragmentDisposition::Incremental,
        "a program carrying formulas is never certified Incremental"
    );
    // The Horn formula is decidable by the full reasoner → routed to full rebuild.
    assert_eq!(
        session.fragment_disposition(),
        &FragmentDisposition::RequiresFullRebuild(
            RebuildReason::AdditionsOutsideIncrementalFragment
        ),
    );
    assert!(!session.fragment_supported());

    // apply must NEVER return Applied for such a program.
    let delta = SessionDelta::new(
        session.identity().data_generation.clone(),
        session.head(),
        edge_dataset(&[("c", "d")]),
        vec![],
        None,
    )
    .expect("valid delta");
    match session.apply(&delta) {
        OperationOutcome::Applied { .. } => {
            panic!("apply must never present a certified closure that drops formula semantics")
        }
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
