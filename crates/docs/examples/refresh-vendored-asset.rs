// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit maintainer producer for vendored-wasm digest and substrate records.

use gmeow_docs::vendored_asset::{
    GMN_ASSET, MCP_ASSET, MCP_CORE_ASSET, QUERY_ASSET, REASON_ASSET, VALIDATE_ASSET,
};

fn main() {
    let name = std::env::args()
        .nth(1)
        .expect("usage: refresh-vendored-asset <query|validate|reason|gmn|mcp|mcp-core>");
    let asset = match name.as_str() {
        "query" => &QUERY_ASSET,
        "validate" => &VALIDATE_ASSET,
        "reason" => &REASON_ASSET,
        "gmn" => &GMN_ASSET,
        "mcp" => &MCP_ASSET,
        "mcp-core" => &MCP_CORE_ASSET,
        other => panic!("unknown vendored asset {other:?}"),
    };
    asset.refresh_manifest();
}
