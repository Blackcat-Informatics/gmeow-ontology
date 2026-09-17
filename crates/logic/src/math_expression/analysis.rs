// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Typed source-only expression lowering. Consumers that need an authored
//! structural identity must not substitute an existentially completed closure.

use purrdf::RdfDataset;
use std::collections::BTreeMap;

pub use crate::physical::lower::MathLoweringError;

/// Compute every source expression root through the same native lowerer used by
/// the expression gate. A rejected component cannot hide an independent root.
#[must_use]
pub fn structural_keys(
    dataset: &RdfDataset,
) -> BTreeMap<String, Result<String, MathLoweringError>> {
    crate::physical::lower::math_expression_structural_keys(dataset)
}

/// Lower one explicitly selected root from an original native source dataset.
///
/// # Errors
/// Returns the exact typed grammar/depth/cycle rejection from the native lowerer.
pub fn structural_key_at(dataset: &RdfDataset, root: &str) -> Result<String, MathLoweringError> {
    let graph = crate::physical::lower::MathGraph::from_dataset(dataset);
    crate::physical::lower::arena_structural_key(&graph, root)
}

/// The live recursion limit used by the native expression lowerer.
#[must_use]
pub const fn depth_limit() -> usize {
    crate::physical::lower::MAX_MATH_EXPRESSION_DEPTH
}
