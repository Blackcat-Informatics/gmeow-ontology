// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

impl CorrespondenceAnalysis {
    /// Build a lookup carrying a single complete source binding entry — for the
    /// dialect lowerings' unit tests that construct a `ProfileBinding` directly (without a
    /// DSL store to transpile from). Production builds the lookup only via
    /// [`transpile_correspondences_indexed`].
    pub(crate) fn for_binding_test(
        cell: &ProjectionCell,
        binding: &ProfileBinding,
        typed: TypedRelation,
    ) -> Self {
        let mut by_key = BTreeMap::new();
        by_key.insert(
            NaturalKey::Binding {
                semantic_key: binding_key(cell, binding),
            },
            typed,
        );
        Self {
            by_key,
            binding_keys: BTreeMap::new(),
            alignment_cells: Vec::new(),
            projection_cells: vec![cell.clone()],
        }
    }
}
