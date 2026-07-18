// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! AC6 (composition) — a session opened over a paged world-source produces the identical
//! derived closure as the resident session over the same facts, reports non-trivial
//! page-fault / source accounting, threads the paged `WorldSourceIdentity` into the
//! data-generation axis, and maintains that equality across an incremental delta.
//!
//! NOTE (PackView): the [`ReasoningSession`] façade exposes exactly two constructors —
//! `open` (resident `RdfDataset`) and `open_paged` (`PagedDataset`). There is no
//! pack-view session constructor, so a succinct-pack path cannot be driven through the
//! façade; the cross-view (resident == paged) equality below is the composition surface.

use std::sync::Arc;

use gmeow_logic::runtime::{
    FragmentDisposition, OperationOutcome, ReasoningSession, SessionDelta, Suppression,
};
use gmeow_logic::seam::WorldSourceIdentity;
use purrdf::ir::InMemoryPageProvider;
use purrdf::{PageGeneration, PagedDataset, PagedQueryLimits};

mod session_common;
use session_common::*;

fn paged_identity() -> WorldSourceIdentity {
    WorldSourceIdentity::new(
        "urn:blake3:paged-generation-7",
        "https://example.org/pages-v1",
    )
}

fn open_paged_session(program: &gmeow_logic_compile::ir::LogicProgram) -> ReasoningSession {
    let (contract, annotation) = baseline_contracts();
    let pages = vec![edge_arc(&[("a", "b")]), edge_arc(&[("b", "c")])];
    let provider = Arc::new(InMemoryPageProvider::with_generation(
        pages,
        PageGeneration(7),
    ));
    let paged = PagedDataset::from_provider(provider).expect("seal pages");
    match ReasoningSession::open_paged(
        &paged,
        paged_identity(),
        WORLD,
        program,
        &contract,
        &annotation,
        PagedQueryLimits::UNBOUNDED,
    ) {
        Ok(session) => session,
        Err(outcome) => panic!("paged open failed: {outcome:?}"),
    }
}

#[test]
fn ac6_paged_and_resident_closures_are_equal() {
    let program = transitive_program();
    let (contract, annotation) = baseline_contracts();
    let idb = idb_reach();

    let resident_edb = edge_dataset(&[("a", "b"), ("b", "c")]);
    let resident =
        ReasoningSession::open(&resident_edb, &program, &contract, &annotation).expect("resident");
    let paged = open_paged_session(&program);

    // Cross-view closure equality (resident == paged), and both are genuine incremental
    // maintainers.
    assert_eq!(
        session_derived(&paged, &idb),
        session_derived(&resident, &idb),
        "paged closure equals resident closure"
    );
    assert!(
        !session_derived(&paged, &idb).is_empty(),
        "the closure is non-trivial"
    );
    assert_eq!(
        paged.fragment_disposition(),
        &FragmentDisposition::Incremental
    );

    // Page-fault / source accounting is exposed and non-trivial.
    let metrics = paged.paged_metrics().expect("paged metrics present");
    assert!(
        metrics.source.delivered_quads() >= 2,
        "both edge pages were paged in (delivered quads > 0)"
    );
    assert!(
        !metrics.backend.requested_pages.is_empty(),
        "the backend recorded per-page fault accounting"
    );

    // The paged source identity threads into the data-generation axis.
    assert_eq!(paged.identity().data_generation, paged_identity());

    // A resident open over the same facts carries a DIFFERENT (content-addressed) data
    // generation, so the two identities are distinct even though the closures agree.
    assert_ne!(
        paged.identity().data_generation,
        resident.identity().data_generation,
        "the paged source identity is honored, not overwritten by a resident mint"
    );
}

#[test]
fn ac6_paged_session_maintains_parity_across_a_delta() {
    let program = transitive_program();
    let idb = idb_reach();

    let mut paged = open_paged_session(&program);

    // Apply an incremental delta to the paged-composed session.
    let delta = SessionDelta::new(
        paged.identity().data_generation.clone(),
        paged.head(),
        edge_dataset(&[("c", "d")]),
        Vec::<Suppression>::new(),
        None,
    )
    .expect("valid delta");
    match paged.apply(&delta) {
        OperationOutcome::Applied { .. } => {}
        other => panic!("expected Applied on the paged session, got {other:?}"),
    }

    // Parity against the full-recompute oracle over the paged base + delta facts.
    let combined = edge_dataset(&[("a", "b"), ("b", "c"), ("c", "d")]);
    assert_eq!(
        session_derived(&paged, &idb),
        oracle_derived(&program, &combined, &idb),
        "the paged session maintains full-recompute parity across a delta"
    );
}
