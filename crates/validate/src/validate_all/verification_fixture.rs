// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit authenticated laws for tests whose own RDF input is tiny and synthetic.

use gmeow_logic::verify::{PreparedReasonedGates, PreparedVerification};
use std::cell::OnceCell;
use std::rc::Rc;
use std::sync::OnceLock;

pub(crate) fn gates() -> &'static PreparedReasonedGates {
    struct Selected {
        selector: String,
        gates: PreparedReasonedGates,
    }
    static GATES: OnceLock<gmeow_errors::Result<Selected>> = OnceLock::new();
    let selector = std::env::var(gmeow_action_cache::selection::MANIFEST_SHA256_ENV)
        .expect("synthetic verification controls require the exact native law selector");
    let selected = GATES
        .get_or_init(|| {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
            let bytes = gmeow_action_cache::selection::source_artifacts::load(
                &root,
                "stage-conformance",
                gmeow_logic::verify::PREPARED_GATES_CHANNEL,
            )
            .map_err(gmeow_errors::Diag::from)?;
            let gates = serde_json::from_slice(&bytes).map_err(gmeow_errors::Diag::from)?;
            Ok(Selected {
                selector: selector.clone(),
                gates,
            })
        })
        .as_ref()
        .unwrap_or_else(|error| panic!("authenticated native laws: {error}"));
    assert_eq!(
        selected.selector, selector,
        "native law controls cannot cross selectors"
    );
    &selected.gates
}

pub(crate) fn verification() -> Rc<PreparedVerification<'static>> {
    thread_local! {
        static VERIFY: OnceCell<gmeow_errors::Result<Rc<PreparedVerification<'static>>>> =
            const { OnceCell::new() };
    }
    let gates = gates();
    VERIFY.with(|verification| {
        Rc::clone(
            verification
                .get_or_init(|| PreparedVerification::new(&[], gates).map(Rc::new))
                .as_ref()
                .unwrap_or_else(|error| panic!("selected math verification context: {error}")),
        )
    })
}
