// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use gmeow_logic_compile::projections::grounding_catalog::GroundingCatalog;

/// Consume the already-produced authenticated native catalog and its retained
/// support. No repository source discovery or corpus producer is invoked.
pub fn grounding() -> &'static GroundingCatalog {
    static CATALOG: std::sync::OnceLock<GroundingCatalog> = std::sync::OnceLock::new();
    CATALOG.get_or_init(|| {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("resolve the repository fixture selector root");
        let bytes = gmeow_bundle_import::load_authenticated_source_bytes(&root)
            .expect("load the exact producer-selected bundle without production");
        GroundingCatalog::from_bundle(&bytes).expect("admit the authenticated structural catalog")
    })
}
