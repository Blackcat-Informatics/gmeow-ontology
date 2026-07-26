// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// WASM half of the W4 conjecture-playground parity WITNESS (symmetric proof / counterproof).
// Runs the wasm `conjecture()` over the SAME two curated inputs the native
// `tests/witness_conjecture.rs` pins, joins the two verdict bodies with the SAME delimiter,
// and asserts byte-identity with the committed native attestation. Both matching the one
// attestation proves native ≡ wasm for the conjecture engine.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { ready, conjecture } from "../index.mjs";

await ready();

const STANDPOINT =
  "https://blackcatinformatics.ca/gmeow/examples/conjecture/demo-standpoint";

// Demo 1 — the PROOF leg (corroborated): a reified ground atom the KB already asserts.
const PROOF_FORMULA =
  "@prefix logic: <https://blackcatinformatics.ca/logic/> .\n" +
  "@prefix ex:  <http://ex/> .\n" +
  "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n" +
  "ex:phi a logic:Formula ;\n" +
  "    logic:relation rdf:type ;\n" +
  "    logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ;\n" +
  "    logic:argument [ logic:termIndex 1 ; logic:termIri ex:B ] .\n";

const PROOF_KB =
  "@prefix ex:  <http://ex/> .\n" +
  "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n" +
  "ex:a rdf:type ex:B .\n";

// Demo 2 — the COUNTERPROOF leg (refuted-in-standpoint, with witness): a universally
// quantified Horn candidate whose head class is disjoint with the triggered individual's type.
const REFUTE_FORMULA =
  "@prefix logic: <https://blackcatinformatics.ca/logic/> .\n" +
  "@prefix ex:  <http://ex/> .\n" +
  "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n" +
  "ex:cand a logic:Formula ;\n" +
  "    logic:forall ex:body ;\n" +
  '    logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable "x" ] .\n' +
  "ex:body a logic:Formula ;\n" +
  "    logic:antecedent ex:ant ;\n" +
  "    logic:consequent ex:con .\n" +
  "ex:ant a logic:Formula ;\n" +
  "    logic:relation ex:trigger ;\n" +
  '    logic:argument [ logic:termIndex 0 ; logic:termVariable "x" ] ;\n' +
  "    logic:argument [ logic:termIndex 1 ; logic:termIri ex:mark ] .\n" +
  "ex:con a logic:Formula ;\n" +
  "    logic:relation rdf:type ;\n" +
  '    logic:argument [ logic:termIndex 0 ; logic:termVariable "x" ] ;\n' +
  "    logic:argument [ logic:termIndex 1 ; logic:termIri ex:B ] .\n";

const REFUTE_KB =
  "@prefix ex:  <http://ex/> .\n" +
  "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n" +
  "@prefix owl: <http://www.w3.org/2002/07/owl#> .\n" +
  "ex:a ex:trigger ex:mark .\n" +
  "ex:a rdf:type ex:A .\n" +
  "ex:A owl:disjointWith ex:B .\n";

// MUST match `DELIM` in tests/witness_conjecture.rs byte-for-byte.
const DELIM =
  "# ── conjecture witness · counterproof leg ──────────────────────────────\n";

test("wasm conjecture verdicts are byte-identical to the native witness attestation", async () => {
  const attestation = await readFile(
    fileURLToPath(new URL("../../../docs/assets/reason/WITNESS.conjecture.nq", import.meta.url)),
    "utf8",
  );
  const proof = conjecture(PROOF_KB, "turtle", PROOF_FORMULA, STANDPOINT);
  const refute = conjecture(REFUTE_KB, "turtle", REFUTE_FORMULA, STANDPOINT);
  const bundle = proof + DELIM + refute;
  assert.equal(bundle, attestation, "wasm conjecture verdicts drifted from native attestation");
});
