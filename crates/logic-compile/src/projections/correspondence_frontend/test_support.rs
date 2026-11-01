// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

impl CorrespondenceAnalysis {
    /// Build a lookup carrying a single complete source binding entry — for the
    /// dialect lowerings' unit tests that construct an owned `ProjectionBinding`
    /// wrapper directly (without a DSL store to transpile from). Production builds the lookup only via
    /// [`transpile_correspondences_indexed`].
    pub(crate) fn for_binding_test(
        cell: &ProjectionCell,
        binding: &ProjectionBinding,
        typed: TypedRelation,
    ) -> Self {
        let mut by_key = BTreeMap::new();
        let empty = purrdf::RdfDatasetBuilder::new().freeze().unwrap();
        let view = DslView::new(&empty);
        let catalog =
            crate::projections::grounding_catalog::GroundingCatalog::admit(&view, &view).unwrap();
        let semantic_key = binding_key(cell, binding);
        let (correspondence, _) = correspondence_for_binding(cell, binding)
            .expect("authored test binding must admit its canonical correspondence identity");
        let binding_keys = BTreeMap::from([(correspondence.iri, semantic_key.clone())]);
        by_key.insert(NaturalKey::Binding { semantic_key }, typed);
        Self {
            native_catalog: catalog,
            definition_subjects: BTreeMap::new(),
            by_key,
            binding_keys,
            alignment_cells: Vec::new(),
            projection_cells: vec![cell.clone()],
        }
    }
}
