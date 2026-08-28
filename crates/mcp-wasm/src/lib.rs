// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! # gmeow-mcp-wasm — the DEMAND-LOADED reasoning segment, in the browser
//!
//! Compiles the reasoning half of the shipped consumer MCP engine ([`gmeow_mcp`]) to
//! `wasm32-unknown-unknown` and exposes it to JavaScript/TypeScript. It is the second tier
//! of the console: the host loads it on the first `tools/call` handed off by the
//! always-resident core image (`gmeow-mcp-core-wasm`), and replays the identical frame
//! against it.
//!
//! ## Scope
//!
//! - **A genuine DELTA, not a superset.** This image links `gmeow-logic` (the DL
//!   reasoner) and `gmeow-slice-quality` (the rubric kernel over it) and NOTHING of the
//!   core tool surface: `gmeow-mcp` is taken with `default-features = false, features =
//!   ["reasoning"]`, so the transcode hub and the distribution-catalog reader are not in
//!   this crate's dependency tree at all and the [`gmeow_mcp::CORE_SEGMENT_TOOL_COUNT`]
//!   core tool bodies are not compiled. It used to be built with the `reasoning` feature ON
//!   TOP of the defaults — i.e. the whole core image plus the reasoner — which duplicated
//!   every core byte on disk. That was the bug; this is the fix.
//! - **It owns the grounded-memory claim package.** The whole triad — `store_claim`,
//!   `recall`, `store_segment`, `revise_belief` — is served HERE, not because recall
//!   reasons (it does not) but because a wasm module's claim store is private to that
//!   module: a triad split across the two images would lose every write. The writes are
//!   pinned here by their Transaction-Logic commit gate, so the reads follow them.
//! - **The whole surface is still ADVERTISED.** `tools/list` returns all
//!   [`gmeow_mcp::TOOL_COUNT`] descriptors, byte-identical to the core image's and to
//!   native, and every frame is dispatched through
//!   [`gmeow_mcp::McpServer::handle_message`], the one protocol implementation. A
//!   `tools/call` for a CORE tool answers with the typed `mcp.segment-not-loaded` signal
//!   naming the `core` segment — the exact mirror of what core does for a reasoning tool.
//!   A deployment tier is not a reduced theory.
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

use gmeow_mcp::{McpServer, SegmentSet};
use wasm_bindgen::prelude::*;

/// Route a Rust panic's MESSAGE to the host before the trap reaches it.
///
/// Without this a panic in the engine reaches JavaScript as a bare
/// `RuntimeError: unreachable` — no message, no location, indistinguishable from any other
/// trap. That is not a failure a caller can act on, and it is not one a maintainer can
/// diagnose: the browser lane spent several runs unable to say WHICH refusal it had hit.
/// Installed once, on first engine construction.
#[cfg(target_arch = "wasm32")]
fn install_panic_reporter() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            error(&format!("gmeow-mcp-wasm panic: {info}"));
            previous(info);
        }));
    });
}

/// Native parity tests drive the shim without a JavaScript host, so there is no console
/// hook to install. Keeping this a no-op also leaves Rust's normal panic reporter intact.
#[cfg(not(target_arch = "wasm32"))]
fn install_panic_reporter() {}

/// Preserve the engine's exact refusal on both targets. `JsError::new` itself invokes a
/// wasm import and therefore aborts when constructed natively; panic with the original
/// message instead so a native parity failure reports the actionable engine diagnostic.
#[cfg(target_arch = "wasm32")]
fn transport_error(message: &str) -> JsError {
    JsError::new(message)
}

#[cfg(not(target_arch = "wasm32"))]
fn transport_error(message: &str) -> JsError {
    panic!("gmeow-mcp-wasm transport error: {message}")
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    /// The host's own `console.error`. Bound by NAMESPACE rather than through an inline
    /// snippet: a snippet is a separate file wasm-bindgen emits beside the module, and the
    /// vendored asset carries a pinned file list — a diagnostic that only works when an extra
    /// file happens to be copied is a diagnostic that fails exactly when it is needed.
    #[wasm_bindgen(js_namespace = console)]
    fn error(text: &str);
}

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
/// as [`gmeow_mcp::McpServer::from_snapshot`] does natively; the ONLY difference is
/// [`SegmentSet::reasoning_only`], which routes the [`gmeow_mcp::CORE_SEGMENT_TOOL_COUNT`]
/// CORE tools back to the always-resident core image with the typed
/// `mcp.segment-not-loaded` signal instead of answering them here. The
/// [`gmeow_mcp::REASONING_SEGMENT_TOOLS`] answer for real.
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
    install_panic_reporter();
    ENGINE.with_borrow_mut(|slot| *slot = None);
    let server = McpServer::from_snapshot_segmented(snapshot, SegmentSet::reasoning_only())
        .map_err(|error| transport_error(error.message()))?;
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
        None => Err(transport_error(
            "no gmeow.gts snapshot loaded — call init(snapshotBytes) before mcp(frame)",
        )),
    })
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn native_transport_error_preserves_the_underlying_diagnostic() {
        let panic = std::panic::catch_unwind(|| {
            let _: JsError = transport_error("sentinel underlying engine refusal");
        })
        .expect_err("native transport conversion must not call a wasm import");
        let message = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .expect("panic carries a string diagnostic");
        assert!(
            message.contains("sentinel underlying engine refusal"),
            "native transport failure discarded its source diagnostic: {message}"
        );
    }
}
