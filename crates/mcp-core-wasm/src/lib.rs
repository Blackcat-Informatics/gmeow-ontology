// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! # gmeow-mcp-core-wasm — the consumer MCP engine's LEAN core, in the browser
//!
//! The first-load half of the tiered browser console. Compiles [`gmeow_mcp`] with the
//! `reasoning` feature selected OUT to `wasm32-unknown-unknown` and exposes the SAME
//! lifecycle its full sibling [`gmeow-mcp-wasm`] does, so a page can boot the engine on a
//! small image and fetch the reasoner only if a caller actually asks for reasoning.
//!
//! ## What is different from `gmeow-mcp-wasm`, and what is NOT
//!
//! Different: this image does not link the DL reasoner (`gmeow-logic`) or the rubric
//! kernel built on it (`gmeow-slice-quality`). That is the whole point — the governing
//! metric for a console's first load is bytes shipped before the first answer, and the
//! reasoner is by far the largest thing the engine links.
//!
//! NOT different — and this is the contract that makes the split honest rather than a
//! feature cut:
//!
//! - **The surface is total.** `tools/list` advertises all [`gmeow_mcp::TOOL_COUNT`] tools
//!   with byte-identical descriptors, `resources/list` all 5. Discovery cannot tell the two
//!   images apart, and a client written against the full engine needs no conditional code.
//! - **The action theory is total.** `action_policy` serves the same projection: every
//!   tool has its schema and every schema its tool. The theory is not a function of which
//!   segment happens to be resident.
//! - **Nothing is refused and nothing is weakened.** A `tools/call` for one of
//!   [`gmeow_mcp::REASONING_SEGMENT_TOOLS`] returns the typed, machine-readable
//!   `mcp.segment-not-loaded` signal — the stable code, the tool asked for, and the
//!   segment that serves it — which the JS layer uses to load `gmeow-mcp-wasm` and
//!   re-dispatch the IDENTICAL frame. The caller waits longer; it never gets a smaller
//!   answer, an empty result, or an "unknown tool".
//! - **The grounded-memory triad is NOT split across the two images.** `store_claim`,
//!   `recall`, `store_segment`, and `revise_belief` are all deferred together, because each
//!   wasm module owns its own claim store and two modules cannot share one: a triad split
//!   across the images would mint a claim id here and answer `[]` there. Reading memory
//!   therefore costs the reasoning segment's fetch, exactly as writing it does — a slower
//!   answer instead of a lost write. See [`gmeow_mcp::REASONING_SEGMENT_TOOLS`].
//!
//! ## Lifecycle
//!
//! Identical to the full segment's, deliberately: the host swaps one module for the other
//! without changing its calling code.
//!
//! ```text
//!   init(snapshotBytes)   // once — parses the GTS, folds the view, assembles the surface
//!   ready()               // -> true once a snapshot is loaded
//!   mcp(requestJson)      // many — one JSON-RPC frame in, one frame out
//!   version()             // liveness probe: the engine's SemVer
//! ```
//!
//! As in the full segment, the bundle is passed IN rather than embedded (a `gmeow.gts` is
//! caller data, not a build constant), the server is built once because construction is
//! the expensive half, and a second [`init`] is a new session that replaces the engine
//! wholesale.
//!
//! ## Architecture
//!
//! A thin shim: all engine logic lives in `gmeow-mcp` (native-tested), and this crate only
//! owns the snapshot handle and marshals strings/bytes across the JS boundary. The
//! `#[wasm_bindgen]` functions compile natively too, which is what lets the deferral
//! witness (`tests/witness_core.rs`) drive [`init`]/[`mcp`] themselves rather than a
//! native look-alike.

use std::cell::RefCell;

use gmeow_mcp::{McpServer, SegmentSet};
use wasm_bindgen::prelude::*;

thread_local! {
    /// The loaded engine, owned by the module for the lifetime of the wasm instance.
    ///
    /// A thread-local (rather than a `static OnceLock`) because `wasm32-unknown-unknown`
    /// is single-threaded by construction — there is no second thread to share it with —
    /// and because the handle must be REPLACEABLE: [`init`] with a different snapshot is a
    /// new session, which a write-once cell could not express. `RefCell` is the honest
    /// interior-mutability primitive for that on a single-threaded target; every borrow
    /// below is short and non-reentrant (the engine never calls back into this module).
    static ENGINE: RefCell<Option<McpServer>> = const { RefCell::new(None) };
}

/// The engine version (the crate's SemVer), exposed to JS as `version()`.
///
/// A liveness probe for the wasm build: importing the module and calling `version()`
/// proves it instantiated and the MCP engine linked. It does NOT require [`init`] —
/// version is a property of the image, not of a loaded bundle.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// The tool names this image DEFERS, as a JSON array of strings.
///
/// The host reads this ONCE, at load, so it can decide to pre-fetch the reasoning segment
/// (say, because the user's first click is a proof) instead of discovering the need
/// mid-frame. It is the same list the deferral signal names, read off the engine's single
/// declaration ([`gmeow_mcp::REASONING_SEGMENT_TOOLS`]) rather than restated here — a
/// second copy in JS would be the exact drift this crate must not introduce.
#[wasm_bindgen]
pub fn deferred_tools() -> String {
    let names: Vec<&str> = gmeow_mcp::REASONING_SEGMENT_TOOLS.to_vec();
    // A fixed-shape array of plain identifiers; formatting it directly keeps this shim
    // free of a JSON dependency it would otherwise need for one literal.
    let body = names
        .iter()
        .map(|name| format!("\"{name}\""))
        .collect::<Vec<_>>()
        .join(",");
    format!("[{body}]")
}

/// The identifier of the segment this image defers, as it appears in the
/// `mcp.segment-not-loaded` signal's `segment` field.
///
/// A host maps this to the module it must load; exporting it means the mapping keys off
/// the engine's own constant rather than a string the JS layer hard-codes.
#[wasm_bindgen]
pub fn deferred_segment() -> String {
    gmeow_mcp::REASONING_SEGMENT.to_string()
}

/// Whether a snapshot has been loaded and the engine is ready to take frames.
///
/// `false` before the first successful [`init`] (and after an [`init`] that failed —
/// a failed load leaves NO engine installed rather than a half-built one).
#[wasm_bindgen]
pub fn ready() -> bool {
    ENGINE.with_borrow(Option::is_some)
}

/// Load a `gmeow.gts` snapshot and build the LEAN core MCP engine over it.
///
/// `snapshot` is the raw bundle bytes — the identical artifact the native `gmeow mcp`
/// embeds, the docs site serves, and the full browser segment loads. The bytes are parsed
/// to the carrier dataset, the bundle view is folded, and the builtin tool/resource
/// surface is assembled exactly as [`gmeow_mcp::McpServer::from_snapshot`] does natively;
/// the ONLY difference is [`SegmentSet::core`], which routes the reasoning-segment tools
/// to the deferral signal.
///
/// Calling this again REPLACES the engine wholesale (a new bundle is a new session).
///
/// The replacement is ORDERED: the installed engine is dropped BEFORE the new one is
/// built, so a failed load leaves no engine at all — [`ready`] reports `false` and [`mcp`]
/// refuses frames — rather than leaving the PREVIOUS session's bundle serving. The
/// alternative ordering (build, then install on success) reads as safer and is the exact
/// opposite: it makes a failed re-`init` invisible, and the caller who asked for a new
/// bundle keeps getting answers from the old one's data.
///
/// # Errors
///
/// Throws a JS exception if the snapshot does not read as a GTS bundle, if the startup
/// language is unresolvable, or if the builtin surface does not assemble.
#[wasm_bindgen]
pub fn init(snapshot: &[u8]) -> Result<(), JsError> {
    ENGINE.with_borrow_mut(|slot| *slot = None);
    let server = McpServer::from_snapshot_segmented(snapshot, SegmentSet::core())
        .map_err(|e| JsError::new(e.message()))?;
    ENGINE.with_borrow_mut(|slot| *slot = Some(server));
    Ok(())
}

/// Handle ONE JSON-RPC 2.0 frame and return the response frame.
///
/// `request_json` is a single MCP request object (`initialize`, `tools/list`,
/// `resources/list`, `tools/call`, `resources/read`, `shutdown`, or a
/// `notifications/*` notification). The return is the serialized response frame — or
/// the EMPTY string for a notification, which by protocol has no response. Protocol
/// and tool errors are reported IN the frame (a JSON-RPC `error` member, or a tool
/// envelope with `isError: true`), exactly as native, and are therefore not JS
/// exceptions: a tool that fails is a successful protocol exchange.
///
/// A frame naming a deferred tool likewise comes back IN the frame, as the structured
/// `mcp.segment-not-loaded` envelope — the host's cue to load the reasoning segment and
/// send this very string to the full engine.
///
/// # Errors
///
/// Throws a JS exception only when no snapshot has been loaded — the one condition
/// that is not expressible as a protocol response, since without a bundle there is no
/// server to answer for. Call [`init`] first; [`ready`] reports whether that happened.
#[wasm_bindgen]
pub fn mcp(request_json: &str) -> Result<String, JsError> {
    ENGINE.with_borrow(|slot| match slot {
        Some(server) => Ok(server.handle_message(request_json)),
        None => Err(JsError::new(
            "no gmeow.gts snapshot loaded — call init(snapshotBytes) before mcp(frame)",
        )),
    })
}
