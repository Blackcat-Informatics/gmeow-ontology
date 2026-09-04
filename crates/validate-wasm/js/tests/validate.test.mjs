// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// Node real-execution demo for the gmeow-validate-wasm package: drives the ACTUAL
// compiled wasm through the public `validate()` surface, validating authored GMEOW
// RDF against the REAL committed `gmeow.gts` bundle — proving client-side Tier-1
// conformance works with no repository, server, or Docker.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { ready, validate, version } from "../index.mjs";

// One-time wasm instantiation before any test runs.
await ready();

// The shipped bundle carrying the SHACL shapes (REP_SHAPES) the validator enforces.
const BUNDLE = new Uint8Array(
  await readFile(
    fileURLToPath(new URL("../../../../generated/dist/gmeow.gts", import.meta.url)),
  ),
);

const GMEOW_NS = "https://blackcatinformatics.ca/gmeow/";
const PREFIXES =
  "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n" +
  "@prefix ex: <https://example.org/> .\n";

/** Count error-severity findings in a validate() JSON report. */
function errorFindings(json) {
  const report = JSON.parse(json);
  return (report.findings ?? []).filter((f) => f.severity === "error");
}

// The published manifest, read from the shipped bytes — never a literal restated here.
const packageJson = JSON.parse(
  await readFile(fileURLToPath(new URL("../package.json", import.meta.url)), "utf8"),
);

test("version() equals the published package version", () => {
  assert.equal(version(), packageJson.version);
});

test("a conforming instance validates clean against the real bundle", () => {
  const json = validate(
    `${PREFIXES}ex:ann a gmeow:Person .\n`,
    "turtle",
    BUNDLE,
    GMEOW_NS,
    "clean.ttl",
  );
  assert.equal(
    errorFindings(json).length,
    0,
    `a well-formed gmeow:Person must yield zero error findings: ${json}`,
  );
});

test("a bare Entity surfaces authored advice through the real wasm bundle", () => {
  const json = validate(
    `${PREFIXES}ex:bare a gmeow:Entity .\n`,
    "turtle",
    BUNDLE,
    GMEOW_NS,
    "bare-entity.ttl",
  );
  const findings = JSON.parse(json).findings ?? [];
  const advice = findings.find(
    (finding) =>
      finding.code?.startsWith("advice.") &&
      finding.suggestions?.includes(
        "Type each instance with its most specific sortal (Agent, InformationObject, PhysicalObject, SocialObject) and let it inherit Entity; attach the universal facets here because they range over the whole sortal spine.",
      ) &&
      finding.suggestions?.includes(
        "Use when: Use as the universal domain when a property, facet, or claim must attach to anything that persists and can bear properties — the anchor for the domain-free facets (granularity, determinacy, sensitivity, consumer eligibility) that are defined Entity-wide.",
      ),
  );
  assert.ok(
    advice,
    `a bare gmeow:Entity must surface its authored howToUse and useWhen advice: ${json}`,
  );
  assert.equal(advice.severity, "note");
  assert.ok(
    !findings.some(
      (finding) =>
        finding.code?.startsWith("shacl.") && finding.severity === "info",
    ),
    `the raw shacl.* Info finding must be suppressed after advisory projection: ${json}`,
  );
});

test("a shape-violating instance yields an error finding", () => {
  // gmeow:ToolCallShape requires gmeow:usedTool (sh:minCount 1); a bare
  // gmeow:ToolCall violates it — a Tier-1 SHACL catch that needs no reasoner.
  const json = validate(
    `${PREFIXES}ex:tc a gmeow:ToolCall .\n`,
    "turtle",
    BUNDLE,
    GMEOW_NS,
    "invalid.ttl",
  );
  const errors = errorFindings(json);
  assert.ok(
    errors.length >= 1,
    `a gmeow:ToolCall missing gmeow:usedTool must yield >=1 error finding: ${json}`,
  );
});

test("a malformed bundle throws rather than validating silently", () => {
  // No shapes-archive → the validator hard-fails; the wasm boundary surfaces it as
  // a thrown exception, not a silent pass.
  assert.throws(() =>
    validate(
      `${PREFIXES}ex:ann a gmeow:Person .\n`,
      "turtle",
      new Uint8Array([0, 1, 2, 3]),
      GMEOW_NS,
      "clean.ttl",
    ),
  );
});
