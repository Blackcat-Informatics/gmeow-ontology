// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// WASM half of the W4c GMN transcode parity WITNESS (T1).
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { ready, to_gmn1, from_gmn1, version } from "../index.mjs";

await ready();

// The SAME fixed GMEOW-namespace input the native witness pins
// (crates/gmn-wasm/tests/witness_gmn.rs): every term resolves through the embedded
// codebook, so the GMN-1 surface is self-contained and reads back from raw text.
const INPUT =
  "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n" +
  "gmeow:gate1 gmeow:hasState gmeow:doorGate1 .\n" +
  "gmeow:gate1 gmeow:locatedIn gmeow:yardNorth .\n" +
  'gmeow:gate1 gmeow:statusLabel "open" .\n';

test("version() returns the crate semver", () => {
  assert.match(version(), /^\d+\.\d+\.\d+/);
});

test("wasm GMN-1 transcode is byte-identical to the native witness attestation", async () => {
  const attestation = await readFile(
    fileURLToPath(new URL("../../tests/WITNESS.gmn1.txt", import.meta.url)),
    "utf8",
  );
  const gmn1 = to_gmn1(INPUT, "turtle");
  assert.equal(gmn1, attestation, "wasm GMN-1 transcode drifted from native attestation");
});

test("wasm GMN-1 round-trip reproduces the input canonical N-Quads", () => {
  const gmn1 = to_gmn1(INPUT, "turtle");
  const back = from_gmn1(gmn1);
  // Re-encoding the decoded N-Quads must yield the identical GMN-1 surface — the
  // round-trip is a fixed point (the native witness pins the byte-exact N-Quads side).
  assert.equal(to_gmn1(back, "nquads"), gmn1, "GMN-1 round-trip is not a fixed point");
});
