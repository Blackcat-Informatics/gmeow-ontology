// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import assert from "node:assert/strict";
import { compactStreamable, CompactRefusedError } from "../src/compact.js";
import { Read } from "../src/reader.js";
import { Writer } from "../src/writer.js";
import { TermKind } from "../src/model.js";
import { hex, digestStr } from "../src/wire.js";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, "../../../../");
const vectorsDir = join(repoRoot, "generated", "gts-vectors");

test("compact reproduces the frozen 25b bytes exactly (§14.1 determinism)", () => {
    const source = new Uint8Array(
        readFileSync(join(vectorsDir, "25-streamable-source.gts")),
    );
    const frozen = new Uint8Array(
        readFileSync(join(vectorsDir, "25b-streamable-compacted.gts")),
    );
    const out = compactStreamable(source, {
        timestamp: "2026-01-01T00:00:00Z",
    });
    assert.equal(hex(out), hex(frozen), "compacted bytes differ from frozen");
});

test("compact output claims streamable with zero tail and verifies clean", () => {
    const source = new Uint8Array(
        readFileSync(join(vectorsDir, "25-streamable-source.gts")),
    );
    const out = compactStreamable(source, {
        timestamp: "2026-01-01T00:00:00Z",
    });
    const g = Read(out, true);
    assert.equal(g.diagnostics.length, 0);
    assert.equal(g.segmentStreamable.length, 1);
    assert.ok(g.segmentStreamable[0].claimed);
    assert.equal(g.segmentStreamable[0].tail, 0);
});

test("compact carries the reasoned suppression forward, ids shifted (§10.1)", () => {
    const source = new Uint8Array(
        readFileSync(join(vectorsDir, "25-streamable-source.gts")),
    );
    const src = Read(source, true);
    const out = Read(
        compactStreamable(source, { timestamp: "2026-01-01T00:00:00Z" }),
        true,
    );
    assert.equal(out.suppressions.length, src.suppressions.length);
    assert.equal(out.suppressions[0].reason, "superseded");
    const target = out.suppressions[0].targets[0];
    assert.ok(target instanceof Map);
    assert.equal(target.get("kind"), "term");
    const tid = target.get("id") as number;
    // The shifted target resolves to the same term value as in the source.
    const srcTarget = src.suppressions[0].targets[0] as Map<unknown, unknown>;
    const srcTid = srcTarget.get("id") as number;
    assert.equal(out.terms[tid].value, src.terms[srcTid].value);
});

test("compact refuses an evidence input without --seal-original (§10.1)", () => {
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
    const data = w.toBytes();
    assert.throws(
        () => compactStreamable(data, { timestamp: "2026-01-01T00:00:00Z" }),
        CompactRefusedError,
    );
    const sealed = compactStreamable(data, {
        timestamp: "2026-01-01T00:00:00Z",
        sealOriginal: true,
    });
    const g = Read(sealed, true);
    assert.equal(g.diagnostics.length, 0);
    // The sealed original travels as a blob, role "source", byte-intact.
    const sealedDigest = digestStr(data);
    const blob = g.blobs.find((b) => b.digest === sealedDigest);
    assert.ok(blob, "sealed original blob missing");
    assert.equal(hex(blob.data), hex(data));
});

test("compact is reproducible for a fixed timestamp", () => {
    const source = new Uint8Array(
        readFileSync(join(vectorsDir, "25-streamable-source.gts")),
    );
    const a = compactStreamable(source, { timestamp: "2026-01-01T00:00:00Z" });
    const b = compactStreamable(source, { timestamp: "2026-01-01T00:00:00Z" });
    assert.equal(hex(a), hex(b));
});
