// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Compact producer exports, bound to the exact selector throughout each suite.

use std::sync::OnceLock;

use serde::de::DeserializeOwned;

pub(super) struct Selected<T> {
    selector: String,
    value: T,
}

pub(super) fn get<'a, T: DeserializeOwned>(
    cell: &'a OnceLock<Result<Selected<T>, gmeow_errors::Diag>>,
    channel: &str,
) -> &'a T {
    let selector = std::env::var(gmeow_action_cache::selection::MANIFEST_SHA256_ENV)
        .expect("source contracts require the exact producer selector");
    let selected = cell
        .get_or_init(|| {
            let bytes = gmeow_action_cache::selection::source_artifacts::load(
                &gmeow_conformance::paths::repo_root(),
                "stage-conformance",
                channel,
            )
            .map_err(gmeow_errors::Diag::from)?;
            let value = serde_json::from_slice(&bytes).map_err(gmeow_errors::Diag::from)?;
            Ok(Selected {
                selector: selector.clone(),
                value,
            })
        })
        .as_ref()
        .unwrap_or_else(|error| panic!("authenticated source artifact {channel}: {error}"));
    assert_eq!(
        selected.selector, selector,
        "source observations cannot cross producer selector identities"
    );
    &selected.value
}
