// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only inspection of prepared native source facts.

pub(super) use purrdf::RdfDataset;

use super::{InputFacts, PreparedReasoningInput, WorldGraphs, prepare_reasoning_input};

/// Read every native statement once, retaining the exact source graph bindings.
pub(in crate::reason) fn input_facts(
    edb: &impl purrdf::DatasetView,
) -> gmeow_errors::Result<(InputFacts, WorldGraphs)> {
    let PreparedReasoningInput { facts, graphs, .. } = prepare_reasoning_input(edb)?;
    Ok((facts, graphs))
}
