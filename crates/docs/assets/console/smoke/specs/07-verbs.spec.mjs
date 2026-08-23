// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// The four verbs plus the two the console adds, each on a real document, in the browser.
//
//   * Tier-1 validation returns a conformance verdict with its findings;
//   * the reasoner returns the entailed triple;
//   * a conjecture returns its Belnap lifecycle verdict, with the contradiction witness;
//   * GMN 0 → 1 → 0 is a FIXED POINT on codebook-covered input;
//   * malformed input THROWS rather than being read as an empty document.
//
// The last one is the load-bearing half: a parser that answers "no findings" for input it
// could not read is worse than one that refuses, because the reader is told their document
// is fine.

import { expect, test } from "../lib/test.mjs";
import {
  ANNOTATED_RECORD,
  CODEBOOK_COVERED,
  CONJECTURE_FORMULA,
  CONJECTURE_KB,
  CONJECTURE_STANDPOINT,
  SUBSUMPTION,
  SUBSUMPTION_ENTAILMENT,
} from "../lib/fixtures.mjs";

test("Tier-1 validation answers with a conformance verdict over a real document", async ({ app }) => {
  const conformant = await app.call("validate_local", { data: ANNOTATED_RECORD, format: "turtle" });
  expect(conformant.tool).toBe("validate");
  expect(Array.isArray(conformant.findings), "a verdict carries a findings ledger").toBe(true);

  // A NEGATIVE verdict is still a successful call — the distinction the transport is
  // written around, and the one that made the site's validation buttons report a tool
  // failure instead of rendering the findings they pointed at.
  const counterExample = `@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/gmeow/console/> .
ex:call a gmeow:ToolCall .
`;
  //
  // The assertions below are UNCONDITIONAL on purpose. `ex:call a gmeow:ToolCall .` is a
  // ToolCall with none of the properties a ToolCall must carry, so Tier-1 has something to
  // say about it; a spec that only asked whether `ok` was a boolean, or that guarded its
  // finding checks on `findings.length > 0`, would pass unchanged against a validator that
  // reported EVERY document conformant — the precise failure the note above says matters.
  const verdict = await app.call("validate_local", { data: counterExample, format: "turtle" });
  expect(Array.isArray(verdict.findings)).toBe(true);
  expect(
    verdict.ok,
    `a document missing every required ToolCall property must NOT be conformant: ${JSON.stringify(
      verdict,
    ).slice(0, 400)}`,
  ).toBe(false);
  expect(verdict.findings.length, "…and the refusal must be itemised").toBeGreaterThan(0);
  for (const finding of verdict.findings) {
    expect(typeof finding.code, "every finding carries a code").toBe("string");
    expect(finding.code.length).toBeGreaterThan(0);
  }
});

test("the reasoner derives the entailed triple through the demand-loaded segment", async ({ app }) => {
  const closure = await app.call("reason_graph", { data: SUBSUMPTION, format: "turtle" });
  expect(closure.closure_nquads.trim().split("\n")).toContain(SUBSUMPTION_ENTAILMENT);

  // `verify_graph` is the second reasoning-segment verb: it must answer over the same input.
  // `verify_graph` is a tier of its own: it chases the governed bundle in UNION with whatever
  // it is given, which takes ~3.2 GiB to fold plus some 5.4 GiB to chase — measured, against
  // wasm32's hard 4 GiB ceiling. No input fits, so the browser routes instead of answering,
  // and the derivation above is what this deployment genuinely does.
  const attempt = await app.attempt("verify_graph", { data: SUBSUMPTION, format: "turtle" });
  expect(attempt.ok, "verify_graph cannot be answered by a 32-bit host").toBe(false);
  expect(attempt.error, "…and it says which tier does serve it").toContain("`chase` engine segment");
});

test("a conjecture returns its lifecycle verdict and its contradiction witness", async ({ app }) => {
  const answer = await app.call("conjecture_test", {
    formula: CONJECTURE_FORMULA,
    kb: CONJECTURE_KB,
    standpoint: CONJECTURE_STANDPOINT,
  });
  expect(answer.ok).toBe(true);
  // The candidate's head class is disjoint with the witness individual's asserted type, so
  // firing it clashes: the engine must REFUTE it in the standpoint, on a COMPLETE
  // evaluation (a budget-cut run could report the same lifecycle for the wrong reason),
  // and name the witness that made it clash.
  expect(answer.verdict.lifecycle, `the verdict must be a refutation: ${JSON.stringify(answer.verdict)}`).toBe(
    "refuted-in-standpoint",
  );
  expect(answer.verdict.information, "…on the opposed side of the Belnap square").toBe("opposed");
  expect(answer.verdict.evaluation, "…from an evaluation that ran to completion").toBe("completed");
  expect(answer.verdict.discharge, "…with its obligation discharged").toBe("ObligationDischarged");
  expect(answer.judgment_nquads.length, "the verdict projects as deterministic N-Quads").toBeGreaterThan(0);
  expect(answer.witness.individual, "the refutation names its witness individual").toBe(
    "https://example.org/gmeow/console/a",
  );
  expect(answer.witness.premises.length, "…and the premises that clashed").toBeGreaterThan(0);

  // The shipped conjecture library reads back structurally through the same engine.
  const corpus = await app.page.evaluate(async () => {
    const { callTool, conjectureLibrary } = await import("/assets/mcp-transport.mjs");
    const ttl = await (await fetch("/assets/conjectures.ttl", { cache: "no-store" })).text();
    const converted = await callTool("convert", { data: ttl, from: "turtle", to: "nquads" });
    return conjectureLibrary(converted.output).map((entry) => ({
      id: entry.id,
      label: entry.label,
      standpoint: entry.standpoint,
      lifecycle: entry.lifecycle,
    }));
  });
  expect(corpus.length, "the shipped corpus must declare conjectures").toBeGreaterThan(0);
  for (const entry of corpus) {
    expect(entry.label.length, `${entry.id} has no label`).toBeGreaterThan(0);
    expect(entry.standpoint, `${entry.id} names no standpoint`).not.toBeNull();
    expect(entry.lifecycle, `${entry.id} records no Belnap lifecycle`).not.toBeNull();
  }
});

test("GMN 0 → 1 → 0 is a fixed point, and a non-conformant document is refused", async ({ app }) => {
  const encoded = await app.call("encode_gmn1", { data: CODEBOOK_COVERED, format: "turtle" });
  expect(encoded.round_trip, "the encoder must report its own round trip").toBe(true);

  const expanded = await app.call("gmn_expand", { gmn: encoded.gmn1 });
  expect(expanded.ok).toBe(true);
  // 0 → 1 → 0: the expansion is byte-identical to the canonical N-Quads the encoder read.
  expect(expanded.expanded_nquads, "GMN-1 expansion must return the canonical N-Quads").toBe(
    encoded.canonical_nquads,
  );
  // …and 1 → 0 → 1: re-encoding the expansion returns the identical GMN-1 document.
  expect(expanded.reencoded_gmn, "re-encoding must return the identical GMN-1 document").toBe(
    encoded.gmn1,
  );
  expect(expanded.round_trip).toBe(true);

  const conformant = await app.call("gmn_validate", { gmn: encoded.gmn1 });
  expect(conformant.conformant).toBe(true);

  const refused = await app.call("gmn_validate", { gmn: "this is not a GMN-1 document" });
  expect(refused.conformant, "a non-conformant document must be refused").toBe(false);
  expect(refused.failure_local_name.length, "…by a NAMED failure class").toBeGreaterThan(0);
});

test("malformed input THROWS rather than being read as an empty document", async ({ app }) => {
  const malformed = [
    ["convert", { data: "<urn:a> <urn:b>", from: "turtle", to: "nquads" }],
    ["validate_local", { data: "@prefix ex: <", format: "turtle" }],
    ["reason_graph", { data: "ex:a ex:b ex:c .", format: "turtle" }],
    ["query_local", { data: "", format: "turtle", scope: "input", query: "SELECT WHERE {" }],
    ["gmn_expand", { gmn: "@gmn{v: 1" }],
  ];
  for (const [tool, args] of malformed) {
    const outcome = await app.attempt(tool, args);
    const failed =
      outcome.ok === false || (outcome.value !== null && outcome.value.ok === false);
    expect(
      failed,
      `${tool} accepted malformed input instead of refusing it: ${JSON.stringify(outcome).slice(0, 400)}`,
    ).toBe(true);
    const message = outcome.ok ? String(outcome.value.error) : outcome.error;
    expect(message.length, `${tool}'s refusal must say what it could not read`).toBeGreaterThan(0);
  }

  // The console's own reader is total on the same terms: a half-read term is REPORTED.
  const readerRefusals = await app.page.evaluate(async () => {
    const { parseNQuads } = await import("/assets/mcp-transport.mjs");
    const lines = [
      '<http://example.org/s> <http://example.org/p> "no closing quote .\n',
      "<http://example.org/s> <http://example.org/p> <http://example.org/o>\n",
      '<http://example.org/s> <http://example.org/p> <http://example.org/o> "not a graph" .\n',
    ];
    return lines.map((line) => {
      try {
        parseNQuads(line);
        return null;
      } catch (error) {
        return String(error.message);
      }
    });
  });
  for (const refusal of readerRefusals) {
    expect(refusal, "the shipped N-Quads reader must refuse a half-read term").not.toBeNull();
    expect(refusal.length).toBeGreaterThan(0);
  }
});
