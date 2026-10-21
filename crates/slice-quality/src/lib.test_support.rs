// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// Explicit empty authority for independently authored fixtures that contain no
/// selected grounding correspondence; never used by corpus-backed consumers.
pub(crate) fn independent_grounding_catalog() -> GroundingCatalog {
    let dataset = purrdf::parse_dataset(b"", "text/turtle", None).unwrap();
    GroundingCatalog::from_sources(&[], &gmeow_logic_compile::ingest::DslView::new(&dataset))
        .unwrap()
}
