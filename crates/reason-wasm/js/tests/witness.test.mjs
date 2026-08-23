// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// WASM half of the W4b reasoner parity WITNESS (T1/F3).
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { ready, reason, version } from "../index.mjs";

await ready();

const INPUT =
  "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n" +
  "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n" +
  "@prefix ex: <https://example.org/> .\n" +
  "ex:Cat rdfs:subClassOf ex:Animal .\n" +
  "ex:Animal rdfs:subClassOf ex:Organism .\n" +
  "ex:felix rdf:type ex:Cat .\n";

// The published manifest, read from the shipped bytes — never a literal restated here.
const packageJson = JSON.parse(
  await readFile(fileURLToPath(new URL("../package.json", import.meta.url)), "utf8"),
);

test("version() equals the published package version", () => {
  assert.equal(version(), packageJson.version);
});

test("wasm reasoned closure is byte-identical to the native witness attestation", async () => {
  const attestation = await readFile(
    fileURLToPath(new URL("../../tests/WITNESS.reason.nq", import.meta.url)),
    "utf8",
  );
  const wasmClosure = reason(INPUT, "turtle");
  assert.equal(wasmClosure, attestation, "wasm reasoned closure drifted from native attestation");
});
