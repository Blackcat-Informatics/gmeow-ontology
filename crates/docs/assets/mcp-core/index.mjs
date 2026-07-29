// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// gmeow-mcp-core-wasm — the LEAN core of the consumer GMEOW MCP engine, plus the tiered
// dispatcher that makes its deferral invisible to a caller.
//
// The wasm-bindgen `init`/`mcp`/`version` are re-exported as-is; this wrapper adds two
// things the synchronous boundary cannot express: the one-time async wasm instantiation
// (`ready()`, the sibling-shim convention), and `tieredMcp()` — the demand-loader that
// turns the engine's `mcp.segment-not-loaded` signal into "load the reasoning segment and
// re-send this exact frame".
//
// Naming: the sibling shims all call the instantiation helper `ready()`, and this module
// keeps that convention. The wasm module's OWN `ready()` export (is a gmeow.gts snapshot
// installed?) is therefore re-exported here as `loaded()` — one rename at the JS wrapper,
// rather than two different meanings for one name.

import wasmInit, {
  deferred_segment as deferredSegmentJson,
  deferred_tools as deferredToolsJson,
  init,
  mcp,
  ready as snapshotLoaded,
  version,
} from "./pkg/gmeow_mcp_core_wasm.js";

// Cache the in-flight instantiation PROMISE, not a post-resolution boolean: two
// callers that both reach `ready()` before the first `wasmInit()` resolves must share
// one instantiation, not each trigger a full wasm fetch/instantiate. On failure the
// cache is cleared so a later call can retry.
let _ready = null;

async function instantiate(wasmBytesOrUrl) {
  if (wasmBytesOrUrl !== undefined) {
    await wasmInit({ module_or_path: wasmBytesOrUrl });
  } else if (typeof process !== "undefined" && process.versions?.node) {
    const { readFile } = await import("node:fs/promises");
    const { fileURLToPath } = await import("node:url");
    const wasmPath = fileURLToPath(new URL("./pkg/gmeow_mcp_core_wasm_bg.wasm", import.meta.url));
    await wasmInit({ module_or_path: await readFile(wasmPath) });
  } else {
    await wasmInit();
  }
}

export function ready(wasmBytesOrUrl) {
  if (_ready === null) {
    _ready = instantiate(wasmBytesOrUrl).catch((error) => {
      _ready = null;
      throw error;
    });
  }
  return _ready;
}

// The wasm module's own `ready()` — whether a snapshot has been installed by `init`.
function loaded() {
  return snapshotLoaded();
}

// The tool names this image defers, and the segment identifier that serves them. Both are
// read from the engine's own constants (marshalled as JSON across the boundary), never
// restated here: a second copy of the split in JS is exactly the drift the tiering must
// not introduce.
export function deferredTools() {
  return JSON.parse(deferredToolsJson());
}

export function deferredSegment() {
  return deferredSegmentJson();
}

// The stable diagnostic code the engine raises for a tool whose segment is not resident.
// A CONSTANT, matched exactly — never a substring search of a human-readable message.
export const SEGMENT_NOT_LOADED = "mcp.segment-not-loaded";

// Read the deferral signal out of a response frame, or `null` if the frame is anything
// else (a real answer, an ordinary tool error, a protocol error).
//
// The check is structural AND TOTAL over the envelope this function's caller then routes
// on. A deferral is a ROUTING INSTRUCTION: whatever comes back here is handed to
// `loadSegment(segment)` and re-dispatched, so every field it promises must be present and
// of the right type BEFORE it is trusted. Checking only `code` was enough to recognise a
// well-formed signal and not enough to reject a malformed one — a payload carrying the
// code but no `segment` reached `loadSegment(undefined)`, which is a host call made on a
// frame nobody validated.
//
// Required, all of them:
//   * the envelope is an ERROR envelope (`result.isError === true`) — a deferral computed
//     nothing, so a signal riding a success envelope is not one;
//   * `tool` and `segment` are non-empty strings — the two fields the host routes on;
//   * `segment_tools` is an array of strings — the segment's advertised inventory, which a
//     host may pre-flight against.
// Anything else is `null`: the frame is returned to the caller untouched rather than being
// retried blindly. A frame that fails to parse is likewise not a deferral.
export function segmentDeferral(responseFrame) {
  if (typeof responseFrame !== "string" || responseFrame.length === 0) return null;
  let frame;
  try {
    frame = JSON.parse(responseFrame);
  } catch {
    return null;
  }
  if (frame?.result?.isError !== true) return null;
  const text = frame?.result?.content?.[0]?.text;
  if (typeof text !== "string") return null;
  let payload;
  try {
    payload = JSON.parse(text);
  } catch {
    return null;
  }
  if (payload?.code !== SEGMENT_NOT_LOADED) return null;
  const { tool, segment, segment_tools: segmentTools } = payload;
  if (typeof tool !== "string" || tool.length === 0) return null;
  if (typeof segment !== "string" || segment.length === 0) return null;
  if (!Array.isArray(segmentTools)) return null;
  if (!segmentTools.every((name) => typeof name === "string")) return null;
  return { tool, segment, segmentTools };
}

// Dispatch ONE frame with demand loading.
//
// The core engine answers directly whenever it can. When it returns the deferral signal,
// this loads the segment that serves the named tool and re-sends the IDENTICAL frame
// string to it — so a deferred tool costs the caller latency, never capability. That is the
// whole contract of the tiered console, implemented once here rather than at each call
// site.
//
// It is not, however, a promise that nothing can fail. `loadSegment` is a dynamic import
// and an unreachable segment REJECTS; a re-init that supersedes this frame's session
// rejects too. Both propagate: a routing step that cannot complete is a hard failure the
// caller must see, and answering anyway — from the core tier, or from another session's
// bundle — would be the degradation the tiering exists to avoid.
//
// `loadSegment(segmentName)` is supplied by the host and must resolve to a module with the
// same `{ ready, init, mcp }` lifecycle this one has (`gmeow-mcp-wasm` for the `reasoning`
// segment). It is called at most once per segment: the resolved module is cached, and the
// snapshot handed to `initTiered` is installed into it on first use, so a segment load
// costs one fetch and one bundle import, not one per deferred call.
//
// `onSegmentLoad` is an optional progress hook — the seam a UI uses to show that the
// answer is coming from a segment being fetched RIGHT NOW. Deferral must be visible as a
// loading state; a silent multi-second stall would be its own kind of degradation.
const _segments = new Map();
let _snapshot = null;
// The SESSION counter. `initTiered` is a new session, and a segment load is an async
// window that can straddle one: the loader awaits a dynamic import, then a wasm
// instantiation, then installs a snapshot — seconds during which the page may re-init over
// a different bundle. Without a generation stamp the in-flight closure read the
// module-global `_snapshot` AFTER its awaits (so it installed the NEW bundle into a segment
// the OLD session asked for) and then wrote itself into the freshly-cleared `_segments`
// map, where the new session would answer from it. Every load below captures BOTH the
// generation and the snapshot it belongs to, and a completion whose generation has been
// superseded is discarded rather than published.
let _generation = 0;

export function initTiered(snapshotBytes) {
  _generation += 1;
  // Retained because a segment loaded later must be initialised over the SAME bundle —
  // answering a re-dispatched frame from a different snapshot would be a mixed view.
  _snapshot = snapshotBytes;
  _segments.clear();
  init(snapshotBytes);
}

// Raised when a session boundary crossed an in-flight frame. NOT a degraded answer and not
// a retry: the frame was dispatched against a bundle this module no longer serves, and
// answering it from the new one would silently substitute a different question's answer.
function supersededSession(tool) {
  return new Error(
    `the engine was re-initialised over a different gmeow.gts while the \`${tool}\` frame ` +
      "was in flight; that frame belongs to a session this module no longer serves — " +
      "re-dispatch it against the current snapshot",
  );
}

export async function tieredMcp(requestFrame, { loadSegment, onSegmentLoad } = {}) {
  // The session this frame belongs to, stamped BEFORE the first dispatch: everything below
  // is checked against it, so a re-init at any point is caught rather than raced.
  const generation = _generation;
  const snapshot = _snapshot;

  const first = mcp(requestFrame);
  const deferral = segmentDeferral(first);
  if (deferral === null) return first;

  if (typeof loadSegment !== "function") {
    throw new Error(
      `\`${deferral.tool}\` is served by the \`${deferral.segment}\` segment; ` +
        "pass `loadSegment` to tieredMcp() so it can be fetched and the frame re-dispatched",
    );
  }
  if (snapshot === null) {
    throw new Error("no gmeow.gts snapshot retained — call initTiered(snapshotBytes) first");
  }

  let segment = _segments.get(deferral.segment);
  if (segment === undefined) {
    onSegmentLoad?.({ phase: "loading", ...deferral });
    const loading = (async () => {
      const module = await loadSegment(deferral.segment);
      await module.ready();
      // The snapshot CAPTURED at entry, never the module-global: this segment must be
      // installed over the bundle its frame was dispatched against.
      module.init(snapshot);
      return module;
    })();
    // Cache the in-flight PROMISE so two concurrent deferrals share one segment load.
    _segments.set(deferral.segment, loading);
    try {
      segment = await loading;
    } catch (error) {
      // Only evict what is still ours: a re-init already cleared the map, and deleting
      // blind could drop a load the NEW session started under the same key.
      if (_generation === generation && _segments.get(deferral.segment) === loading) {
        _segments.delete(deferral.segment);
      }
      throw error;
    }
    if (_generation !== generation) throw supersededSession(deferral.tool);
    _segments.set(deferral.segment, segment);
    onSegmentLoad?.({ phase: "loaded", ...deferral });
  } else {
    segment = await segment;
    if (_generation !== generation) throw supersededSession(deferral.tool);
  }

  // The IDENTICAL frame string, not a rebuild of it: the re-dispatch must be a replay.
  return segment.mcp(requestFrame);
}

export { init, loaded, mcp, version };
