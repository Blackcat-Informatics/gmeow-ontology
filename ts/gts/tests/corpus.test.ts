// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

import { readFileSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import assert from "node:assert/strict";
import { Read } from "../src/reader.js";
import { toNQuads } from "../src/nquads.js";
import { hex } from "../src/wire.js";

const __dirname = dirname(fileURLToPath(import.meta.url));

interface ExpectedBlobs {
    [digest: string]: { mt?: string; size: number };
}

interface ExpectedStreamable {
    claimed: boolean;
    covered: number;
    tail: number;
}

interface Expected {
    blobs: ExpectedBlobs;
    diagnostics: string[];
    mode?: string;
    nquads: string[];
    opaque_reasons: string[];
    profiles: string[];
    quads: number;
    segment_heads: string[];
    segments: number;
    streamable?: ExpectedStreamable[];
    suppressions: number;
    terms: number;
}

const repoRoot = resolve(__dirname, "../../../../");
const vectorsDir = join(repoRoot, "generated", "gts-vectors");

function vectorNames(): string[] {
    const files = readdirSync(vectorsDir);
    const names = new Set<string>();
    for (const f of files) {
        if (f.endsWith(".gts")) names.add(f.replace(/\.gts$/, ""));
    }
    return [...names].sort();
}

function sorted(arr: string[]): string[] {
    return [...arr].sort();
}

for (const name of vectorNames()) {
    test(`corpus vector ${name}`, () => {
        const gtsPath = join(vectorsDir, `${name}.gts`);
        const expectedPath = join(vectorsDir, `${name}.expected.json`);
        const data = new Uint8Array(readFileSync(gtsPath));
        const expected: Expected = JSON.parse(
            readFileSync(expectedPath, "utf8"),
        );

        const allowSegments = expected.mode !== "pre-segment";
        const g = Read(data, allowSegments);

        assert.equal(g.terms.length, expected.terms, "terms count");
        assert.equal(g.quads.length, expected.quads, "quads count");
        assert.equal(
            g.segmentHeads.length,
            expected.segments,
            "segments count",
        );
        assert.equal(
            g.suppressions.length,
            expected.suppressions,
            "suppressions count",
        );

        assert.deepEqual(
            sorted(g.segmentProfiles),
            sorted(expected.profiles),
            "profiles",
        );
        assert.deepEqual(
            sorted(g.segmentHeads.map(hex)),
            sorted(expected.segment_heads),
            "segment heads",
        );
        assert.deepEqual(
            sorted(g.diagnostics.map((d) => d.code)),
            sorted(expected.diagnostics),
            "diagnostics",
        );
        assert.deepEqual(
            sorted(g.opaque.map((o) => o.reason)),
            sorted(expected.opaque_reasons),
            "opaque reasons",
        );

        if (expected.streamable !== undefined) {
            assert.deepEqual(
                g.segmentStreamable.map((si) => ({
                    claimed: si.claimed,
                    covered: si.covered,
                    tail: si.tail,
                })),
                expected.streamable,
                "streamable layout state",
            );
        }

        const actualNQuads = toNQuads(g)
            .split("\n")
            .filter((l) => l.trim() !== "");
        assert.deepEqual(
            sorted(actualNQuads),
            sorted(expected.nquads),
            "N-Quads",
        );

        const actualBlobs: ExpectedBlobs = {};
        for (const b of g.blobs) {
            let mt = "";
            for (const bm of g.blobMeta) {
                if (bm.digest === b.digest && bm.meta instanceof Map) {
                    mt = String(bm.meta.get("mt") ?? "");
                }
            }
            actualBlobs[b.digest] = {
                mt: mt || undefined,
                size: b.data.length,
            };
        }
        assert.deepEqual(actualBlobs, expected.blobs, "blobs");
    });
}
