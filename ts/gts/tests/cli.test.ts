// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import assert from "node:assert/strict";

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
