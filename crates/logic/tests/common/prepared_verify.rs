// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The synthetic scene is local; its authored native law inventory is producer-owned.

use std::cell::OnceCell;
use std::rc::Rc;
use std::sync::OnceLock;

use gmeow_logic::verify::{PREPARED_GATES_CHANNEL, PreparedReasonedGates, PreparedVerification};

pub fn verification(queries: &[(String, String)]) -> Rc<PreparedVerification<'static>> {
    struct Selected {
        selector: String,
        queries: Vec<(String, String)>,
        gates: PreparedReasonedGates,
    }
    static GATES: OnceLock<gmeow_errors::Result<Selected>> = OnceLock::new();
    // The law identity is immutable and shared. PurRDF's query engine owns a
    // thread-local plan cache, so each worker retains its own prepared verifier.
    thread_local! {
        static VERIFY: OnceCell<gmeow_errors::Result<Rc<PreparedVerification<'static>>>> =
            const { OnceCell::new() };
    }
    let selector = std::env::var(gmeow_action_cache::selection::MANIFEST_SHA256_ENV)
        .expect("synthetic verification requires the producer-selected authored law fixture");
    let selected = GATES
        .get_or_init(|| {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
            let bytes = gmeow_action_cache::selection::source_artifacts::load(
                &root,
                "stage-conformance",
                PREPARED_GATES_CHANNEL,
            )
            .map_err(gmeow_errors::Diag::from)?;
            let gates = serde_json::from_slice(&bytes).map_err(gmeow_errors::Diag::from)?;
            Ok(Selected {
                selector: selector.clone(),
                queries: queries.to_vec(),
                gates,
            })
        })
        .as_ref()
        .unwrap_or_else(|error| panic!("authenticated native law fixture: {error}"));
    assert_eq!(
        selected.selector, selector,
        "prepared law fixtures cannot cross selector identities"
    );
    assert_eq!(
        selected.queries, queries,
        "prepared verification cannot change selected query inventories"
    );
    VERIFY.with(|verification| {
        Rc::clone(
            verification
                .get_or_init(|| {
                    PreparedVerification::new(&selected.queries, &selected.gates).map(Rc::new)
                })
                .as_ref()
                .unwrap_or_else(|error| panic!("prepared native verification: {error}")),
        )
    })
}
