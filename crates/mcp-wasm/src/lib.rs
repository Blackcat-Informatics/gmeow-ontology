// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! # gmeow-mcp-wasm — the consumer MCP engine, in the browser
//!
//! Compiles the shipped consumer MCP engine ([`gmeow_mcp`]) to
//! `wasm32-unknown-unknown` and exposes it to JavaScript/TypeScript, so a browser
//! console, an editor plugin, or an in-page LLM client can drive the SAME 35-tool /
//! 5-resource JSON-RPC surface the native `gmeow mcp` serves — client-side, with no
//! server, no stdio, and no repository.
//!
//! ## Scope
//!
//! - **The whole surface, not a subset.** The engine is linked in its entirety and
//!   every frame is dispatched through [`gmeow_mcp::McpServer::handle_message`], the
//!   one protocol implementation. `initialize`, `tools/list`, `resources/list`,
//!   `tools/call`, `resources/read`, `shutdown`, and the `notifications/*` sink all
//!   behave exactly as they do natively, because they ARE the native code paths.
//!   Nothing here selects, trims, or degrades the tool registry.
//! - **The bundle is passed IN, never embedded.** A `gmeow.gts` snapshot is tens of
//!   megabytes; baking one into the wasm image would freeze the engine to one bundle
//!   version and make the module unusable against any other. The caller hands the
//!   snapshot bytes over ONCE via [`init`] and then drives frames with [`mcp`]. This
//!   is a deliberate departure from `gmeow-gmn-wasm` / `gmeow-validate-wasm`, which
//!   embed the small, frozen `lang:` codebook: that carrier is a build constant, a
//!   bundle is caller data.
//! - **Transport, not protocol.** `run_stdio` is native-only by construction (a
//!   browser has no stdin); the wasm host supplies the transport by calling [`mcp`]
//!   once per frame. That is the same seam the native stdio loop uses internally, so
//!   there is no second protocol implementation to drift.
//!
//! ## Lifecycle
//!
//! ```text
//!   init(snapshotBytes)   // once — parses the GTS, folds the view, assembles the surface
//!   ready()               // -> true once a snapshot is loaded
//!   mcp(requestJson)      // many — one JSON-RPC frame in, one frame out
//!   version()             // liveness probe: the engine's SemVer
//! ```
//!
//! The server is built once and reused because construction is the expensive half
//! (importing a multi-megabyte GTS event stream and folding it to the carrier
//! dataset); rebuilding it per frame would make the browser surface unusable while
//! producing identical answers. [`init`] may be called again to swap bundles — a
//! second bundle is a new session, and the replacement is total (no state from the
//! previous snapshot survives), so it cannot produce a mixed view.
//!
//! ## Architecture
//!
//! A thin shim: all engine logic lives in `gmeow-mcp` (native-tested), and this crate
//! only owns the snapshot handle and marshals strings/bytes across the JS boundary.
//! The `#[wasm_bindgen]` functions compile natively too, which is what lets the
//! native≡wasm witness (`tests/witness_mcp.rs`) drive [`init`]/[`mcp`] themselves
//! rather than a native look-alike.

use std::cell::RefCell;

use gmeow_mcp::McpServer;
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

/// Whether a snapshot has been loaded and the engine is ready to take frames.
///
/// `false` before the first successful [`init`] (and after an [`init`] that failed —
/// a failed load leaves NO engine installed rather than a half-built one).
#[wasm_bindgen]
pub fn ready() -> bool {
    ENGINE.with_borrow(Option::is_some)
}

/// Load a `gmeow.gts` snapshot and build the consumer MCP engine over it.
///
/// `snapshot` is the raw bundle bytes — the identical artifact the native `gmeow mcp`
/// embeds and the docs site serves. The bytes are parsed to the carrier dataset, the
/// bundle view is folded, and the builtin tool/resource surface is assembled, exactly
/// as [`gmeow_mcp::McpServer::from_snapshot`] does natively.
///
/// Calling this again REPLACES the engine wholesale (a new bundle is a new session).
/// A failed load installs nothing, so [`ready`] stays `false` and [`mcp`] keeps
/// refusing frames rather than answering from a stale or partial bundle.
///
/// # Errors
///
/// Throws a JS exception if the snapshot does not read as a GTS bundle, if the startup
/// language is unresolvable, or if the builtin surface does not assemble.
#[wasm_bindgen]
pub fn init(snapshot: &[u8]) -> Result<(), JsError> {
    let server = McpServer::from_snapshot(snapshot).map_err(|e| JsError::new(e.message()))?;
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
