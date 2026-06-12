// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import assert from "node:assert/strict";
import { Writer } from "../src/writer.js";
import { TermKind } from "../src/model.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, "../../../../");
const cli = resolve(__dirname, "../bin/gts.js");
const vectorsDir = join(repoRoot, "generated", "gts-vectors");

function run(
    args: string[],
    opts?: { cwd?: string; input?: Uint8Array },
): { code: number; stdout: string; stderr: string } {
    try {
        const stdout = execFileSync("node", [cli, ...args], {
            cwd: opts?.cwd,
            input: opts?.input,
            encoding: "utf8",
            stdio: [opts?.input ? "pipe" : "ignore", "pipe", "pipe"],
        });
        return { code: 0, stdout, stderr: "" };
    } catch (e) {
        const err = e as { status?: number; stdout?: string; stderr?: string };
        return {
            code: err.status ?? 1,
            stdout: err.stdout ?? "",
            stderr: err.stderr ?? "",
        };
    }
}

test("CLI fold emits N-Quads for a clean vector", () => {
    const r = run(["fold", join(vectorsDir, "01-minimal.gts")]);
    assert.equal(r.code, 0);
    assert.match(r.stdout, /<https:\/\/example.org\/Cat>/);
});

test("CLI verify reports diagnostics for damaged vector", () => {
    const r = run(["verify", join(vectorsDir, "04-damaged-frame.gts")]);
    assert.equal(r.code, 1);
    assert.match(r.stdout + r.stderr, /DamagedFrame/);
});

test("CLI ls lists inline blobs", () => {
    const r = run(["ls", join(vectorsDir, "22-inline-blob.gts")]);
    assert.equal(r.code, 0);
    assert.match(r.stdout, /blake3:/);
    assert.match(r.stdout, /image\/webp/);
});

test("CLI info prints segment ledger", () => {
    const r = run(["info", join(vectorsDir, "15-two-segment-union.gts")]);
    assert.equal(r.code, 0);
    assert.match(r.stdout, /2 segment\(s\)/);
});

test("CLI pack and unpack round-trip", () => {
    const tmp = mkdtempSync(join(tmpdir(), "gts-cli-"));
    const src = join(tmp, "src");
    mkdirSync(src);
    writeFileSync(join(src, "hello.txt"), "hello");
    const archive = join(tmp, "out.gts");

    const pack = run(["pack", src, "-o", archive]);
    assert.equal(pack.code, 0, pack.stderr);

    const dest = join(tmp, "dest");
    mkdirSync(dest);
    const unpack = run(["unpack", archive, "-C", dest]);
    assert.equal(unpack.code, 0, unpack.stderr);

    const content = readFileSync(join(dest, "hello.txt"), "utf8");
    assert.equal(content, "hello");
});

test("CLI diff reports no changes for identical tree", () => {
    const tmp = mkdtempSync(join(tmpdir(), "gts-cli-"));
    const src = join(tmp, "src");
    mkdirSync(src);
    writeFileSync(join(src, "a.txt"), "a");
    const archive = join(tmp, "out.gts");

    const pack = run(["pack", src, "-o", archive]);
    assert.equal(pack.code, 0, pack.stderr);

    const diff = run(["diff", archive, src]);
    assert.equal(diff.code, 0, diff.stdout);
});

test("CLI compact round-trips: verify exit 0 with a layout line", () => {
    const tmp = mkdtempSync(join(tmpdir(), "gts-cli-"));
    const out = join(tmp, "streamable.gts");
    const r = run([
        "compact",
        join(vectorsDir, "25-streamable-source.gts"),
        "-o",
        out,
        "--streamable",
        "--timestamp",
        "2026-01-01T00:00:00Z",
    ]);
    assert.equal(r.code, 0, r.stderr);
    const v = run(["verify", out]);
    assert.equal(v.code, 0, v.stdout + v.stderr);
    assert.match(v.stdout, /layout: streamable through frame/);
    assert.doesNotMatch(v.stdout, /accretive tail/);
    assert.doesNotMatch(v.stderr, /warning/);
});

test("CLI verify refuses the streamable lie (vector 26)", () => {
    const r = run(["verify", join(vectorsDir, "26-streamable-lie.gts")]);
    assert.equal(r.code, 1);
    assert.match(r.stdout, /StreamableLayoutError/);
});

test("CLI info reports the accretive tail (vector 27)", () => {
    const r = run(["info", join(vectorsDir, "27-streamable-tail.gts")]);
    assert.equal(r.code, 0, r.stderr);
    assert.match(r.stdout, /layout: streamable through frame/);
    assert.match(r.stdout, /accretive tail 2 frame\(s\)/);
});

test("CLI compact without --streamable exits 2", () => {
    const tmp = mkdtempSync(join(tmpdir(), "gts-cli-"));
    const r = run([
        "compact",
        join(vectorsDir, "25-streamable-source.gts"),
        "-o",
        join(tmp, "x.gts"),
    ]);
    assert.equal(r.code, 2);
    assert.match(r.stderr, /compact requires --streamable/);
});

test("CLI compact refuses evidence input, then seals on request", () => {
    const tmp = mkdtempSync(join(tmpdir(), "gts-cli-"));
    const w = new Writer("evidence");
    w.addTerms([
        { kind: TermKind.Iri, value: "https://example.org/Cat" },
        {
            kind: TermKind.Iri,
            value: "http://www.w3.org/2000/01/rdf-schema#label",
        },
        { kind: TermKind.Literal, value: "Cat", lang: "en" },
    ]);
    w.addQuads([{ s: 0, p: 1, o: 2 }]);
    const path = join(tmp, "evidence.gts");
    writeFileSync(path, w.toBytes());
    const out = join(tmp, "out.gts");

    const refused = run(["compact", path, "-o", out, "--streamable"]);
    assert.equal(refused.code, 1);
    assert.match(refused.stderr, /refusing compact: .*seal-original/);

    const sealed = run([
        "compact",
        path,
        "-o",
        out,
        "--streamable",
        "--seal-original",
    ]);
    assert.equal(sealed.code, 0, sealed.stderr);
    const v = run(["verify", out]);
    assert.equal(v.code, 0, v.stdout + v.stderr);
});

test("CLI cat composes two clean segments", () => {
    const tmp = mkdtempSync(join(tmpdir(), "gts-cli-"));
    const a = join(vectorsDir, "01-minimal.gts");
    const b = join(vectorsDir, "01-minimal.gts");
    const out = join(tmp, "composed.gts");
    const r = run(["cat", "-o", out, a, b]);
    assert.equal(r.code, 0, r.stderr);
    const folded = run(["fold", out]);
    assert.equal(folded.code, 0);
    assert.match(folded.stdout, /Cat/);
});
