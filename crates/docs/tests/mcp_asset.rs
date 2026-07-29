// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Anti-rot gate for the console's two vendored MCP segments.
//!
//! The site ships PINNED copies of both engine segments — `crates/docs/assets/mcp-core/`
//! (the first-load image) and `crates/docs/assets/mcp/` (the demand-loaded reasoner) —
//! refreshed by `make maint-refresh-mcp-core-asset` / `make maint-refresh-mcp-asset`. The
//! pipeline never rebuilds wasm, so nothing structurally forces a vendored blob to stay in
//! step with its source crate. This gating test drives the SHARED vendored-wasm-asset
//! harness ([`gmeow_docs::vendored_asset`]) over both descriptors: each vendored `.wasm` is
//! a real module of plausible size, each JS surface still carries the API the docs
//! controller dispatches through, and a BLAKE3 manifest pins the exact bytes so a
//! stale-but-still-functional engine cannot slip through.
//!
//! # Why one test file for two segments, and why it replaced three
//!
//! This file replaced `validate_asset.rs`, `reason_asset.rs` and `gmn_asset.rs`. Those gated
//! three separate vendored shims — a validator, a reasoner, a codec — each with its own
//! bespoke export surface. They were duplicate capability once the MCP surface could answer
//! the same questions, and they are retired: the site now speaks ONE protocol to the SAME
//! engine an agent drives. The two segments that remain are DISJOINT halves of one surface
//! rather than independent engines, so they are gated together — a core image whose
//! deferral signal drifted from the reasoning image that answers it would be a defect of
//! the PAIR, not of either alone.
//!
//! Behaviour (does a frame actually get answered?) is covered by the Node lanes
//! (`crates/mcp-core-wasm/js/tests/`, `crates/mcp-wasm/js/tests/`), which the two refresh
//! targets run BEFORE re-pinning the digests — so the pinned bytes always describe an
//! engine whose native≡wasm parity has just been proven.

use gmeow_docs::vendored_asset::{MCP_ASSET, MCP_CORE_ASSET};

#[test]
fn vendored_mcp_core_segment_passes_the_anti_rot_gate() {
    // Structural (real `\0asm` module + plausible size), export-surface (the wrapper's
    // `tieredMcp`/`initTiered` demand-loader plus the bindings' `mcp` frame entry), and the
    // `DIGESTS.blake3` equality gate — all defined once on the shared descriptor.
    MCP_CORE_ASSET.verify();
}

#[test]
fn vendored_mcp_reasoning_segment_passes_the_anti_rot_gate() {
    MCP_ASSET.verify();
}

/// The attestation each segment ships must be PRESENT and CURRENT — that is what makes the
/// site's interactive capabilities proven surfaces rather than decorative self-claims.
///
/// `attestation_status` returns `Some(message)` on a missing, empty, or drifted witness; an
/// engine whose digests no longer match its attestation is exactly the "the parity we proved
/// was for different bytes" failure the digest pin exists to catch.
#[test]
fn both_segments_carry_a_present_and_current_native_wasm_attestation() {
    for asset in [&MCP_CORE_ASSET, &MCP_ASSET] {
        assert!(
            asset.attestation_status().is_none(),
            "segment '{}' has no current native↔wasm attestation: {}",
            asset.name,
            asset.attestation_status().unwrap_or_default()
        );
    }
}

/// The two segments must be genuinely DIFFERENT images.
///
/// The bug this pins closed is the one the consolidation was built to fix: the reasoning
/// segment used to be the core image PLUS the reasoner — a superset that duplicated every
/// core byte on disk. If a future refactor made `gmeow-mcp-wasm` link the core tool surface
/// again, the two vendored blobs would converge in size and content; this catches that
/// without needing to re-measure a byte budget.
#[test]
fn the_two_segments_are_distinct_images_not_a_superset_and_a_subset() {
    let dir = |a: &gmeow_docs::vendored_asset::VendoredWasmAsset| a.asset_dir();
    let core = std::fs::read(dir(&MCP_CORE_ASSET).join(MCP_CORE_ASSET.wasm_file))
        .expect("the core segment blob is vendored");
    let reasoning = std::fs::read(dir(&MCP_ASSET).join(MCP_ASSET.wasm_file))
        .expect("the reasoning segment blob is vendored");
    assert_ne!(
        core, reasoning,
        "the two segments must be different images — identical bytes would mean one is a \
         copy of the other rather than its disjoint half"
    );
}
