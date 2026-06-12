// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

import { Tagged } from "cbor";
import * as wire from "./wire.js";
import { Term, Quad, ReifierEntry, Triple, TermKind } from "./model.js";

interface CatalogEntry {
    name: string;
    cls: string;
}

const Catalog: Record<number, CatalogEntry> = {
    0: { name: "identity", cls: "encode" },
    1: { name: "gzip", cls: "compress" },
    2: { name: "zstd", cls: "compress" },
    7: { name: "cose-encrypt0", cls: "encrypt" },
};

function termToWire(t: Term): Map<unknown, unknown> {
    const entries = new Map<unknown, unknown>();
    entries.set("k", t.kind);
    if (t.value !== "" || t.kind === TermKind.Literal) {
        entries.set("v", t.value);
    }
    if (t.datatype !== undefined) entries.set("dt", t.datatype);
    if (t.lang) entries.set("l", t.lang);
    if (t.reifier !== undefined) entries.set("rf", t.reifier);
    return entries;
}

/** Deterministic GTS writer. */
export class Writer {
    private nameToId: Map<string, number>;
    private prev: Uint8Array;
    private buf: Uint8Array;

    constructor(profile: string) {
        this.nameToId = new Map<string, number>();
        const catEntries = new Map<unknown, unknown>();
        for (const [id, c] of Object.entries(Catalog)) {
            const nid = Number(id);
            this.nameToId.set(c.name, nid);
            const ce = new Map<unknown, unknown>();
            ce.set("name", c.name);
            ce.set("cls", c.cls);
            catEntries.set(nid, ce);
        }
        const header = new Map<unknown, unknown>();
        header.set("gts", wire.Magic);
        header.set("v", wire.Version);
        header.set("prof", profile);
        header.set("cat", catEntries);
        const id = wire.headerId(header);
        header.set("id", id);
        const tagged = new Tagged(wire.SelfDescribeTag, header);
        this.prev = id;
        this.buf = wire.encode(tagged);
    }

    /** The id the next appended frame must reference as "prev". */
    head(): Uint8Array {
        return new Uint8Array(this.prev);
    }

    private chainIds(chain: string[]): unknown[] {
        return chain.map((name) => {
            const id = this.nameToId.get(name);
            if (id === undefined) throw new Error(`unknown codec '${name}'`);
            return id;
        });
    }

    /**
     * Append one frame and return its "id".
     * payload and raw are mutually exclusive. transform names a codec chain
     * (only "identity" is supported by this writer).
     */
    addFrame(
        frameType: string,
        payload?: unknown,
        raw?: Uint8Array,
        transform?: string[],
        pubMeta?: unknown,
    ): Uint8Array {
        if (payload !== undefined && raw !== undefined) {
            throw new Error("payload and raw are mutually exclusive");
        }
        const frame = new Map<unknown, unknown>();
        frame.set("t", frameType);

        let data: unknown = undefined;
        if (transform && transform.length > 0) {
            if (raw === undefined && payload === undefined) {
                throw new Error("transform requires a raw or payload source");
            }
            for (const name of transform) {
                if (name !== "identity") {
                    throw new Error(
                        "non-identity transforms require the Python producer",
                    );
                }
            }
            const source = raw ?? wire.mustEncode(payload);
            frame.set("x", this.chainIds(transform));
            data = source;
        } else if (raw !== undefined) {
            data = raw;
        } else if (payload !== undefined) {
            data = payload;
        }

        if (data !== undefined) frame.set("d", data);
        if (pubMeta !== undefined) frame.set("pub", pubMeta);
        frame.set("prev", this.prev);

        const id = wire.contentId(frame);
        frame.set("id", id);

        const encoded = wire.encode(frame);
        const combined = new Uint8Array(this.buf.length + encoded.length);
        combined.set(this.buf);
        combined.set(encoded, this.buf.length);
        this.buf = combined;
        this.prev = id;
        return id;
    }

    addTerms(terms: Term[]): Uint8Array {
        const rows = terms.map((t) => termToWire(t));
        return this.addFrame("terms", rows);
    }

    addQuads(quads: Quad[]): Uint8Array {
        const rows = quads.map((q) => {
            const row: unknown[] = [q.s, q.p, q.o];
            if (q.g !== undefined) row.push(q.g);
            return row;
        });
        return this.addFrame("quads", rows);
    }

    addReifies(bindings: ReifierEntry[]): Uint8Array {
        const m = new Map<unknown, unknown>();
        for (const b of bindings) {
            m.set(b.rid, [b.spo.s, b.spo.p, b.spo.o]);
        }
        return this.addFrame("reifies", m);
    }

    addAnnot(rows: Triple[]): Uint8Array {
        const arr = rows.map((r) => [r.s, r.p, r.o]);
        return this.addFrame("annot", arr);
    }

    addBlob(data: Uint8Array, mt?: string): Uint8Array {
        let pub: Map<unknown, unknown> | undefined;
        if (mt) {
            pub = new Map<unknown, unknown>();
            pub.set("mt", mt);
        }
        return this.addFrame("blob", undefined, data, undefined, pub);
    }

    addMeta(meta: Map<unknown, unknown>): Uint8Array {
        return this.addFrame("meta", meta);
    }

    addSuppress(targets: unknown[], reason?: string): Uint8Array {
        const payload = new Map<unknown, unknown>();
        payload.set("targets", targets);
        if (reason) payload.set("reason", reason);
        return this.addFrame("suppress", payload);
    }

    toBytes(): Uint8Array {
        return new Uint8Array(this.buf);
    }
}

/** Pack bytes into a blake3:<hex> digest string. */
export function digestString(data: Uint8Array): string {
    return wire.digestStr(data);
}
