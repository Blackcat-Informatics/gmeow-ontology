// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// WASM half of the MCP-engine native↔wasm parity WITNESS.
//
// Drives the SHIPPED wasm through the same lifecycle a browser uses — hand the
// gmeow.gts snapshot over once via `init`, then drive frames with `mcp` — over the SAME
// request the native half (`crates/mcp-wasm/tests/witness_mcp.rs`) pins, and asserts the
// response frame is byte-identical to the committed attestation. Both halves matching
// the one attestation is the proof that native ≡ wasm.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { init, loaded, mcp, ready, version } from "../index.mjs";

await ready();

// Byte-for-byte the `REQUEST` constant of the native half.
const REQUEST =
  '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"convert",' +
  '"arguments":{"data":"<http://example.org/r> <http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies> ' +
  "<<( <http://example.org/s> <http://example.org/p> <http://example.org/o> )>> .\\n\"," +
  '"from":"nt","to":"turtle"}}}';

const snapshot = await readFile(
  fileURLToPath(new URL("../../../../generated/dist/gmeow.gts", import.meta.url)),
);

test("version() returns the crate semver", () => {
  assert.match(version(), /^\d+\.\d+\.\d+/);
});

test("mcp() refuses frames before a snapshot is loaded", () => {
  assert.equal(loaded(), false, "no snapshot is loaded before init");
  assert.throws(() => mcp(REQUEST), "a frame sent before init must be refused");
});

test("wasm MCP response frame is byte-identical to the native witness attestation", async () => {
  const attestation = await readFile(
    fileURLToPath(new URL("../../tests/WITNESS.mcp.json", import.meta.url)),
    "utf8",
  );
  init(snapshot);
  assert.equal(loaded(), true, "init installs the engine");
  const frame = mcp(REQUEST);
  assert.equal(frame, attestation, "wasm MCP response frame drifted from native attestation");
});
