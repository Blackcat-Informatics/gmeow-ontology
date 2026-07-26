// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// WASM half of the W1 native↔wasm validation parity WITNESS (T1/F1).
//
// Runs the REAL compiled gmeow-validate-wasm `validate()` over the SAME
// (counter-example, bundle) inputs the native half uses, and asserts the findings
// JSON is BYTE-IDENTICAL to the committed attestation the native Rust test blessed
// (crates/docs/assets/validate/WITNESS.validate.json). Native == attestation (the
// Rust test) AND wasm == attestation (this test) ⇒ native ≡ wasm: the in-browser
// validate button runs exactly the on-gate Tier-1 validator, proven, not asserted.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { ready, validate } from "../index.mjs";

await ready();

const root = new URL("../../../../", import.meta.url);
const NS = "https://blackcatinformatics.ca/gmeow/";
const COUNTER_EXAMPLE =
  "slices/extensions/embedding-projection/tests/counter-examples/ce-cross-space-rejected.ttl";

test("wasm Tier-1 validation is byte-identical to the native witness attestation", async () => {
  const bundle = new Uint8Array(
    await readFile(fileURLToPath(new URL("generated/dist/gmeow.gts", root))),
  );
  const turtle = await readFile(fileURLToPath(new URL(COUNTER_EXAMPLE, root)), "utf8");
  const attestation = await readFile(
    fileURLToPath(new URL("crates/docs/assets/validate/WITNESS.validate.json", root)),
    "utf8",
  );

  const wasmFindings = validate(turtle, "turtle", bundle, NS, COUNTER_EXAMPLE);

  // Byte-identical to the committed, native-blessed attestation — the parity proof.
  assert.equal(
    wasmFindings,
    attestation,
    "wasm validate() findings drifted from the native witness attestation — the " +
      "in-browser validator no longer matches the on-gate authority",
  );
});
