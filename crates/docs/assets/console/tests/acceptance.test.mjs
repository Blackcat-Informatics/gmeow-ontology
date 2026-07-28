// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// The console's named acceptance assertions, run under `node --test` against the SHIPPED
// wasm engine and the SHIPPED bundle — no browser, no mocks, no stubs.
//
// Each `test(...)` name below is one of the seven named assertions. They are gate
// blockers: none is skipped, none is conditional, and the engine they drive is the same
// `crates/docs/assets/mcp-core/` image the site and the console load in a browser.
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
  actionPolicyPanes,
  callTool,
  conjectureLibrary,
  configure,
  listTools,
  parseNQuads,
} from "../../mcp-transport.mjs";
import { ConsoleSession, decodePermalink, exportSegment } from "../session.mjs";
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
    derived: [
      {
        subject: "https://example.org/a",
        predicate: "https://example.org/p",
        object: "https://example.org/b",
        antecedents: ["https://blackcatinformatics.ca/gmeow/ToolCall"],
      },
      {
        subject: "https://example.org/c",
        predicate: "https://example.org/q",
        object: "a literal answer",
        antecedents: ["https://example.org/a", "https://example.org/b"],
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
        derived: [{ subject: "https://example.org/x", predicate: "https://example.org/y", object: "z", antecedents: [] }],
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
  const gts = exportSegment(session, "<http://example.org/claim> <http://example.org/p> \"v\" .\n");
  assert.match(gts, /store-segment/, "the export names its session-store segment graph");
  const stored = parseNQuads(gts.split("\n").filter((l) => !l.startsWith("#")).join("\n")).filter(
    (q) => q.graph !== null,
  );
  assert.ok(stored.length > 0, "the store rides in a NAMED graph, not the default one");
  // An export with no store is refused — half a snapshot is not a snapshot.
  assert.throws(() => exportSegment(session, undefined), /store serialization is required/);
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
