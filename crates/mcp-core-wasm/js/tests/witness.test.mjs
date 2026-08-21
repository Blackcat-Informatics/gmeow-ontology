// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// WASM half of the LEAN-CORE engine's native↔wasm parity WITNESS.
//
// Drives the SHIPPED wasm through the same lifecycle a browser uses — hand the gmeow.gts
// snapshot over once via `init`, then drive frames with `mcp` — over the SAME requests the
// native half (`crates/mcp-core-wasm/tests/witness_core.rs`) pins, and asserts the response
// frames are byte-identical to the committed attestations. Both halves matching the one
// attestation is the proof that native ≡ wasm.
//
// It also drives the DEMAND LOADER end to end: `tieredMcp` must turn the deferral signal
// into a real answer from the reasoning segment, with the frame replayed byte-for-byte.
// That is the claim the tiering rests on, so it is executed rather than described.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import {
  SEGMENT_NOT_LOADED,
  deferredSegment,
  deferredSegmentFor,
  deferredSegmentTools,
  deferredTools,
  init,
  initTiered,
  loaded,
  mcp,
  ready,
  segmentDeferral,
  tieredMcp,
  version,
} from "../index.mjs";

await ready();

// Byte-for-byte the `CORE_REQUEST` / `DEFERRED_REQUEST` constants of the native half.
const CORE_REQUEST =
  '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"convert",' +
  '"arguments":{"data":"<http://example.org/r> <http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies> ' +
  "<<( <http://example.org/s> <http://example.org/p> <http://example.org/o> )>> .\\n\"," +
  '"from":"nt","to":"turtle"}}}';

const DEFERRED_TOOL = "recall";
const DEFERRED_REQUEST =
  '{"jsonrpc":"2.0","id":2,"method":"tools/call",' +
  '"params":{"name":"recall","arguments":{"query":"anything"}}}';

const TOOLS_LIST = '{"jsonrpc":"2.0","id":3,"method":"tools/list","params":{}}';

const snapshot = await readFile(
  fileURLToPath(new URL("../../../../generated/dist/gmeow.gts", import.meta.url)),
);

// The published manifest, read from the shipped bytes — never a literal restated here.
const packageJson = JSON.parse(
  await readFile(fileURLToPath(new URL("../package.json", import.meta.url)), "utf8"),
);

test("version() equals the published package version", () => {
  assert.equal(version(), packageJson.version);
});

test("mcp() refuses frames before a snapshot is loaded", () => {
  assert.equal(loaded(), false, "no snapshot is loaded before init");
  assert.throws(() => mcp(CORE_REQUEST), "a frame sent before init must be refused");
});

test("the lean core advertises the WHOLE 38-tool surface", () => {
  init(snapshot);
  assert.equal(loaded(), true, "init installs the engine");
  const listed = JSON.parse(mcp(TOOLS_LIST)).result.tools.map((t) => t.name);
  assert.equal(listed.length, 38, "deferral must be invisible to discovery");
  for (const tool of deferredTools()) {
    assert.ok(listed.includes(tool), `deferred tool ${tool} is still advertised`);
  }
});

test("wasm core response frame is byte-identical to the native witness attestation", async () => {
  const attestation = await readFile(
    fileURLToPath(new URL("../../tests/WITNESS.core.json", import.meta.url)),
    "utf8",
  );
  init(snapshot);
  assert.equal(mcp(CORE_REQUEST), attestation, "wasm core frame drifted from native attestation");
});

test("wasm deferral signal is byte-identical to the native witness attestation", async () => {
  const attestation = await readFile(
    fileURLToPath(new URL("../../tests/WITNESS.core-deferral.json", import.meta.url)),
    "utf8",
  );
  init(snapshot);
  const frame = mcp(DEFERRED_REQUEST);
  assert.equal(frame, attestation, "wasm deferral frame drifted from native attestation");

  const deferral = segmentDeferral(frame);
  assert.notEqual(deferral, null, "the frame is recognised as a deferral, structurally");
  assert.equal(deferral.tool, DEFERRED_TOOL);
  assert.equal(deferral.segment, deferredSegmentFor(deferral.tool));
  // The signal carries the tools of the segment it NAMED — one tier — while `deferredTools()`
  // is everything this image defers across both. The first is what a host loading that module
  // gets; the second is what the image cannot answer at all.
  assert.deepEqual(deferral.segmentTools, deferredSegmentTools(deferral.segment));
  for (const tool of deferral.segmentTools) {
    assert.ok(deferredTools().includes(tool), `${tool} is deferred by this image`);
  }
  assert.equal(JSON.parse(JSON.parse(frame).result.content[0].text).code, SEGMENT_NOT_LOADED);
});

test("a core frame is NOT mistaken for a deferral", () => {
  init(snapshot);
  assert.equal(segmentDeferral(mcp(CORE_REQUEST)), null, "a real answer is not a routing signal");
});

test("tieredMcp demand-loads the reasoning segment and replays the identical frame", async () => {
  initTiered(snapshot);

  // The frame the lean image cannot answer, dispatched through the tiered path with the
  // REAL full segment as the loader. Nothing is stubbed: the answer comes from
  // gmeow-mcp-wasm, over the same snapshot, driven by the same bytes.
  const loads = [];
  const answer = await tieredMcp(DEFERRED_REQUEST, {
    loadSegment: async (name) => {
      loads.push(name);
      return await import("../../../mcp-wasm/js/index.mjs");
    },
    onSegmentLoad: (event) => loads.push(event.phase),
  });

  assert.deepEqual(loads, ["loading", deferredSegmentFor(DEFERRED_TOOL), "loaded"], "the segment load is visible");
  assert.equal(segmentDeferral(answer), null, "the re-dispatched frame produced a real answer");
  const result = JSON.parse(answer).result;
  assert.equal(result.isError, false, `the replay answered for real: ${answer}`);
  assert.equal(JSON.parse(result.content[0].text).ok, true, `the payload is real: ${answer}`);

  // The SAME frame sent straight to the full segment must give the SAME bytes: the tier a
  // frame travelled through is not observable in the answer.
  const full = await import("../../../mcp-wasm/js/index.mjs");
  assert.equal(full.mcp(DEFERRED_REQUEST), answer, "re-dispatch is lossless");

  // A second deferred call must reuse the loaded segment rather than fetching it again.
  loads.length = 0;
  await tieredMcp(DEFERRED_REQUEST, {
    loadSegment: async () => {
      throw new Error("the segment must be loaded at most once");
    },
    onSegmentLoad: () => loads.push("reload"),
  });
  assert.deepEqual(loads, [], "the loaded segment is cached");
});

test("tieredMcp answers a core frame without loading anything", async () => {
  initTiered(snapshot);
  const answer = await tieredMcp(CORE_REQUEST, {
    loadSegment: async () => {
      throw new Error("a core frame must never trigger a segment load");
    },
  });
  assert.equal(JSON.parse(answer).result.isError, false, "the core tier answered directly");
});

// ── a failed re-init must evict, not fall back to the previous session ───────────────
//
// Asserted HERE rather than in the native witness because the failure path is a thrown
// `JsError`, and constructing one calls a wasm-bindgen imported function — which panics by
// design off wasm. The refusal is only real in the real image.
test("a failed re-init leaves no engine rather than the previous bundle", () => {
  init(snapshot);
  assert.equal(loaded(), true, "the first load installed an engine");

  // The defect: `init` used to build the replacement server FIRST and touch the engine slot
  // only on success, so a failed second load returned early and left the previous session's
  // bundle installed. `loaded()` stayed `true` and `mcp` kept answering — from another
  // bundle's data, under the new bundle's name. The docs claimed the opposite.
  assert.throws(
    () => init(new Uint8Array([0x6e, 0x6f, 0x70, 0x65])),
    "a non-bundle must not load",
  );
  assert.equal(
    loaded(),
    false,
    "a FAILED re-init must leave NO engine installed — reporting ready here means the " +
      "previous session's bundle is still serving",
  );
  assert.throws(() => mcp(CORE_REQUEST), "and the engine refuses frames rather than answering");

  // The eviction is an eviction, not a corrupted slot: a good bundle re-installs.
  init(snapshot);
  assert.equal(loaded(), true, "a good bundle re-installs after a failed load");
});

// ── the grounded-memory triad, across the two REAL images ────────────────────────────
//
// The defect this pins was live on the shipped console: `store_claim` is served by the
// reasoning image and `recall` / `store_segment` were served by the core image, and each
// wasm module owns its own claim store — two linear memories, two stores. A user stored a
// claim, got `ok: true` and a minted id back, and then `recall` answered `[]` and
// `store_segment` reported `0/0`. The write was unreachable by every read.
//
// It can only be observed HERE. Natively there is one process and one store, so the native
// suite cannot reproduce it; the routing invariant that prevents it is asserted in Rust
// (`the_grounded_memory_triad_is_served_by_one_segment`) and its CONSEQUENCE is asserted
// here, against the two images the browser actually loads, driven exactly as the console
// drives them: one `tieredMcp` dispatcher, one snapshot, one demand-loaded segment.
const reasoningSegment = async () => await import("../../../mcp-wasm/js/index.mjs");

const toolFrame = (id, name, args) =>
  JSON.stringify({ jsonrpc: "2.0", id, method: "tools/call", params: { name, arguments: args } });

test("the grounded-memory triad reads back what it wrote, across the segment boundary", async () => {
  initTiered(snapshot);

  const call = async (name, args) => {
    const frame = await tieredMcp(toolFrame(9, name, args), { loadSegment: reasoningSegment });
    const result = JSON.parse(frame).result;
    assert.equal(result.isError, false, `${name} did not answer: ${frame}`);
    return JSON.parse(result.content[0].text);
  };

  const text = "the launch window closes on the 14th";
  const stored = await call("store_claim", { text });
  assert.equal(stored.ok, true, "the write reports success");
  const claimId = stored.claim.id;
  assert.equal(typeof claimId, "string", "the write mints a claim id");

  // THE assertion: the read sees the write. Before the triad was made indivisible this
  // returned an empty list, because the read ran in the other image.
  const recalled = await call("recall", { query: "launch window" });
  assert.equal(recalled.ok, true);
  assert.ok(
    recalled.claims.some((claim) => claim.id === claimId),
    `recall must return the claim store_claim just minted, got ${JSON.stringify(recalled)}`,
  );

  // …and the SERIALIZATION the console exports a session with sees it too.
  const exported = await call("store_segment", {});
  assert.equal(exported.ok, true);
  assert.ok(exported.claim_count >= 1, `store_segment must report the stored claim: ${JSON.stringify(exported)}`);
  assert.ok(
    exported.nquads.includes(claimId),
    "the exported session store carries the claim that was stored",
  );
});

// ── the deferral envelope is validated before it is routed on ────────────────────────
test("segmentDeferral refuses anything that is not a complete routing instruction", () => {
  const envelope = (payload, isError = true) =>
    JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      result: { isError, content: [{ type: "text", text: JSON.stringify(payload) }] },
    });
  const complete = {
    ok: false,
    code: SEGMENT_NOT_LOADED,
    tool: "coherence_certificate",
    segment: "reasoning",
    segment_tools: ["coherence_certificate"],
  };

  assert.deepEqual(
    segmentDeferral(envelope(complete)),
    { tool: "coherence_certificate", segment: "reasoning", segmentTools: ["coherence_certificate"] },
    "a complete signal is recognised",
  );

  // Every field the host routes on is REQUIRED. `loadSegment(undefined)` was reachable
  // because only `code` was ever checked.
  assert.equal(segmentDeferral(envelope(complete, false)), null, "a success envelope is not a deferral");
  assert.equal(segmentDeferral(envelope({ ...complete, segment: undefined })), null, "no segment");
  assert.equal(segmentDeferral(envelope({ ...complete, segment: "" })), null, "empty segment");
  assert.equal(segmentDeferral(envelope({ ...complete, segment: 7 })), null, "non-string segment");
  assert.equal(segmentDeferral(envelope({ ...complete, tool: undefined })), null, "no tool");
  assert.equal(segmentDeferral(envelope({ ...complete, tool: 7 })), null, "non-string tool");
  assert.equal(
    segmentDeferral(envelope({ ...complete, segment_tools: undefined })),
    null,
    "no segment inventory",
  );
  assert.equal(
    segmentDeferral(envelope({ ...complete, segment_tools: "coherence_certificate" })),
    null,
    "a string is not an inventory",
  );
  assert.equal(
    segmentDeferral(envelope({ ...complete, segment_tools: [1, 2] })),
    null,
    "an inventory of non-strings is not an inventory",
  );
  assert.equal(segmentDeferral(envelope({ ...complete, code: "mcp.unknown-tool" })), null, "other code");
});

// ── a session boundary must not be crossed by an in-flight segment load ──────────────
test("a re-init during an in-flight segment load is never answered from the old session", async () => {
  initTiered(snapshot);

  // Hold the segment load open, exactly as a slow multi-megabyte fetch would.
  let release;
  const held = new Promise((resolve) => {
    release = resolve;
  });
  const inFlight = tieredMcp(DEFERRED_REQUEST, {
    loadSegment: async () => {
      await held;
      return await reasoningSegment();
    },
  });

  // A NEW session starts while that load is pending. The pending frame was dispatched
  // against the previous bundle; answering it now would answer a different question.
  initTiered(snapshot);
  release();

  await assert.rejects(
    inFlight,
    /re-initialised over a different gmeow\.gts/,
    "a frame that straddles a session boundary must reject, not answer",
  );

  // …and the superseded load must NOT have published itself into the new session's cache.
  // That write-back is the actual defect: the stale closure re-inserted itself into the
  // freshly-cleared segment map, and the new session then answered from it.
  await assert.rejects(
    tieredMcp(DEFERRED_REQUEST, {
      loadSegment: async () => {
        throw new Error("the new session must load its own segment");
      },
    }),
    /the new session must load its own segment/,
    "the new session's segment cache must be empty",
  );
});
