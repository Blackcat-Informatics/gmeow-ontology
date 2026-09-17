// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Strict compact native-law access through an already retained immutable snapshot.

use std::sync::Arc;

use purrdf::gts::model::Graph;

use super::{Bundle, BundleDecode};

/// Native reasoned-gate preparation shipped with its matching reasoning result.
pub use gmeow_gts_profile::archive::REASONED_GATES_MEMBER;

impl Bundle {
    /// Read the native laws from this snapshot's reasoning archive once.
    ///
    /// Reuses the retained GTS view, authenticates the selected archive blob, and scans
    /// borrowed tar members without cloning unrelated bodies. Only the compact selected
    /// member remains cached, bounded to 16 MiB and scoped to this immutable bundle.
    /// The caller decodes and validates the native law type, preserving the crate DAG.
    ///
    /// Corpus tests must consume the producer-published selected member action instead
    /// of extracting an archive here. This method performs no source compilation.
    ///
    /// # Errors
    /// Rejects reader diagnostics, missing or ambiguous archives/members, corrupt blob
    /// identity, upstream archive-reader errors, oversized selected members or a
    /// poisoned cache lock.
    pub fn prepared_reasoned_gates(&self) -> gmeow_errors::Result<Arc<[u8]>> {
        let mut selected = self
            .prepared_reasoned_gates
            .lock()
            .map_err(|_| decode_error("prepared native-law cache lock is poisoned"))?;
        if let Some(bytes) = selected.as_ref() {
            return Ok(Arc::clone(bytes));
        }
        let bytes = read_member(self.view.graph())?;
        *selected = Some(Arc::clone(&bytes));
        Ok(bytes)
    }
}

fn decode_error(message: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(BundleDecode {
        message: message.into(),
    })
}

fn read_member(graph: &Graph) -> gmeow_errors::Result<Arc<[u8]>> {
    gmeow_gts_profile::archive::required_member(
        graph,
        gmeow_gts_profile::archive::REASONING_REP,
        REASONED_GATES_MEMBER,
        gmeow_gts_profile::archive::MAX_NATIVE_MEMBER_BYTES,
    )
}

#[path = "reasoned_gates.tests.rs"]
#[cfg(test)]
mod tests;
