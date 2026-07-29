// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// The console's named acceptance assertions, run under `node --test` against the SHIPPED
// wasm engine and the SHIPPED bundle — no browser, no mocks, no stubs.
//
// The file is in two halves. The first is the seven NAMED acceptance assertions, numbered
// in the section rules below. The second — "the shipped-runtime totality assertions" — is
// one test per shipped defect that contradicted the surface's own written contract: the
// N-Quads reader that documented itself as total, the session emitter that guessed term
// kinds and under-escaped literals, the re-grapher that emitted five-term lines, the
// worker dispatch that ran inherited methods, the element that hung for ever on a failed
// worker, and the manifest that promised a PWA install with no icon to install.
//
// Every one of them is a gate blocker: none is skipped, none is conditional, and the
// engine they drive is the same `crates/docs/assets/mcp-core/` image the site and the
// console load in a browser.
//
// The DOM-free modules (`session.mjs`, `../mcp-transport.mjs`) are imported directly. The
// three assertions that would otherwise need a browser are written as structural
// assertions over the same functions the browser calls, so every claim is EXECUTED here
// even before the Playwright lane exists.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import {
  GMEOW_NS,
  actionPolicyPanes,
  callTool,
  conjectureLibrary,
  configure,
  listTools,
  parseNQuads,
} from "../../mcp-transport.mjs";
import {
  ConsoleSession,
  SESSION_STORE_GRAPH,
  decodePermalink,
  exportSegment,
  storeReading,
} from "../session.mjs";
import { VIGNETTES } from "../examples/gallery.mjs";

const here = (rel) => fileURLToPath(new URL(rel, import.meta.url));

// Node has no `fetch` for `file:` URLs, so the transport's byte reader is pointed at the
// filesystem. This is the SAME seam the hard-error assertion perturbs.
configure({
  assetBase: new URL("../../", import.meta.url),
  fetchBytes: async (url) => new Uint8Array(await readFile(fileURLToPath(url))),
});

// The snapshot the browser fetches as `assets/gmeow.gts` is, in the repo, the built
// bundle. Symlinking it into `assets/` would be a second copy of a 37 MB file, so the
// reader resolves that one name to the real path instead.
const SNAPSHOT = here("../../../../../generated/dist/gmeow.gts");

// `assets/bundle-manifest.json` is emitted by the RENDERER, over the assets it emits — it
// is not a checked-in file, so `crates/docs/assets/` (a build-input tree, not a rendered
// site) has none. The reader below stands in for the renderer for the one asset this lane
// boots over, computing the same `{blake3, bytes}` entry the Rust side emits.
//
// That makes the POSITIVE direction a wiring check, not a proof — hashing bytes and then
// verifying them against their own hash proves nothing about the bytes. The proof is the
// NEGATIVE direction, in `integrity_hard_fails_on_a_tampered_snapshot` below: perturb
// either half and the boot must refuse. What this reader does establish is that the shipped
// `blake3.mjs` and the shipped verification path are actually reached on the boot route,
// which is what silently was not true before.
const manifestFor = async (path, sitePath) => {
  const { blake3Hex } = await import("../../blake3.mjs");
  const bytes = new Uint8Array(await readFile(path));
  return { [sitePath]: { blake3: `blake3:${blake3Hex(bytes)}`, bytes: bytes.length } };
};
const SNAPSHOT_MANIFEST = await manifestFor(SNAPSHOT, "assets/gmeow.gts");

const encodeJson = (value) => new TextEncoder().encode(JSON.stringify(value));

configure({
  fetchBytes: async (url) => {
    const name = url.toString();
    if (name.endsWith("/bundle-manifest.json")) return encodeJson(SNAPSHOT_MANIFEST);
    return new Uint8Array(await readFile(name.endsWith("/gmeow.gts") ? SNAPSHOT : fileURLToPath(url)));
  },
});

// ── 1 ───────────────────────────────────────────────────────────────────────

test("panes_are_derived_from_the_shipped_action_policy", async () => {
  const policy = await callTool("action_policy", {});
  const { panes, excluded } = actionPolicyPanes(policy.nquads);
  const advertised = new Set((await listTools()).map((t) => t.name));

  assert.ok(panes.length > 0, "the derived pane set must be non-empty");

  // BOTH directions: the pane set EQUALS the read half of the policy, and the read half
  // is exactly the advertised tools that are not the write set.
  const paneSet = new Set(panes);
  const excludedSet = new Set(excluded);
  assert.equal(paneSet.size + excludedSet.size, advertised.size, "panes ⊎ excluded = advertised");
  for (const name of paneSet) {
    assert.ok(advertised.has(name), `derived pane ${name} is not advertised by tools/list`);
    assert.ok(!excludedSet.has(name), `${name} is both a pane and excluded`);
  }
  for (const name of advertised) {
    assert.ok(paneSet.has(name) || excludedSet.has(name), `advertised tool ${name} is in neither half`);
  }

  // The excluded set EQUALS the six governed writes — checked by value, so a policy that
  // quietly re-typed a write as a read would fail here rather than grow a pane.
  assert.deepEqual(
    [...excludedSet].sort(),
    [
      "refute_conjecture",
      "revise_belief",
      "store_claim",
      "store_conjecture",
      "submit_candidate",
      "withdraw_candidate",
    ],
    "the excluded set must be exactly the 6-member write set",
  );
});

// ── 2 ───────────────────────────────────────────────────────────────────────

test("roundtrip_failure_is_recorded", async () => {
  // A fixture that fails on EXACTLY one target: `gmn1` is not a `convert` codec, so the
  // transcode hub refuses it while every RDF target succeeds.
  const data = "<http://example.org/s> <http://example.org/p> <http://example.org/o> .\n";
  const formats = ["turtle", "ntriples", "nquads", "trig", "jsonld", "gmn1"];
  const rows = [];
  for (const to of formats) {
    try {
      const out = await callTool("convert", { data, from: "ntriples", to });
      rows.push({ format: to, ok: true, bytes: out.bytes, loss: out.loss ?? [] });
    } catch (error) {
      rows.push({ format: to, ok: false, error: String(error.message ?? error) });
    }
  }
  const failed = rows.filter((r) => !r.ok);
  assert.equal(failed.length, 1, `exactly one target must fail: ${JSON.stringify(rows)}`);
  assert.equal(failed[0].format, "gmn1");
  // The failing row carries the full `{format, ok:false, error}` triple …
  assert.equal(failed[0].ok, false);
  assert.match(failed[0].error, /gmn1/, "the recorded error must name the codec that refused");
  // … and every OTHER target still produced a verdict — the differential did not abort.
  assert.equal(rows.filter((r) => r.ok).length, formats.length - 1);
  for (const row of rows.filter((r) => r.ok)) {
    assert.ok(row.bytes > 0, `${row.format} produced no output`);
    assert.ok(Array.isArray(row.loss), `${row.format} carries no loss ledger`);
  }
});

// ── 3 ───────────────────────────────────────────────────────────────────────

test("hard_error_on_missing_asset", async () => {
  // A FRESH transport module instance, so this test perturbs its own engine boot and not
  // the shared one every other test uses.
  const fresh = await import(`../../mcp-transport.mjs?missing-asset=${Date.now()}`);
  const missing = new URL("./gmeow.gts", new URL("../../", import.meta.url)).toString();
  fresh.configure({
    assetBase: new URL("../../", import.meta.url),
    fetchBytes: async (url) => {
      // One engine byte removed — modelled as the asset being unreadable, which is what a
      // truncated or absent file produces at the fetch boundary.
      throw new Error(`ENOENT: no such file or directory, open '${fileURLToPath(url)}'`);
    },
  });
  await assert.rejects(
    () => fresh.ensureMcp(),
    (error) => {
      // The rejection NAMES the asset path — a bare "boot failed" would leave the reader
      // with nothing to act on.
      assert.ok(
        error.message.includes(missing) || error.message.includes("gmeow.gts"),
        `the boot rejection must name the asset path, got: ${error.message}`,
      );
      return true;
    },
    "a missing engine asset must reject the boot, never degrade it",
  );

  // The shell renders that rejection into `#error-banner`. Its handler is asserted here
  // over the shipped shell source, so the wiring cannot be removed without this failing.
  const shell = await readFile(here("../index.html"), "utf8");
  assert.match(shell, /id="error-banner" role="alert"/, "the shell must carry a role=alert banner");
  assert.match(
    shell,
    /gmeow-console-error[\s\S]*errorBanner\.textContent/,
    "the shell must render the element's hard-error event into #error-banner",
  );
  assert.match(
    shell,
    /unhandledrejection[\s\S]*errorBanner\.textContent/,
    "a boot rejection that never reaches the element must still reach the banner",
  );
});

test("integrity_hard_fails_on_a_tampered_snapshot", async () => {
  // The engine boots over `assets/gmeow.gts` — the whole ontology it then answers from, and
  // by far the largest asset the client fetches. Verifying it by byte LENGTH alone accepts
  // any same-length substitution, which is the only substitution worth making; so the
  // transport recomputes its BLAKE3 against the emitted manifest. Both failure modes are
  // exercised here, each against a FRESH transport instance so neither perturbs the shared
  // one, and the boot must REFUSE in both — a snapshot that does not match its content
  // address is never "close enough to boot".
  const truthful = await readFile(SNAPSHOT);

  // (a) The bytes moved under a manifest that still describes the originals.
  const tampered = new Uint8Array(truthful);
  tampered[tampered.length - 1] ^= 0x01; // one bit, same length
  const a = await import(`../../mcp-transport.mjs?tamper-bytes=${Date.now()}`);
  a.configure({
    assetBase: new URL("../../", import.meta.url),
    fetchBytes: async (url) =>
      url.toString().endsWith("/bundle-manifest.json")
        ? encodeJson(SNAPSHOT_MANIFEST)
        : tampered,
  });
  await assert.rejects(
    () => a.ensureMcp(),
    (error) => {
      assert.match(
        error.message,
        /blake3:[0-9a-f]{64}/,
        `the refusal must name the digests it compared, got: ${error.message}`,
      );
      return true;
    },
    "a same-length bit flip in the snapshot must refuse the boot — byte length is not integrity",
  );

  // (b) The manifest entry is missing entirely. A manifest that does not describe an asset
  //     is not permission to load it unchecked.
  const b = await import(`../../mcp-transport.mjs?tamper-manifest=${Date.now()}`);
  b.configure({
    assetBase: new URL("../../", import.meta.url),
    fetchBytes: async (url) =>
      url.toString().endsWith("/bundle-manifest.json")
        ? encodeJson({ "assets/conjectures.ttl": { blake3: "blake3:00", bytes: 0 } })
        : new Uint8Array(truthful),
  });
  await assert.rejects(
    () => b.ensureMcp(),
    (error) => {
      assert.match(error.message, /assets\/gmeow\.gts/, error.message);
      return true;
    },
    "a missing manifest entry is a hard failure, not a bypass",
  );
});

// ── 4 ───────────────────────────────────────────────────────────────────────

test("session_annotations_are_quoted_triples", () => {
  const session = new ConsoleSession({ id: "t4", now: (i) => `2026-01-01T00:00:0${i}Z` });
  session.record({
    tool: "lookup_term",
    schema: "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/lookupTerm",
    args: { term: "gmeow:ToolCall" },
    result: { ok: true },
    // Every term DECLARES its kind. `{iri: …}` is an IRI; a bare string is a plain
    // literal. Nothing is inferred from the text of a value.
    derived: [
      {
        subject: { iri: "https://example.org/a" },
        predicate: { iri: "https://example.org/p" },
        object: { iri: "https://example.org/b" },
        antecedents: [{ iri: "https://blackcatinformatics.ca/gmeow/ToolCall" }],
      },
      {
        subject: { iri: "https://example.org/c" },
        predicate: { iri: "https://example.org/q" },
        object: "a literal answer",
        antecedents: [{ iri: "https://example.org/a" }, { iri: "https://example.org/b" }],
      },
    ],
  });
  const nquads = session.trajectoryNQuads();
  const quads = parseNQuads(nquads);

  const REIFIES = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";
  const DERIVED_BY = "https://blackcatinformatics.ca/gmeow/derivedBy";
  const FROM = "https://blackcatinformatics.ca/gmeow/wasDerivedFrom";

  const reifiers = quads.filter((q) => q.predicate === REIFIES);
  assert.equal(reifiers.length, 2, "every derived statement gets exactly one reifier");
  for (const quad of reifiers) {
    // The annotation is an RDF-1.2 TRIPLE TERM, not a blank-node reification.
    assert.equal(quad.object.kind, "triple", "the annotation must be a quoted triple");
    assert.equal(quad.object.value.length, 3);
    const subject = quad.subject.value;
    // … whose reifier names the call it came out of …
    const by = quads.filter((q) => q.subject.value === subject && q.predicate === DERIVED_BY);
    assert.equal(by.length, 1, `${subject} must name exactly one gmeow:derivedBy call`);
    // … and its ANTECEDENT SET, non-empty.
    const from = quads.filter((q) => q.subject.value === subject && q.predicate === FROM);
    assert.ok(from.length > 0, `${subject} must carry at least one antecedent`);
  }
  // Every derived RESULT triple in the trajectory is annotated: the asserted statement and
  // its reifier are both present, so no derived triple stands unattributed.
  const asserted = quads.filter((q) => q.subject.value.startsWith("https://example.org/"));
  assert.ok(asserted.length >= 2, "the derived statements are asserted as well as annotated");

  // A derived statement with NO antecedents is refused outright.
  assert.throws(
    () =>
      session.record({
        tool: "lookup_term",
        schema: "https://example.org/schema",
        args: {},
        derived: [
          {
            subject: { iri: "https://example.org/x" },
            predicate: { iri: "https://example.org/y" },
            object: "z",
            antecedents: [],
          },
        ],
      }) && session.trajectoryNQuads(),
    /names no antecedents/,
  );
});

test("session_records_the_shape_the_native_auditor_discovers", () => {
  const session = new ConsoleSession({ id: "t4b", now: (i) => `2026-01-01T00:00:0${i}Z` });
  for (const tool of ["lookup_term", "convert"]) {
    session.record({ tool, schema: `https://example.org/schema/${tool}`, args: {}, result: {} });
  }
  const quads = parseNQuads(session.trajectoryNQuads());
  const G = "https://blackcatinformatics.ca/gmeow/";
  const L = "https://blackcatinformatics.ca/logic/";
  const calls = quads
    .filter((q) => q.predicate.endsWith("#type") && q.object.value === `${G}ToolCall`)
    .map((q) => q.subject.value);
  assert.equal(calls.length, 2);
  for (const call of calls) {
    const of = (p) => quads.filter((q) => q.subject.value === call && q.predicate === p);
    // Exactly-one, on every join the auditor performs.
    assert.equal(of(`${L}instantiatesSchema`).length, 1, "bound: exactly one action schema");
    assert.equal(of(`${L}properPartOf`).length, 1, "exactly one trajectory anchor");
    assert.equal(of(`${G}atTime`).length, 1, "exactly one crisp timestamp");
    assert.equal(of(`${G}eventTemporalFrame`).length, 1, "exactly one temporal frame");
  }
  // ONE shared frame across the trajectory — the auditor hard-fails on a mixed one.
  const frames = new Set(
    quads.filter((q) => q.predicate === `${G}eventTemporalFrame`).map((q) => q.object.value),
  );
  assert.equal(frames.size, 1, `a trajectory must not mix temporal frames: ${[...frames]}`);
  // The anchor bears the start state the auditor requires.
  const anchor = quads.find((q) => q.predicate === `${L}properPartOf`).object.value;
  assert.ok(
    quads.some((q) => q.subject.value === anchor && q.predicate === `${L}transitionFromState`),
    "the trajectory anchor must bear logic:transitionFromState",
  );
});

test("session_permalink_round_trips_and_refuses_tampering", () => {
  const session = new ConsoleSession({ id: "t4c", now: () => "2026-01-01T00:00:00Z" });
  session.record({ tool: "lookup_term", schema: "https://example.org/s", args: { term: "gmeow:Cat" } });
  const fragment = session.permalink();
  const decoded = decodePermalink(fragment);
  assert.deepEqual(decoded.calls, [
    { tool: "lookup_term", schema: "https://example.org/s", args: { term: "gmeow:Cat" } },
  ]);
  const [address, payload] = fragment.split(/\.(.*)/s);
  assert.throws(() => decodePermalink(`${address}.${payload.slice(0, -2)}`), /content address/);
});

test("session_export_carries_the_store_segment_graph", () => {
  const session = new ConsoleSession({ id: "t4d", now: () => "2026-01-01T00:00:00Z" });
  session.record({ tool: "recall", schema: "https://example.org/s", args: {} });

  // ── case 1: the store holds state AND the engine serialized it → carried ───────
  const gts = exportSegment(session, {
    nquads: "<http://example.org/claim> <http://example.org/p> \"v\" .\n",
    heldBy: ["store_segment"],
    carriedBy: ["store_segment"],
  });
  assert.match(gts, /store-segment/, "the export names its session-store segment graph");
  const stored = parseNQuads(gts.split("\n").filter((l) => !l.startsWith("#")).join("\n")).filter(
    (q) => q.graph !== null,
  );
  assert.ok(stored.length > 0, "the store rides in a NAMED graph, not the default one");

  // ── case 2: the store holds state the engine will NOT serialize → refused ──────
  // This is the shipped defect: `""` is a string, so a type-only guard waved it through
  // and the export silently emitted an empty store graph. The refusal names the tool.
  for (const empty of ["", "   \n# only a comment\n"]) {
    assert.throws(
      () => exportSegment(session, { nquads: empty, heldBy: ["store_segment"], carriedBy: [] }),
      /store_segment reported stored state/,
      "state that cannot be carried must fail the export, and the failure must name its tool",
    );
  }
  assert.throws(
    () =>
      exportSegment(session, {
        nquads: "",
        heldBy: ["store_segment", "list_candidates"],
        carriedBy: [],
      }),
    /store_segment and list_candidates reported stored state/,
  );
  // ── case 2b: PARTIAL coverage is a refusal too, and names only the uncarried half.
  // A whole-reading "is the serialization non-empty?" test passes here — the claim
  // package serialized — while the candidate library is dropped on the floor. That is
  // half a snapshot claiming to be a whole one, so coverage is judged per HOLDER.
  assert.throws(
    () =>
      exportSegment(session, {
        nquads: "<http://example.org/claim> <http://example.org/p> \"v\" .\n",
        heldBy: ["store_segment", "list_candidates"],
        carriedBy: ["store_segment"],
      }),
    (error) => {
      assert.match(error.message, /^session export: list_candidates reported stored state/);
      assert.doesNotMatch(
        error.message,
        /store_segment reported/,
        "the holder that WAS carried must not be blamed",
      );
      return true;
    },
    "a holder whose state nothing carried must fail the export, alone and by name",
  );
  // A reading that was never taken is still refused: not knowing is not the same as empty.
  assert.throws(() => exportSegment(session, undefined), /store reading is required/);
  assert.throws(() => exportSegment(session, ""), /store reading is required/);
  assert.throws(
    () => exportSegment(session, { nquads: "", heldBy: [] }),
    /store reading is required/,
    "a reading with no coverage report is not a reading — it cannot say what it carried",
  );

  // ── case 3: the store holds NOTHING → exports, with no store graph at all ──────
  const bare = exportSegment(session, { nquads: "", heldBy: [], carriedBy: [] });
  assert.match(bare, /ToolCall/, "a no-store session must still export its trajectory");
  assert.doesNotMatch(
    bare,
    /store-segment/,
    "an untouched store must emit NO store graph — an empty one would imply state was captured",
  );
  assert.match(bare, /# store segment: none/, "the header must say the store carried nothing");
  const bareQuads = parseNQuads(bare.split("\n").filter((l) => !l.startsWith("#")).join("\n"));
  assert.ok(bareQuads.length > 0, "the trajectory is still present");
  assert.equal(
    bareQuads.filter((q) => q.graph !== null).length,
    0,
    "no named-graph quad may appear when the store held nothing",
  );
});

test("session_export_carries_a_stored_claim_and_the_result_parses", async () => {
  // The defect this pins: `storeReading` used to read `store_nquads ?? nquads` off a
  // `recall` result — fields NO engine tool returns — so `nquads` was ALWAYS `""`. A
  // session that recorded a `recall` against a store holding a claim therefore hit the
  // refusal on every export: the console could RECORD a session it could never EXPORT.
  //
  // The store BODY here is the engine's transport shape for one stored claim, written out
  // rather than read off a live browser store, because the shipped browser engine cannot
  // put a claim in the store this tool reads: `store_claim` lives in the reasoning image
  // and `recall` / `store_segment` in the core image, and the two images hold SEPARATE
  // in-process stores. That split is a real shipped defect, and it is exactly why the
  // engine linkage is asserted separately — `session_export_drives_the_real_worker_store_read`
  // below drives the REAL `store_segment` tool, and the segment's CONTENT is proven
  // against both storage backends (and re-seeded through purrdf's own `Memory::store()`)
  // by the Rust suite. What is under test HERE is the export composition: a session with a
  // store to carry must EXPORT, and what it emits must be RDF.
  const claim = "urn:gmeow:session:claim:0000";
  const storeBody =
    `<${claim}> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <${GMEOW_NS}ClaimToken> .\n` +
    `<${claim}> <http://www.w3.org/1999/02/22-rdf-syntax-ns#value> "the launch window closes on the 14th" .\n` +
    `<${claim}> <${GMEOW_NS}confidence> "0.8"^^<http://www.w3.org/2001/XMLSchema#decimal> .\n`;
  const reading = storeReading(
    { ok: true, claim_count: 1, tool_call_count: 0, nquads: storeBody },
    { ok: true, candidate_count: 0, candidates: [] },
  );
  assert.deepEqual(reading.heldBy, ["store_segment"], "the store reports it holds a claim");
  assert.deepEqual(reading.carriedBy, ["store_segment"], "…and the reading carries it");

  const session = new ConsoleSession({ id: "t4f", now: () => "2026-01-01T00:00:00Z" });
  session.record({
    tool: "recall",
    schema: "https://blackcatinformatics.ca/gmeow/examples/agentic/mcp-policy/recall",
    args: {},
    // The REAL engine answer for the recorded call, so the trajectory carries what the
    // shipped engine actually returned rather than a stand-in.
    result: await callTool("recall", { query: "launch window" }),
  });

  // The export SUCCEEDS — the whole point — and what it emitted is RDF, not a string that
  // resembles it: every non-comment line is parsed back.
  const gts = exportSegment(session, reading);
  const quads = parseNQuads(gts.split("\n").filter((l) => !l.startsWith("#")).join("\n"));
  assert.ok(quads.length > 0, "the exported segment must parse as N-Quads");

  const inStore = quads.filter((q) => q.graph !== null);
  assert.ok(inStore.length > 0, "the store rides in the named session-store graph");
  assert.ok(
    inStore.every((q) => q.graph.value === SESSION_STORE_GRAPH),
    "every store quad lands in SESSION_STORE_GRAPH and nowhere else",
  );
  const tokens = inStore
    .filter((q) => q.predicate.endsWith("#type") && q.object.value === `${GMEOW_NS}ClaimToken`)
    .map((q) => q.subject.value);
  assert.deepEqual(tokens, [claim], "the exported store carries the claim as a ClaimToken");
  const texts = inStore
    .filter((q) => q.subject.value === claim && q.predicate.endsWith("#value"))
    .map((q) => q.object.value);
  assert.deepEqual(
    texts,
    ["the launch window closes on the 14th"],
    "the claim text survives into the export",
  );
  // …and the trajectory is still in the DEFAULT graph, where the auditor reads it.
  assert.ok(
    quads.some((q) => q.graph === null && q.object.value === `${GMEOW_NS}ToolCall`),
    "the recorded trajectory stays in the default graph",
  );
});

test("session_export_drives_the_real_worker_store_read", async () => {
  // NON-VACUOUS by construction: the store string is not hand-supplied here, it is read
  // off the REAL results of the two shipped tools by the SAME function `OPS.export` uses,
  // and `store_segment` is the tool that actually serializes the store — `recall` answers
  // a query and cannot. The previous test could not have observed the defect this one
  // exists to pin, because it never touched the worker's store read at all.
  const engineStore = await callTool("store_segment", {});
  const candidates = await callTool("list_candidates", {});
  const store = storeReading(engineStore, candidates);

  const session = new ConsoleSession({ id: "t4e", now: () => "2026-01-01T00:00:00Z" });
  session.record({ tool: "recall", schema: "https://example.org/s", args: {} });

  // The holders are read off the tools' own answers, so they track the engine rather than
  // a list kept here. Whatever they say, the export's behaviour must follow from it.
  assert.ok(Array.isArray(store.heldBy), "the reading must report which tools hold state");
  assert.ok(Array.isArray(store.carriedBy), "the reading must report what it carried");
  assert.equal(typeof store.nquads, "string", "the reading must report a serialization");

  if (store.heldBy.length === 0) {
    // Nothing stored: the export must SUCCEED and carry no store graph.
    assert.equal(store.nquads, "", "an empty store cannot produce a serialization");
    const gts = exportSegment(session, store);
    assert.match(gts, /ToolCall/, "an untouched store must not block the trajectory export");
    assert.doesNotMatch(gts, /store-segment/, "no store graph may be emitted for an empty store");
    return;
  }

  const uncarried = store.heldBy.filter((tool) => !store.carriedBy.includes(tool));
  if (uncarried.length > 0) {
    // State exists and nothing carried it. The export MUST refuse, naming the tools, so
    // the console can never emit a snapshot that quietly dropped them.
    assert.throws(
      () => exportSegment(session, store),
      new RegExp(`${uncarried[0]}.*reported stored state`),
      "stored state that cannot be carried must FAIL the export and name its tool",
    );
    return;
  }

  // State exists and the engine serialized it: the export must land it in the named graph.
  const gts = exportSegment(session, store);
  const stored = parseNQuads(gts.split("\n").filter((l) => !l.startsWith("#")).join("\n")).filter(
    (q) => q.graph !== null,
  );
  assert.ok(stored.length > 0, "a non-empty store must export a non-empty store segment graph");
});

// ── 5 ───────────────────────────────────────────────────────────────────────

test("conjecture_selector_matches_the_shipped_library", async () => {
  const ttl = await readFile(here("../../../../../slices/grounding/logic/examples/conjectures.ttl"), "utf8");
  // Transcoded by the ENGINE's own convert tool — the same path the browser controller
  // takes, so the selector is derived from what the engine read.
  const converted = await callTool("convert", { data: ttl, from: "turtle", to: "nquads" });
  const library = conjectureLibrary(converted.output);

  // The selector is `library.map(entry => option(entry.id, entry.label))` in
  // `assets/docs-controller.mjs`; asserting the derivation is total is asserting the two
  // sets are equal, because the option list is the image of the entry list.
  const shipped = [...ttl.matchAll(/^(\S+)\s*\n\s+a logic:Conjecture\b/gm)].map((m) =>
    m[1].replace(/^ex:/, ""),
  );
  assert.ok(shipped.length > 0, "the shipped corpus must declare conjectures");
  assert.deepEqual(
    library.map((e) => e.id).sort(),
    shipped.slice().sort(),
    "the selector entries must equal the library entries, in both directions",
  );
  // Every entry carries the facets the panel renders — a null-everywhere entry would
  // "match" a set comparison while rendering nothing.
  for (const entry of library) {
    assert.ok(entry.label.length > 0, `${entry.id} has no label`);
    assert.ok(entry.formulaKey !== null, `${entry.id} records no formula content key`);
    assert.ok(entry.standpoint !== null, `${entry.id} names no standpoint`);
    assert.ok(entry.lifecycle !== null, `${entry.id} records no Belnap lifecycle`);
  }
  // The refutations carry their contradiction witnesses.
  const refuted = library.filter((e) => e.lifecycle.endsWith("ConjectureRefutedInStandpoint"));
  assert.ok(refuted.length > 0, "the corpus exercises the refutation branch");
  for (const entry of refuted) {
    assert.ok(entry.witness !== null, `refuted ${entry.id} names no contradiction witness`);
    assert.ok(entry.witnessPremises.length > 0, `witness of ${entry.id} carries no premises`);
  }
});

// ── 6 ───────────────────────────────────────────────────────────────────────

test("console_js_exports_are_a_subset_of_mcp_wasm", async () => {
  // The wasm-bindgen glue of each segment declares that segment's export surface.
  const exportsOf = async (glue) => {
    const text = await readFile(here(glue), "utf8");
    return new Set([...text.matchAll(/^export function (\w+)/gm)].map((m) => m[1]));
  };
  const core = await exportsOf("../../mcp-core/pkg/gmeow_mcp_core_wasm.js");
  const reasoning = await exportsOf("../../mcp/pkg/gmeow_mcp_wasm.js");
  const segmentExports = new Set([...core, ...reasoning]);
  assert.ok(segmentExports.size > 0, "the segment glue must declare exports");

  // What the console's JS actually CALLS of the wasm surface: the shim re-exports the
  // engine functions by name, so the names the transport imports from `mcp-core/index.mjs`
  // (and dynamically from `mcp/index.mjs`) are exactly the wasm entry points reached.
  const shimNames = async (shim) => {
    const text = await readFile(here(shim), "utf8");
    const imported = [...text.matchAll(/^import\s+\w*,?\s*\{([\s\S]*?)\}\s+from\s+"\.\/pkg\//gm)];
    const names = new Set();
    for (const block of imported) {
      for (const entry of block[1].split(",")) {
        const name = entry.trim().split(/\s+as\s+/)[0].trim();
        if (name.length > 0) names.add(name);
      }
    }
    return names;
  };
  const called = new Set([
    ...(await shimNames("../../mcp-core/index.mjs")),
    ...(await shimNames("../../mcp/index.mjs")),
  ]);
  assert.ok(called.size > 0, "the console reaches the wasm surface through the shims");

  // The SUBSET claim, proved by export-set comparison.
  const missing = [...called].filter((name) => !segmentExports.has(name));
  assert.deepEqual(
    missing,
    [],
    `the console calls wasm exports the segments do not declare: ${missing.join(", ")}`,
  );

  // And nothing in the console's own JS reaches around the shims into a `pkg/` module.
  for (const file of ["../element.mjs", "../engine.worker.mjs", "../session.mjs", "../../mcp-transport.mjs", "../../docs-controller.mjs"]) {
    const text = await readFile(here(file), "utf8");
    assert.ok(
      !/from\s+"[^"]*mcp(-core)?\/pkg\//.test(text),
      `${file} imports a wasm pkg module directly instead of going through the shim`,
    );
  }
});

// ── 7 ───────────────────────────────────────────────────────────────────────

test("gallery_vignettes_exercise_quoted_triples_and_mint_no_vocabulary", () => {
  assert.ok(VIGNETTES.length > 0);
  for (const v of VIGNETTES) {
    const payload = JSON.stringify(v.args);
    assert.match(payload, /<<\(|rdf:reifies|reifies/, `vignette ${v.id} exercises no quoted triple`);
    // Every invented individual is under example.org — never minted into gmeow:/logic:.
    const minted = [...payload.matchAll(/https:\\\/\\\/blackcatinformatics\.ca\\\/(gmeow|logic)\\\/([A-Za-z]+)/g)];
    assert.ok(minted.length >= 0);
    assert.ok(
      !/@prefix ex:\s+<https:\/\/blackcatinformatics/.test(payload),
      `vignette ${v.id} points its example prefix at a shipped namespace`,
    );
  }
});

test("worked_vignettes_execute_against_the_shipped_engine", async () => {
  // The gallery is not a screenshot: every vignette whose pane is a core-segment tool is
  // dispatched here for real, and must answer.
  const core = new Set(["validate_local", "convert", "query_local", "encode_gmn1"]);
  for (const v of VIGNETTES.filter((x) => core.has(x.pane))) {
    const payload = await callTool(v.pane, v.args);
    assert.ok(payload !== null && typeof payload === "object", `vignette ${v.id} returned nothing`);
  }
});

// ── The shipped-runtime totality assertions ─────────────────────────────────
//
// Each test below pins one defect that the surface's OWN prose already claimed was
// impossible — a reader documented as total that read a half-term as a whole one, an
// emitter that guessed term kinds, a re-grapher that produced five-term lines, a worker
// dispatch that ran inherited methods, an element that hung for ever on a failed worker,
// and a manifest that promised a PWA install with no icon to install.

/**
 * The shipped `engine.worker.mjs`, importable, at a URL its own specifiers resolve from.
 *
 * The worker's imports are written for the layout the RENDERER emits — `console/…` beside
 * `assets/…` — which is not the layout of this build-input tree, where `console/` sits
 * INSIDE `assets/`. So the site layout is staged in a temp directory: the worker's own
 * bytes are copied in, and its siblings are symlinked. Node resolves a symlink to its real
 * path before keying the module cache, so `mcp-transport.mjs` and `session.mjs` load as the
 * SAME module instances this file already imported and configured — the worker under test
 * drives the engine this lane booted, not a second unconfigured copy of it.
 *
 * Both candidate sibling spellings are staged, so this harness follows the shipped file
 * rather than pinning today's specifier text.
 */
async function shippedWorkerUrl() {
  const { mkdtemp, mkdir, copyFile, symlink } = await import("node:fs/promises");
  const { tmpdir } = await import("node:os");
  const { join } = await import("node:path");
  const root = await mkdtemp(join(tmpdir(), "gmeow-console-worker-"));
  const assets = here("../../");
  await mkdir(join(root, "console", "pkg"), { recursive: true });
  await copyFile(here("../engine.worker.mjs"), join(root, "console", "engine.worker.mjs"));
  await symlink(here("../session.mjs"), join(root, "console", "session.mjs"));
  // The worker imports its transport as `./pkg/mcp-transport.mjs` — the one specifier that
  // resolves both in the assembled site tree (where the producer emits a forwarder there)
  // and in the published npm package (where the whole payload is staged there). Node
  // resolves a symlinked module's own imports from its REAL path, so the transport still
  // finds its siblings in `assets/`.
  await symlink(join(assets, "mcp-transport.mjs"), join(root, "console", "pkg", "mcp-transport.mjs"));
  await symlink(here("../examples"), join(root, "console", "examples"));
  await symlink(assets, join(root, "assets"));
  for (const sibling of ["mcp-transport.mjs", "blake3.mjs", "mcp-core", "mcp"]) {
    await symlink(join(assets, sibling), join(root, sibling));
  }
  return new URL(`file://${join(root, "console", "engine.worker.mjs")}`);
}

/** A backslash. An N-Quads escape is DATA in these tests, so it is built by code point. */
const BS = String.fromCharCode(0x5c);
const uchar = (hex) => `${BS}u${hex}`;
const bigUchar = (hex) => `${BS}U${hex}`;
const echar = (c) => `${BS}${c}`;

test("nquads_reader_decodes_the_full_escape_set_and_refuses_a_half_read_term", () => {
  const S = "<http://example.org/s>";
  const P = "<http://example.org/p>";

  // (a) The CAPABILITY first: every `ECHAR` and both `UCHAR` widths decode to the code
  //     points they name. `A` is the letter A — it is not the five characters
  //     `u0041`, which is what a reader that only special-cased `n`/`t`/`r` produced for
  //     every escape it did not know, silently, in the middle of a literal.
  const encoded = [
    uchar("0041"),
    echar("t"),
    echar("b"),
    echar("f"),
    echar("r"),
    echar("n"),
    echar('"'),
    echar("'"),
    echar(BS),
    bigUchar("0001F63A"),
  ].join("");
  const expected =
    "A" +
    String.fromCharCode(0x09, 0x08, 0x0c, 0x0d, 0x0a) +
    '"' +
    "'" +
    BS +
    String.fromCodePoint(0x1f63a);
  const [decoded] = parseNQuads(`${S} ${P} "${encoded}" .\n`);
  assert.equal(decoded.object.kind, "literal");
  assert.equal(decoded.object.value, expected, "every escape must decode to its code point");

  // A `UCHAR` inside an IRI decodes too, including inside an RDF-1.2 triple term.
  const [starred] = parseNQuads(`<<( <http://example.org/${uchar("0061")}> ${P} ${S} )>> ${P} "x" .\n`);
  assert.equal(starred.subject.kind, "triple");
  assert.equal(starred.subject.value[0].value, "http://example.org/a");

  // (b) …and every half-read term is REPORTED. Each of these was previously accepted:
  //     the literal loop ran off the end of the line and returned the remainder as a
  //     value, the `)>>` closer was "stripped" by a regex that matched nothing, and a
  //     fifth term was read and discarded.
  //
  //     Each refusal names the term that could not be read. That is part of the fix, not
  //     decoration: the old reader swallowed the rest of the line into the unterminated
  //     literal and then blamed the GRAPH term, pointing the reader at the wrong end of
  //     the line.
  const refusals = [
    ["an unterminated literal", `${S} ${P} "no closing quote .`, /cannot read an object term/],
    ["an unknown escape", `${S} ${P} "bad ${echar("q")} escape" .`, /cannot read an object term/],
    ["a truncated UCHAR", `${S} ${P} "${BS}u00" .`, /cannot read an object term/],
    ["a triple term with no closer", `<<( ${S} ${P} ${S} ${S} ${P} "x" .`, /cannot read a subject term/],
    [
      "a fifth term",
      `${S} ${P} ${S} <http://example.org/g> <http://example.org/h> .`,
      /does not end after its terms/,
    ],
    ["a missing terminator", `${S} ${P} ${S}`, /does not end after its terms/],
    ["a literal graph term", `${S} ${P} ${S} "not a graph" .`, /cannot name a graph/],
    ["an IRI carrying a raw space", `<http://example.org/a b> ${P} ${S} .`, /cannot read a subject term/],
  ];
  for (const [why, line, named] of refusals) {
    assert.throws(
      () => parseNQuads(`${line}\n`),
      named,
      `${why} must be reported, by name, never read as if it were a whole term`,
    );
  }
});

test("session_literals_carry_the_engine_s_own_escaping_of_every_control_character", async () => {
  // The console's own `callIri` joins with U+001F, and `toolArguments`/`toolResult` carry
  // JSON built from arbitrary engine payloads, so a control character in a recorded
  // literal is a live case. Escaping only LF/CR/TAB let every other one through raw.
  const text = `unit${String.fromCharCode(0x1f)}sep${String.fromCharCode(0x07)}bell`;
  const session = new ConsoleSession({ id: "t8a", now: () => "2026-01-01T00:00:00Z" });
  session.record({
    tool: "recall",
    schema: "https://example.org/schema/recall",
    args: { needle: text },
    result: { ok: true },
    derived: [
      {
        subject: { iri: "https://example.org/answer" },
        predicate: { iri: "https://example.org/says" },
        object: text,
        antecedents: [{ iri: "https://example.org/claim" }],
      },
    ],
  });
  const nquads = session.trajectoryNQuads();

  // (a) It parses, and the literal decodes back to exactly the characters recorded.
  const said = parseNQuads(nquads).filter((q) => q.predicate === "https://example.org/says");
  assert.equal(said.length, 1);
  assert.equal(said[0].object.value, text, "the recorded characters must survive the round trip");

  // (b) The SERIALIZATION is the engine's own. Feeding the recorded trajectory back
  //     through the shipped `convert` re-emits canonical N-Quads; the console's line and
  //     the engine's line must be the same bytes, which is what "the console writes RDF"
  //     means. A raw control character survives the parse and comes back ESCAPED, so this
  //     comparison — not a parse — is what catches the under-escaping.
  const canonical = await callTool("convert", { data: nquads, from: "nquads", to: "nquads" });
  const saysLine = (body) => body.split("\n").find((line) => line.includes("/says"));
  assert.equal(
    saysLine(nquads),
    saysLine(canonical.output),
    "the console must emit the same escaping the engine's own serializer does",
  );

  // (c) …so no raw control character reaches the wire at all.
  const raw = [...nquads].filter((c) => c !== "\n" && c.codePointAt(0) < 0x20);
  assert.deepEqual(raw, [], "no raw control character may appear in the emitted N-Quads");
});

test("session_emits_the_term_kind_the_caller_declared_and_never_guesses_one", () => {
  const session = new ConsoleSession({ id: "t8b", now: () => "2026-01-01T00:00:00Z" });
  // Prose that happens to begin with `https://` AND carries a space: the old
  // `startsWith("http")` heuristic emitted it as `<…>`, which is not an IRI at all.
  const prose = "https://example.org/a b — the answer, as the tool phrased it";
  session.record({
    tool: "lookup_term",
    schema: "https://example.org/schema/lookupTerm",
    args: {},
    derived: [
      {
        // A `urn:`/`did:` pair — IRIs the heuristic emitted as literals.
        subject: { iri: "urn:gmeow:session:claim:0000" },
        predicate: { iri: "https://example.org/p" },
        object: { iri: "did:example:123" },
        antecedents: [{ iri: "urn:gmeow:session:claim:0001" }],
      },
      {
        subject: { iri: "https://example.org/c" },
        predicate: { iri: "https://example.org/q" },
        object: prose,
        antecedents: ["a plain-literal antecedent"],
      },
      {
        subject: { iri: "https://example.org/c" },
        predicate: { iri: "https://example.org/n" },
        object: { literal: "42", datatype: "http://www.w3.org/2001/XMLSchema#integer" },
        antecedents: [{ literal: "chat", language: "x-gmeow-english" }],
      },
    ],
  });
  const quads = parseNQuads(session.trajectoryNQuads());
  const objectOf = (predicate) => {
    const matched = quads.filter((q) => q.predicate === predicate && q.graph === null);
    assert.equal(matched.length, 1, `${predicate} must be asserted exactly once`);
    return matched[0].object;
  };

  const iriObject = objectOf("https://example.org/p");
  assert.equal(iriObject.kind, "iri", "a declared `did:` IRI rides as an IRI");
  assert.equal(iriObject.value, "did:example:123");

  const literalObject = objectOf("https://example.org/q");
  assert.equal(literalObject.kind, "literal", "declared prose rides as a literal, URL or not");
  assert.equal(literalObject.value, prose, "…verbatim, with its space and its em dash");

  const typed = objectOf("https://example.org/n");
  assert.equal(typed.kind, "literal");
  assert.equal(typed.datatype, "http://www.w3.org/2001/XMLSchema#integer");
  const tagged = quads.filter((q) => q.predicate === `${GMEOW_NS}wasDerivedFrom`).map((q) => q.object);
  assert.ok(
    tagged.some((t) => t.kind === "literal" && t.language === "x-gmeow-english"),
    "a declared language-tagged antecedent keeps its tag",
  );
  assert.ok(
    tagged.some((t) => t.kind === "iri" && t.value === "urn:gmeow:session:claim:0001"),
    "a declared `urn:` antecedent rides as an IRI, not as a literal",
  );

  // The quoted-triple annotation carries the SAME kinds as the asserted statement — the
  // reifier and the statement it reifies cannot disagree about what the object was.
  const REIFIES = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";
  const reified = quads.filter((q) => q.predicate === REIFIES).map((q) => q.object.value[2].kind);
  assert.deepEqual(reified.slice().sort(), ["iri", "literal", "literal"]);

  // An UNDECLARED term is refused rather than guessed at, in every position.
  const undeclared = (statement) =>
    assert.throws(() => {
      const s = new ConsoleSession({ id: "t8c", now: () => "2026-01-01T00:00:00Z" });
      s.record({ tool: "lookup_term", schema: "https://example.org/s", derived: [statement] });
      return s.trajectoryNQuads();
    }, /declared/);
  undeclared({
    subject: "https://example.org/x",
    predicate: { iri: "https://example.org/y" },
    object: "z",
    antecedents: ["a"],
  });
  undeclared({
    subject: { iri: "https://example.org/x" },
    predicate: { iri: "https://example.org/y" },
    object: { url: "https://example.org/z" },
    antecedents: ["a"],
  });
});

test("session_export_regraphs_a_store_quad_instead_of_emitting_a_five_term_line", () => {
  const session = new ConsoleSession({ id: "t8d", now: () => "2026-01-01T00:00:00Z" });
  session.record({ tool: "recall", schema: "https://example.org/s", args: {} });

  // A store serialization that exercises everything a naive `\s*\.\s*$` strip gets wrong:
  // a QUAD that already names a graph, a literal containing both a space and a `.`, and an
  // RDF-1.2 triple term whose own components must not be mistaken for statement terms.
  const RDF_VALUE = "http://www.w3.org/1999/02/22-rdf-syntax-ns#value";
  const storeBody =
    `<urn:gmeow:claim:0> <${RDF_VALUE}> "v. 2 of the plan" <urn:gmeow:store:own-graph> .\n` +
    `<urn:gmeow:claim:0> <${GMEOW_NS}confidence> "0.8"^^<http://www.w3.org/2001/XMLSchema#decimal> .\n` +
    `<<( <urn:gmeow:claim:0> <${RDF_VALUE}> "v. 2 of the plan" )>> <${GMEOW_NS}derivedBy> ` +
    `<urn:gmeow:call:0> <urn:gmeow:store:own-graph> .\n`;
  const gts = exportSegment(session, {
    nquads: storeBody,
    heldBy: ["store_segment"],
    carriedBy: ["store_segment"],
  });

  // The whole export parses — before the re-graph was term-aware, the two four-term lines
  // came back out as five-term lines and this parse failed for the entire file.
  const quads = parseNQuads(gts.split("\n").filter((l) => !l.startsWith("#")).join("\n"));
  const stored = quads.filter((q) => q.graph !== null);
  assert.equal(stored.length, 3, "every store line rides into the export");
  assert.ok(
    stored.every((q) => q.graph.value === SESSION_STORE_GRAPH),
    "the store's own graph term is REPLACED by the segment graph, never appended to it",
  );
  const value = stored.find((q) => q.predicate === RDF_VALUE);
  assert.equal(value.object.value, "v. 2 of the plan", "a literal with a `.` survives intact");
  const annotation = stored.find((q) => q.subject.kind === "triple");
  assert.equal(annotation.subject.value.length, 3, "a quoted triple survives as one term");
  assert.equal(annotation.subject.value[2].value, "v. 2 of the plan");

  // A store line that is not an N-Quads statement cannot be re-graphed into one, and
  // saying so is the point: emitting an invalid quad is the failure being removed.
  for (const malformed of ["<urn:a> <urn:b> .", "<urn:a> <urn:b> <urn:c>"]) {
    assert.throws(
      () =>
        exportSegment(session, {
          nquads: `${malformed}\n`,
          heldBy: ["store_segment"],
          carriedBy: ["store_segment"],
        }),
      /not a terminated N-Quads statement/,
    );
  }
});

test("worker_dispatch_is_total_over_own_operations_only", async () => {
  // The worker's own commitment is that an unregistered tool name is a NAMED hard error.
  // `OPS[op]` is a prototype-chain walk, so `constructor`, `toString` and `valueOf` all
  // resolved to inherited functions and were INVOKED — `{op: "constructor"}` answered
  // `{ok: true, value: {}}`, which is neither a registered operation nor an error.
  const posted = [];
  const listeners = new Map();
  globalThis.self = {
    postMessage: (message) => posted.push(message),
    addEventListener: (type, handler) => listeners.set(type, handler),
  };
  await import(await shippedWorkerUrl());
  const onMessage = listeners.get("message");
  assert.equal(typeof onMessage, "function", "the worker must register a message listener");

  const dispatch = async (op, args) => {
    const before = posted.length;
    onMessage({ data: { id: posted.length, op, args } });
    // The handler is async and posts on settle; poll the queue rather than guess a delay.
    for (let tick = 0; tick < 2000 && posted.length === before; tick += 1) {
      await new Promise((resolve) => setTimeout(resolve, 5));
    }
    return posted.at(-1);
  };

  // The CAPABILITY: a registered operation still dispatches and answers.
  const booted = await dispatch("boot", { sessionId: "t8e" });
  assert.equal(booted.ok, true, `boot must answer: ${JSON.stringify(booted)}`);
  assert.ok(booted.value.panes.length > 0, "boot returns the derived pane set");
  const trajectory = await dispatch("trajectory", {});
  assert.equal(trajectory.ok, true);

  // …and every INHERITED member is a named hard error, not an invocation.
  for (const op of ["constructor", "toString", "valueOf", "hasOwnProperty", "__proto__", "nope"]) {
    const answer = await dispatch(op, {});
    assert.equal(answer.ok, false, `\`${op}\` must not dispatch: ${JSON.stringify(answer)}`);
    assert.match(answer.error, new RegExp(`unknown console operation`));
    assert.ok(answer.error.includes(op), `the refusal must name the operation, got ${answer.error}`);
  }
  delete globalThis.self;
});

test("element_rejects_every_request_in_flight_when_the_worker_fails", async () => {
  // The shipped README promises that an unavailable engine is a VISIBLE hard error and
  // that nothing degrades quietly. The `error` listener rendered the banner but never
  // settled `pending`, so `boot()`'s `await this.ask("boot", …)` hung for ever and the
  // pane sat on "Starting the engine…" behind a banner saying the engine had failed.
  //
  // The element's HOST is substituted here (a DOM and a Worker constructor); the element
  // itself — `boot`, `ask`, the listener wiring and the pending bookkeeping — is the
  // shipped code, run unmodified.
  const workers = [];
  class ShimWorker extends EventTarget {
    constructor(url, options) {
      super();
      this.url = url;
      this.options = options;
      this.terminated = false;
      workers.push(this);
    }
    postMessage() {} // A worker that never loaded never answers. That is the whole case.
    terminate() {
      this.terminated = true;
    }
    failToLoad(message) {
      const event = new Event("error");
      event.message = message;
      this.dispatchEvent(event);
    }
  }
  class ShimNode {
    constructor(tag) {
      this.tagName = tag;
      this.children = [];
      this.dataset = {};
      this.textContent = "";
    }
    append(...kids) {
      this.children.push(...kids);
    }
    replaceChildren(...kids) {
      this.children = kids.filter((kid) => kid !== null && kid !== undefined);
    }
    querySelectorAll() {
      return [];
    }
    setAttribute(name, value) {
      this[name] = value;
    }
  }
  globalThis.document = { createElement: (tag) => new ShimNode(tag) };
  globalThis.HTMLElement = class extends EventTarget {
    attachShadow() {
      this.shadowRoot = new ShimNode("#shadow-root");
      return this.shadowRoot;
    }
  };
  globalThis.Worker = ShimWorker;
  const { GmeowConsole } = await import("../element.mjs");

  /** Settle `promise`, or fail LOUDLY rather than let `node --test` time the suite out. */
  const deadline = (promise, what) =>
    Promise.race([
      promise,
      new Promise((_resolve, reject) => {
        const timer = setTimeout(
          () => reject(new Error(`HUNG: ${what} never settled — the console would wait for ever`)),
          2000,
        );
        timer.unref();
      }),
    ]);

  // ── a worker that fails to load ────────────────────────────────────────────
  const node = new GmeowConsole();
  const reported = [];
  node.addEventListener("gmeow-console-error", (event) => reported.push(event.detail));
  node.connectedCallback();
  const inFlight = node.ask("trajectory", {});
  assert.equal(node.pending.size, 2, "the boot and the second request are both in flight");
  workers.at(-1).failToLoad("engine.worker.mjs could not be loaded");

  await assert.rejects(
    () => deadline(inFlight, "a request in flight when the worker failed"),
    /engine worker failed/,
    "a worker failure must REJECT every request in flight, not just paint a banner",
  );
  assert.equal(node.pending.size, 0, "nothing may be left waiting on a worker that failed");
  assert.ok(
    reported.some((detail) => detail.where === "worker"),
    "the failure is still reported for the shell's #error-banner",
  );
  // A request made AFTER the failure is refused immediately, for the same reason.
  await assert.rejects(() => deadline(node.ask("trajectory", {}), "a post-failure request"), /worker/);

  // ── a worker that is terminated with work outstanding ──────────────────────
  const detached = new GmeowConsole();
  detached.addEventListener("gmeow-console-error", () => {});
  detached.connectedCallback();
  const orphaned = detached.ask("export", {});
  detached.disconnectedCallback();
  assert.ok(workers.at(-1).terminated, "disconnecting terminates the worker");
  await assert.rejects(
    () => deadline(orphaned, "a request outstanding when the worker was terminated"),
    /terminated/,
    "a terminated worker must settle its outstanding requests too",
  );
});

test("the_console_manifest_ships_installable_icons", async () => {
  // The README claims the console installs as a PWA. Chrome's installability criteria
  // require at least one square icon of 192px or more; the manifest carried NO `icons`
  // member at all, so the claim was false on the shipped surface.
  const manifest = JSON.parse(await readFile(here("../manifest.webmanifest"), "utf8"));
  assert.ok(Array.isArray(manifest.icons) && manifest.icons.length > 0, "the manifest declares icons");

  /** `[width, height]` read out of a PNG's IHDR — the bytes, not the file name. */
  const pngSize = (bytes) => {
    const header = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
    assert.ok(bytes.subarray(0, 8).equals(header), "a declared PNG must actually be a PNG");
    assert.equal(bytes.subarray(12, 16).toString("ascii"), "IHDR");
    return [bytes.readUInt32BE(16), bytes.readUInt32BE(20)];
  };

  let installable = 0;
  const purposes = new Set();
  for (const icon of manifest.icons) {
    for (const purpose of String(icon.purpose ?? "any").split(/\s+/)) purposes.add(purpose);
    const bytes = await readFile(here(`../${icon.src.replace(/^\.\//, "")}`));
    if (icon.type === "image/svg+xml") {
      const text = bytes.toString("utf8");
      assert.match(text, /<svg[\s\S]*<\/svg>/, `${icon.src} is not an SVG document`);
      assert.match(text, /SPDX-License-Identifier/, `${icon.src} carries no SPDX header`);
      continue;
    }
    const [width, height] = pngSize(bytes);
    assert.equal(
      `${width}x${height}`,
      icon.sizes,
      `${icon.src} is ${width}x${height}, but the manifest declares ${icon.sizes}`,
    );
    assert.equal(width, height, `${icon.src} must be square`);
    if (width >= 192) installable += 1;
    // A binary asset carries its REUSE sidecar, like every other one in this tree.
    await readFile(here(`../${icon.src.replace(/^\.\//, "")}.license`), "utf8");
  }
  assert.ok(installable > 0, "at least one raster icon must be 192px or larger to be installable");
  assert.ok(purposes.has("maskable"), "a maskable icon is required for a non-letterboxed install");
  assert.ok(purposes.has("any"), "an `any`-purpose icon is required for the general case");

  // An icon the site never emits is a 404, and a manifest pointing at a 404 is not
  // installable either — so every declared icon must be in the console's shipped file set.
  const shipped = await readFile(here("../../../src/console.rs"), "utf8");
  const unshipped = manifest.icons
    .map((icon) => icon.src.replace(/^\.\//, ""))
    .filter((name) => !shipped.includes(`console/${name}`));
  assert.deepEqual(unshipped, [], `SHELL_FILES does not emit: ${unshipped.join(", ")}`);
});
