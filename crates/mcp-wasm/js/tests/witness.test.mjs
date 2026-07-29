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

// Byte-for-byte the `REQUEST` constant of the native half. A REASONING-segment tool,
// because this image IS the demand-loaded reasoning segment: the frame runs the native
// structured-DL conjecture engine over the KB carried in the request, so the attestation
// pins an ANSWER rather than the `mcp.segment-not-loaded` a core tool would return here.
const REQUEST =
  '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"conjecture_test",' +
  '"arguments":{"formula":"@prefix logic: <https://blackcatinformatics.ca/logic/> .\\n' +
  "@prefix ex: <http://ex/> .\\n" +
  "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\\n" +
  "ex:phi a logic:Formula ;\\n" +
  "    logic:relation rdf:type ;\\n" +
  "    logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ;\\n" +
  '    logic:argument [ logic:termIndex 1 ; logic:termIri ex:B ] .\\n",' +
  '"kb":"@prefix ex: <http://ex/> .\\n' +
  "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\\n" +
  'ex:a rdf:type ex:B .\\n",' +
  '"standpoint":"https://blackcatinformatics.ca/gmeow/examples/conjecture/demo-standpoint"}}}';

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
  // A bare `assert.throws(fn)` accepts ANY error — a typo in `REQUEST`, a broken import,
  // a panic inside the engine — and would pass while proving nothing about the refusal.
  // Pin the exact contract: the refusal names the missing snapshot AND the call that
  // supplies it.
  assert.throws(
    () => mcp(REQUEST),
    (error) => {
      assert.match(
        String(error.message ?? error),
        /no gmeow\.gts snapshot loaded — call init\(snapshotBytes\) before mcp\(frame\)/,
        "the pre-init refusal must name the missing snapshot and the init call that loads it",
      );
      return true;
    },
    "a frame sent before init must be refused",
  );
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
