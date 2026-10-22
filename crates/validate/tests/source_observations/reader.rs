// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Compact source observations authenticated against the runner's exact selector.

use std::sync::OnceLock;

pub struct Selected<T> {
    selector: String,
    observation: T,
}

/// Cache only this explicitly selected compact observation, including load failure.
/// The caller must select its decoder; a decode refusal never selects another codec.
/// A changed selector in the same process is rejected before any cached value is read.
pub fn selected<'a, T>(
    cache: &'a OnceLock<gmeow_errors::Result<Selected<T>>>,
    stage: &str,
    channel: &str,
    decode: impl FnOnce(&[u8]) -> gmeow_errors::Result<T>,
) -> &'a T {
    let selector = std::env::var(gmeow_action_cache::selection::MANIFEST_SHA256_ENV)
        .expect("validation requires the exact optimized producer selector");
    let value = cache
        .get_or_init(|| {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
            let bytes =
                gmeow_action_cache::selection::source_artifacts::load(&root, stage, channel)
                    .map_err(gmeow_errors::Diag::from)?;
            let observation = decode(&bytes)?;
            Ok(Selected {
                selector: selector.clone(),
                observation,
            })
        })
        .as_ref()
        .unwrap_or_else(|error| panic!("authenticated {stage}/{channel}: {error}"));
    assert_eq!(
        value.selector, selector,
        "validation observations cannot cross selectors"
    );
    &value.observation
}
