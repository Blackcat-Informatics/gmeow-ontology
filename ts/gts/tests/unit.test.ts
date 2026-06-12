// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import assert from "node:assert/strict";
import { Graph, TermKind } from "../src/model.js";
import * as wire from "../src/wire.js";
import { Writer } from "../src/writer.js";
import { Read } from "../src/reader.js";
import { toNQuads } from "../src/nquads.js";
import { identity, gzip } from "../src/codec.js";

const __dirname = dirname(fileURLToPath(import.meta.url));

test("wire.encode round-trips values", () => {
    const m = new Map<unknown, unknown>();
    m.set("a", 1);
    m.set("b", new Uint8Array([0, 1, 2]));
    const encoded = wire.encode(m);
    assert.ok(encoded.length > 0);
    const decoded = wire.decodeFirst(encoded);
    assert.ok(decoded instanceof Map);
    assert.equal(decoded.get("a"), 1);
    const bytes = wire.asBytes(decoded.get("b"));
    assert.ok(bytes);
    assert.deepEqual(bytes, new Uint8Array([0, 1, 2]));
});

test("blake3_256 returns 32 bytes", () => {
    const h = wire.blake3_256(new Uint8Array([0]));
    assert.equal(h.length, 32);
});

test("codec identity round-trip", () => {
    const data = new TextEncoder().encode("hello gts");
    assert.deepEqual(identity.decode(identity.encode(data)), data);
});

test("codec gzip round-trip", () => {
    const data = new TextEncoder().encode("hello gts hello gts");
    assert.deepEqual(gzip.decode(gzip.encode(data)), data);
});

test("codec zstd decodes the zstd corpus vector", () => {
    const path = resolve(
        __dirname,
        "../../../../generated/gts-vectors/02-zstd-frame.gts",
    );
    const g = Read(readFileSync(path), false);
    assert.equal(g.diagnostics.length, 0);
    assert.ok(g.quads.length > 0);
});

test("writer produces a readable GTS log", () => {
    const w = new Writer("dist");
    const t1 = { kind: TermKind.Iri, value: "https://example.org/Cat" };
    const t2 = { kind: TermKind.Literal, value: "Cat", lang: "en" };
    const t3 = {
        kind: TermKind.Iri,
        value: "http://www.w3.org/2000/01/rdf-schema#label",
    };
    w.addTerms([t1, t2, t3]);
    w.addQuads([{ s: 0, p: 2, o: 1 }]);
    const data = w.toBytes();
    const g = Read(data, false);
    assert.equal(g.terms.length, 3);
    assert.equal(g.quads.length, 1);
    assert.equal(g.segmentProfiles[0], "dist");
    assert.equal(g.diagnostics.length, 0);
});

test("toNQuads serialises a simple graph", () => {
    const g = new Graph();
    g.terms.push(
        { kind: TermKind.Iri, value: "https://example.org/Cat" },
        { kind: TermKind.Literal, value: "Cat", lang: "en" },
        {
            kind: TermKind.Iri,
            value: "http://www.w3.org/2000/01/rdf-schema#label",
        },
    );
    g.quads.push({ s: 0, p: 2, o: 1 });
    const out = toNQuads(g);
    assert.equal(
        out.trim(),
        '<https://example.org/Cat> <http://www.w3.org/2000/01/rdf-schema#label> "Cat"@en .',
    );
});

test("reader rejects a torn file", () => {
    const w = new Writer("generic");
    w.addTerms([{ kind: TermKind.Iri, value: "https://example.org/A" }]);
    const data = w.toBytes();
    const torn = data.subarray(0, data.length - 4);
    const g = Read(torn, false);
    assert.ok(g.diagnostics.some((d) => d.code === "TornAppendError"));
});

test("reader allows clean multi-segment file", () => {
    const w1 = new Writer("dist");
    w1.addTerms([
        { kind: TermKind.Iri, value: "https://example.org/Cat" },
        { kind: TermKind.Literal, value: "Cat", lang: "en" },
        {
            kind: TermKind.Iri,
            value: "http://www.w3.org/2000/01/rdf-schema#label",
        },
    ]);
    w1.addQuads([{ s: 0, p: 2, o: 1 }]);

    const w2 = new Writer("dist");
    w2.addTerms([
        { kind: TermKind.Iri, value: "https://example.org/Dog" },
        { kind: TermKind.Literal, value: "Dog", lang: "en" },
        {
            kind: TermKind.Iri,
            value: "http://www.w3.org/2000/01/rdf-schema#label",
        },
    ]);
    w2.addQuads([{ s: 0, p: 2, o: 1 }]);

    // Concatenate segments manually (Writer produces a full header each time).
    const combined = new Uint8Array(w1.toBytes().length + w2.toBytes().length);
    combined.set(w1.toBytes());
    combined.set(w2.toBytes(), w1.toBytes().length);

    const g = Read(combined, true);
    assert.equal(g.segmentHeads.length, 2);
    assert.equal(g.quads.length, 2);
});
