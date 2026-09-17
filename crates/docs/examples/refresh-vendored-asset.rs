// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit maintainer producer for vendored-wasm digest and substrate records.

use gmeow_docs::vendored_asset::{
    ALL_ASSETS, GMN_ASSET, MCP_ASSET, MCP_CORE_ASSET, QUERY_ASSET, REASON_ASSET, VALIDATE_ASSET,
};

/// Refresh one asset's records, or all records after a coordinated package build.
///
/// Reject an unknown asset name before invoking the explicit maintainer refresh.
fn main() {
    let name = std::env::args()
        .nth(1)
        .expect("usage: refresh-vendored-asset <query|validate|reason|gmn|mcp|mcp-core|all>");
    let assets: &[&gmeow_docs::vendored_asset::VendoredWasmAsset] = match name.as_str() {
        "query" => &[&QUERY_ASSET],
        "validate" => &[&VALIDATE_ASSET],
        "reason" => &[&REASON_ASSET],
        "gmn" => &[&GMN_ASSET],
        "mcp" => &[&MCP_ASSET],
        "mcp-core" => &[&MCP_CORE_ASSET],
        "all" => ALL_ASSETS,
        other => panic!("unknown vendored asset {other:?}"),
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for asset in assets {
        asset.refresh_manifest(&root);
    }
}
