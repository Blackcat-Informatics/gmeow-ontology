// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// The consumability lane: prove that what `npm pack` produces is what a consumer can
// actually USE.
//
// For every package this repository publishes, this driver
//
//   1. runs `npm pack` in the package directory — the real tarball, from the real `files`
//      set, not a copy of the working tree;
//   2. `npm install`s that tarball into a throwaway project, so the package is resolved
//      BY NAME through `node_modules` exactly as a consumer resolves it;
//   3. runs a witness against the INSTALLED module: it imports the scoped package name,
//      instantiates the real wasm out of the installed bytes, and performs a real
//      operation — several of them against the SAME committed attestations the in-tree
//      parity lanes assert byte-identity to.
//
// A package with no witness is a HARD FAILURE, not a skip: a new package that quietly
// went unproven would defeat the whole lane.
//
// Usage: `node scripts/npm-consumability.mjs`

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import { exportedValueNames, publishedPackages, repoRoot } from "./npm-packaging.mjs";

const root = fileURLToPath(repoRoot());
const SNAPSHOT_PATH = join(root, "generated/dist/gmeow.gts");

function run(command, args, cwd) {
  return execFileSync(command, args, { cwd, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
}

/**
 * The `gmeow.gts` snapshot the engine witnesses drive. A missing bundle is a HARD,
 * NAMED failure — never a skipped witness: an engine lane that quietly did not run is
 * indistinguishable from one that passed, which is exactly the silent degradation the
 * no-optionality rule forbids. Mirrors the message the native witness halves use.
 */
async function snapshot() {
  try {
    return new Uint8Array(await readFile(SNAPSHOT_PATH));
  } catch (error) {
    throw new Error(
      `the consumability witness needs the generated bundle ${SNAPSHOT_PATH} ` +
        `(run \`make regen\`): ${error.message}`,
    );
  }
}

// ── the per-package witnesses ───────────────────────────────────────────────────────
//
// Keyed by the package's OWN declared name, read from the manifest — never a name this
// file invents. Each receives the module imported from `node_modules` and the installed
// package directory.

const WITNESSES = {
  "@blackcatinformatics/gmeow-validate-wasm": async (mod) => {
    await mod.ready();
    assert.match(
      mod.gmn_codebook_digest(),
      /^[0-9a-f]{64}$/,
      "the embedded GMN codebook digest must be a blake3 hex digest",
    );

    // Tier-1 conformance against the REAL shipped bundle, through the installed wasm.
    const report = JSON.parse(
      mod.validate(
        '@prefix ex: <https://example.org/> .\nex:s ex:p "o" .\n',
        "turtle",
        await snapshot(),
        "https://blackcatinformatics.ca/gmeow/",
        "consumability.ttl",
      ),
    );
    assert.equal(report.tool, "validate", "the installed validator returns a canonical Report");

    // The GMN-1 codec validator, over the SAME committed vectors the native host tests
    // (`gmn_tests` in crates/validate-wasm/src/lib.rs) pin — read from the slice, never
    // restated here. The negative leg is the load-bearing one: the document is
    // grammar-valid, so it is rejected ONLY because the EMBEDDED codebook was consulted
    // and found its terms uncovered.
    const vectors = join(root, "slices/grounding/lang/tests/gmn1-vectors");
    const positive = JSON.parse(
      mod.gmn_validate(new Uint8Array(await readFile(join(vectors, "claim-basic.gmn")))),
    );
    assert.equal(positive.conformant, true, "the frozen positive vector must validate");
    const negative = JSON.parse(
      mod.gmn_validate(
        new Uint8Array(await readFile(join(vectors, "negative-codec/neg-uncovered-term.gmn"))),
      ),
    );
    assert.equal(negative.conformant, false, "an uncovered term must be REJECTED");
    assert.equal(
      negative.failureClass,
      "https://blackcatinformatics.ca/lang/GmnUncoveredTerm",
      "the rejection must be the codebook-RESOLUTION class, proving the codebook is embedded",
    );

    // `bundle_dataset` is deliberately NOT driven here. It folds the whole carrier into
    // one N-Quads string, and over a full-size `gmeow.gts` that allocation exceeds what a
    // wasm32 linear memory can hold — the module traps, identically in this tree and from
    // the installed tarball, so it is an engine limit rather than a packaging defect and
    // driving it here would prove nothing about consumability. Its real consumer is the
    // documentation site, which calls it over the much smaller core-bundle projection.
    return "validate() over the shipped bundle + gmn_validate() over both frozen vectors";
  },

  "@blackcatinformatics/gmeow-reason-wasm": async (mod) => {
    await mod.ready();
    const closure = mod.reason(
      "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n" +
        "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n" +
        "@prefix ex: <https://example.org/> .\n" +
        "ex:Cat rdfs:subClassOf ex:Animal .\n" +
        "ex:felix rdf:type ex:Cat .\n",
      "turtle",
    );
    assert.ok(
      closure.includes("<https://example.org/felix>") && closure.includes("<https://example.org/Animal>"),
      `the installed reasoner did not entail ex:felix a ex:Animal:\n${closure}`,
    );
    return "reason() entailed the rdfs:subClassOf closure from the installed wasm";
  },

  "@blackcatinformatics/gmeow-gmn-wasm": async (mod) => {
    await mod.ready();
    // The COMMITTED native attestation, used as both input and expected output: reading
    // it back and re-encoding must be a fixed point, so this asserts against the same
    // bytes the native↔wasm witness pins without restating any test constant here.
    const attestation = await readFile(join(root, "crates/gmn-wasm/tests/WITNESS.gmn1.txt"), "utf8");
    const nquads = mod.from_gmn1(attestation);
    assert.equal(
      mod.to_gmn1(nquads, "nquads"),
      attestation,
      "the installed codec is not a fixed point over the committed native attestation",
    );
    const legend = JSON.parse(mod.glyph_legend());
    assert.ok(
      (Array.isArray(legend) ? legend : Object.keys(legend)).length > 0,
      "glyph_legend() must return the non-empty legend the engine embeds",
    );
    return "from_gmn1()/to_gmn1() round-tripped the committed attestation; glyph_legend() read";
  },

  "@blackcatinformatics/gmeow-mcp-wasm": async (mod) => {
    await mod.ready();
    assert.equal(mod.loaded(), false, "no snapshot is installed before init");
    mod.init(await snapshot());
    assert.equal(mod.loaded(), true, "init installs the engine");
    const listed = JSON.parse(
      mod.mcp('{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'),
    ).result.tools;
    assert.ok(listed.length > 0, "the installed engine advertises its tool surface");
    return `mcp() answered tools/list with ${listed.length} tools from the installed wasm`;
  },

  "@blackcatinformatics/gmeow-mcp-core-wasm": async (mod) => {
    await mod.ready();
    mod.initTiered(await snapshot());
    const listed = JSON.parse(
      mod.mcp('{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'),
    ).result.tools.map((t) => t.name);
    const deferred = mod.deferredTools();
    assert.ok(deferred.length > 0, "the lean core defers at least one tool");
    for (const tool of deferred) {
      assert.ok(listed.includes(tool), `deferral must be invisible to discovery: ${tool}`);
    }
    const frame = mod.mcp(
      `{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"${deferred[0]}","arguments":{}}}`,
    );
    const deferral = mod.segmentDeferral(frame);
    assert.notEqual(deferral, null, "a deferred tool raises the typed routing signal");
    assert.equal(deferral.segment, mod.deferredSegment());
    return `the installed lean core advertised ${listed.length} tools and deferred structurally`;
  },

  "@blackcatinformatics/gmeow-console": async (_mod, installed, manifest) => {
    // `element.mjs` (the `.` entry) extends `HTMLElement`, so it is a browser module by
    // construction and its lane is the Playwright smoke one. The DOM-free half is a real
    // published subpath and is exercised here against the installed bytes.
    const session = await import(
      pathToFileURL(join(installed, manifest.exports["./session.mjs"].import)).href
    );
    const s = new session.ConsoleSession({ id: "consume", now: (i) => `2026-01-01T00:00:0${i}Z` });
    s.record({
      tool: "convert",
      schema: "https://blackcatinformatics.ca/gmeow/action/convert",
      args: { from: "nt", to: "turtle" },
    });
    const nquads = s.trajectoryNQuads();
    for (const fact of [
      "https://blackcatinformatics.ca/gmeow/ToolCall",
      "https://blackcatinformatics.ca/logic/instantiatesSchema",
      "https://blackcatinformatics.ca/logic/properPartOf",
      "https://blackcatinformatics.ca/logic/transitionFromState",
    ]) {
      assert.ok(nquads.includes(fact), `the recorded trajectory must carry ${fact}`);
    }
    assert.deepEqual(
      session.decodePermalink(s.permalink()).calls.map((c) => c.tool),
      ["convert"],
      "the permalink must round-trip the invocation list",
    );
    assert.throws(
      () => session.exportSegment(s, undefined),
      "an export without the engine store must be refused, not half-emitted",
    );
    return "the installed session module recorded, exported and round-tripped a trajectory";
  },
};

// ── the lane ────────────────────────────────────────────────────────────────────────

const scratch = await mkdtemp(join(tmpdir(), "gmeow-consumable-"));
let failures = 0;

try {
  const packages = await publishedPackages();
  assert.ok(packages.length > 0, "no publishable package was discovered");

  for (const pkg of packages) {
    const name = pkg.manifest.name;
    const dir = fileURLToPath(pkg.dir);
    const witness = WITNESSES[name];
    if (witness === undefined) {
      console.error(`FAIL ${name}: no consumability witness — every published package needs one`);
      failures += 1;
      continue;
    }

    // 1. pack the REAL tarball.
    const packed = run("npm", ["pack", "--silent", "--pack-destination", scratch], dir)
      .trim()
      .split("\n")
      .pop();
    const tarball = join(scratch, packed);

    // 2. install it into a throwaway project, resolved BY NAME.
    const project = join(scratch, `consume-${packed.replace(/[^\w.-]/g, "_")}`);
    await writeFile(
      join(await mkdirp(project), "package.json"),
      JSON.stringify({ name: "gmeow-consumability-probe", private: true, type: "module" }, null, 2),
    );
    run("npm", ["install", "--no-audit", "--no-fund", "--ignore-scripts", tarball], project);

    const installed = join(project, "node_modules", ...name.split("/"));
    const installedManifest = JSON.parse(await readFile(join(installed, "package.json"), "utf8"));

    // 3. the witness, against the INSTALLED bytes.
    try {
      // Universal: the installed manifest's version, and export-set equality of the
      // installed entry against the installed declaration file.
      assert.equal(
        installedManifest.version,
        pkg.manifest.version,
        "the installed manifest version differs from the source manifest version",
      );
      for (const [subpath, entry] of Object.entries(installedManifest.exports)) {
        assert.deepEqual(
          [...exportedValueNames(await readFile(join(installed, entry.import), "utf8"))].sort(),
          [...exportedValueNames(await readFile(join(installed, entry.types), "utf8"))].sort(),
          `${name} ${subpath}: the INSTALLED module and its declarations disagree`,
        );
      }

      // The `.` entry of the console element extends `HTMLElement`, which is a
      // ReferenceError under Node by construction — that entry's lane is the browser
      // smoke one. Its DOM-free published subpath is exercised by the witness below, so
      // nothing about the console goes unproven here.
      const entryPath = join(installed, installedManifest.exports["."].import);
      const isBrowserEntry = /\bextends\s+HTMLElement\b/.test(await readFile(entryPath, "utf8"));
      const entryUrl = pathToFileURL(entryPath).href;
      const mod = isBrowserEntry ? null : await import(entryUrl);
      if (mod !== null) {
        // Instantiate FIRST: `version()` crosses the wasm boundary, so calling it before
        // `ready()` reads an uninstantiated module rather than reporting a version.
        await mod.ready();
        assert.equal(
          mod.version(),
          installedManifest.version,
          "the installed engine reports a different version than the installed manifest",
        );
      }
      const detail = await witness(mod, installed, installedManifest);
      console.log(`OK   ${name}@${installedManifest.version} — ${detail}`);
    } catch (error) {
      console.error(`FAIL ${name}: ${error.stack ?? error.message}`);
      failures += 1;
    }
  }
} finally {
  await rm(scratch, { recursive: true, force: true });
}

if (failures > 0) {
  console.error(`\nconsumability lane FAILED for ${failures} package(s)`);
  process.exit(1);
}
console.log("\nOK: every published package installs from its own tarball and works");

async function mkdirp(path) {
  await mkdir(path, { recursive: true });
  return path;
}
