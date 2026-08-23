// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// RDF-1.2: a quoted triple through every shipped target, and an honest ledger where it
// cannot go.
//
// The fixture is an annotated statement — `ex:statement rdf:reifies <<( s p o )>>` — which
// is the shape the console's own session annotations use, so this is not a synthetic corner
// of the grammar.
//
// Two claims, and the second is the one that matters. Where a target CAN carry a triple
// term, the term must survive parse → `convert` → re-parse. Where it structurally CANNOT
// (RDF/XML has no triple-term syntax; the JSON-LD 1.1 serializer rejects them), the target
// must RECORD the drop in its realized loss ledger, naming a representability code — a
// smaller graph returned without a word is the failure being ruled out.
//
// The target list is read off the console's own round-trip pane rather than restated here:
// the pane's verdict rows are the shipped set, so a target added to the console appears in
// this assertion with no edit.

import { expect, test } from "../lib/test.mjs";
import { ANNOTATED_RECORD } from "../lib/fixtures.mjs";

/** Count the triple terms in some N-Quads, using the console's own reader. */
async function tripleTerms(app, nquads) {
  return app.page.evaluate(async (text) => {
    const { parseNQuads } = await import("/assets/mcp-transport.mjs");
    return parseNQuads(text).filter(
      (quad) => quad.subject.kind === "triple" || quad.object.kind === "triple",
    ).length;
  }, nquads);
}

test("a quoted triple survives every target that can represent it, and is RECORDED where it cannot", async ({
  app,
}) => {
  // The shipped target set, taken from the console's own differential.
  const { rows } = await app.ask("roundtrip", {
    data: ANNOTATED_RECORD,
    from: "turtle",
    formats: ["turtle", "ntriples", "nquads", "trig", "rdfxml", "jsonld"],
  });
  expect(rows.length, "the differential must produce a verdict per target").toBe(6);

  const source = await tripleTerms(app, (await app.call("convert", {
    data: ANNOTATED_RECORD,
    from: "turtle",
    to: "nquads",
  })).output);
  expect(source, "the fixture itself must carry a quoted triple").toBeGreaterThan(0);

  const lossless = [];
  const lossy = [];
  for (const row of rows) {
    expect(row.ok, `${row.format} produced no verdict: ${row.error}`).toBe(true);
    // Re-parse: the target's own output, read BACK through the engine into canonical
    // N-Quads, is where a silently dropped term would show up.
    const back = await app.call("convert", { data: row.output, from: row.format, to: "nquads" });
    const survived = await tripleTerms(app, back.output);
    if (row.loss.length === 0) {
      expect(survived, `${row.format} reported no loss, so the quoted triple must survive`).toBe(source);
      lossless.push(row.format);
    } else {
      // A loss ledger is a claim about representability, and it must be the reason the term
      // is gone — never a note beside a term that is still there.
      const codes = row.loss.map((entry) => entry.code);
      expect(
        codes.some((code) => code.startsWith("rdf12-star")),
        `${row.format} dropped a quoted triple under code(s) ${codes.join(", ")}, which do not name star representability`,
      ).toBe(true);
      expect(survived, `${row.format} recorded a star loss but the term survived anyway`).toBe(0);
      for (const entry of row.loss) {
        expect(entry.count, `${row.format}'s ledger must count what it dropped`).toBeGreaterThan(0);
        expect(entry.note.length, `${row.format}'s ledger must say why`).toBeGreaterThan(0);
      }
      lossy.push(row.format);
    }
  }

  // Both halves are non-empty: a run where every target was lossless would prove nothing
  // about the ledger, and one where every target was lossy would prove nothing about
  // survival.
  expect(lossless, "the line-based RDF-1.2 syntaxes carry the term").toEqual([
    "turtle",
    "ntriples",
    "nquads",
    "trig",
  ]);
  expect(lossy, "the two syntaxes with no triple-term surface record the drop").toEqual([
    "rdfxml",
    "jsonld",
  ]);
});

test("the round-trip pane renders the verdicts and the derived loss lattice", async ({ app }) => {
  await app.selectPane("@roundtrip");
  await app.page.fill("gmeow-console textarea#rt-input", ANNOTATED_RECORD);
  await app.page.fill("gmeow-console input#rt-from", "turtle");
  await app.page.click('gmeow-console button.run[type="submit"]');

  await expect(app.page.locator("gmeow-console .status")).toContainText(
    "targets produced a verdict.",
    { timeout: 240_000 },
  );
  // One row per target, with the realized ledger rendered beside each verdict.
  await expect(app.page.locator("gmeow-console table tr")).toHaveCount(7); // header + six targets
  await expect(app.page.locator("gmeow-console")).toContainText("rdf12-star-unrepresentable");
  // The lattice is DERIVED from the run — drop-set inclusion over the realized ledgers.
  await expect(app.page.locator("gmeow-console h3")).toContainText([
    "Per-target verdicts",
    "Derived loss lattice (drop-set inclusion)",
  ]);
  await expect(app.page.locator("gmeow-console svg")).toHaveCount(1);
});
