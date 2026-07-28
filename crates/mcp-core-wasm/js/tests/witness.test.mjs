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

const DEFERRED_REQUEST =
  '{"jsonrpc":"2.0","id":2,"method":"tools/call",' +
  '"params":{"name":"coherence_certificate","arguments":{}}}';

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

test("the lean core advertises the WHOLE 37-tool surface", () => {
  init(snapshot);
  assert.equal(loaded(), true, "init installs the engine");
  const listed = JSON.parse(mcp(TOOLS_LIST)).result.tools.map((t) => t.name);
  assert.equal(listed.length, 37, "deferral must be invisible to discovery");
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
  assert.equal(deferral.tool, "coherence_certificate");
  assert.equal(deferral.segment, deferredSegment());
  assert.deepEqual(deferral.segmentTools, deferredTools());
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

  assert.deepEqual(loads, ["loading", deferredSegment(), "loaded"], "the segment load is visible");
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
