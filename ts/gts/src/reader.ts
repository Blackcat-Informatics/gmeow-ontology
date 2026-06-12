// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

import cbor, { Tagged } from "cbor";
import {
    Graph,
    Quad,
    TermKind,
    Triple,
    type Suppression,
    type Diagnostic,
} from "./model.js";
import * as wire from "./wire.js";
import { decodeChain, isCodecError, type Codec } from "./codec.js";

interface PayloadError {
    unavailable: boolean;
    reason: string;
    detail: string;
    damaged: boolean;
}

function payloadErrorFromCodecError(err: unknown): PayloadError {
    if (isCodecError(err)) {
        return {
            unavailable: !err.failed,
            reason: err.reason,
            detail: err.message,
            damaged: err.failed,
        };
    }
    return {
        unavailable: false,
        reason: "",
        detail: err instanceof Error ? err.message : String(err),
        damaged: true,
    };
}

function isPayloadError(x: Codec[] | PayloadError): x is PayloadError {
    return "unavailable" in x;
}

class Folder {
    g: Graph;
    catalog: Map<number, Codec>;

    constructor(g: Graph, catalog: Map<number, Codec>) {
        this.g = g;
        this.catalog = catalog;
    }

    diag(code: string, detail: string, index?: number): void {
        this.g.diagnostics.push({ code, detail, frameIndex: index });
    }

    resolveCodecs(ids: unknown[]): Codec[] | PayloadError {
        const chain: Codec[] = [];
        for (const cid of ids) {
            const n = wire.asInt64(cid);
            if (n === undefined) {
                return {
                    unavailable: true,
                    reason: "unknown-codec",
                    detail: `codec id ${String(cid)} not an integer`,
                    damaged: false,
                };
            }
            const c = this.catalog.get(n);
            if (!c) {
                return {
                    unavailable: true,
                    reason: "unknown-codec",
                    detail: `codec id ${n} not in catalog`,
                    damaged: false,
                };
            }
            chain.push(c);
        }
        return chain;
    }

    payload(
        frame: Map<unknown, unknown>,
        isBlob: boolean,
    ): { value: unknown | null; err?: PayloadError } {
        const d = wire.mapGet(frame, "d");
        const x = wire.mapGet(frame, "x");
        if (x !== undefined) {
            if (!Array.isArray(x)) {
                return {
                    value: null,
                    err: {
                        unavailable: false,
                        reason: "",
                        damaged: true,
                        detail: "transform field 'x' must be an array",
                    },
                };
            }
            if (x.length > 0) {
                const b = wire.asBytes(d);
                if (!b) {
                    return {
                        value: null,
                        err: {
                            unavailable: false,
                            reason: "",
                            damaged: true,
                            detail: "transformed frame 'd' must be a byte string",
                        },
                    };
                }
                const chain = this.resolveCodecs(x);
                if (isPayloadError(chain)) {
                    return { value: null, err: chain };
                }
                let decoded: Uint8Array;
                try {
                    decoded = decodeChain(chain, b);
                } catch (e) {
                    return { value: null, err: payloadErrorFromCodecError(e) };
                }
                if (isBlob) return { value: decoded };
                try {
                    const out = cbor.decodeFirstSync(Buffer.from(decoded), {
                        preferMap: true,
                    });
                    return { value: out };
                } catch (e) {
                    return {
                        value: null,
                        err: {
                            unavailable: false,
                            reason: "",
                            damaged: true,
                            detail: `payload decode failed: ${(e as Error).message}`,
                        },
                    };
                }
            }
        }
        if (d === undefined) return { value: null };
        return { value: d };
    }

    foldFrame(frame: Map<unknown, unknown>, index: number): void {
        const ftype = wire.textOr(wire.mapGet(frame, "t"), "");
        const { value: payload, err: perr } = this.payload(
            frame,
            ftype === "blob",
        );
        if (perr) {
            if (perr.unavailable) {
                this.opaque(frame, ftype, perr.reason);
                this.diag(diagCodeFor(perr.reason), perr.detail, index);
            } else {
                this.opaque(frame, ftype, "damaged");
                this.diag(
                    "DamagedFrame",
                    `payload decode failed: ${perr.detail}`,
                    index,
                );
            }
            return;
        }
        switch (ftype) {
            case "terms":
                this.hTerms(payload, index);
                break;
            case "quads":
                this.hQuads(payload, index);
                break;
            case "reifies":
                this.hReifies(payload, index);
                break;
            case "annot":
                this.hAnnot(payload, index);
                break;
            case "blob":
                this.hBlob(payload as Uint8Array | null, frame);
                break;
            case "meta":
                this.hMeta(payload);
                break;
            case "suppress":
                this.hSuppress(payload);
                break;
            case "snapshot":
                this.hSnapshot(payload, index);
                break;
            case "opaque":
                this.hOpaque(payload);
                break;
            default:
                this.opaque(frame, ftype, "unknown-frame-type");
                this.diag(
                    "UnknownFrameType",
                    `unsupported frame type '${ftype}'`,
                    index,
                );
        }
    }

    hTerms(payload: unknown, index: number): void {
        const rows = Array.isArray(payload) ? payload : undefined;
        if (!rows) return;
        for (const raw of rows) {
            const entries = raw instanceof Map ? raw : undefined;
            if (!entries) continue;
            const k = wire.asInt64(wire.mapGet(entries, "k"));
            const resolvedKind =
                typeof k === "number" ? termKindFromWire(k) : TermKind.Iri;
            let value = "";
            const v = wire.mapGet(entries, "v");
            if (v !== undefined) {
                const s = wire.asText(v);
                if (s !== undefined) value = s;
            }
            let lang = "";
            const l = wire.mapGet(entries, "l");
            if (l !== undefined) {
                const s = wire.asText(l);
                if (s !== undefined) lang = s;
            }
            const dtRaw = wire.mapGet(entries, "dt");
            const rfRaw = wire.mapGet(entries, "rf");
            const tid = this.g.terms.length;
            const sanitize = (r: unknown): number | undefined => {
                if (r === undefined || r === null) return undefined;
                const n = wire.asInt64(r);
                if (n === undefined || n < 0 || n >= tid) return undefined;
                return n;
            };
            const outOfRange = (r: unknown): boolean => {
                const n = wire.asInt64(r);
                return n !== undefined && n >= tid;
            };
            const datatype = sanitize(dtRaw);
            const reifier = sanitize(rfRaw);
            if (outOfRange(dtRaw) || outOfRange(rfRaw)) {
                this.diag(
                    "ForwardReference",
                    `term ${tid} has an out-of-range ref`,
                    index,
                );
            }
            this.g.terms.push({
                kind: resolvedKind,
                value,
                datatype,
                lang,
                reifier,
            });
        }
    }

    hQuads(payload: unknown, index: number): void {
        const rows = Array.isArray(payload) ? payload : undefined;
        if (!rows) return;
        for (const row of rows) {
            const items = Array.isArray(row) ? row : undefined;
            if (!items || items.length < 3) continue;
            const s = wire.asInt(items[0]);
            const p = wire.asInt(items[1]);
            const o = wire.asInt(items[2]);
            let gslot: number | undefined;
            const hasGraph = items.length >= 4;
            if (hasGraph) {
                const g = wire.asInt(items[3]);
                if (g !== undefined) gslot = g;
            }
            if (
                s === undefined ||
                p === undefined ||
                o === undefined ||
                (hasGraph && gslot === undefined)
            ) {
                this.diag(
                    "DamagedFrame",
                    "quad has non-integer term ids",
                    index,
                );
                continue;
            }
            if (!this.checkPositions(s, p, o, gslot, index)) continue;
            this.g.quads.push({ s, p, o, g: gslot });
        }
    }

    hReifies(payload: unknown, index: number): void {
        const entries = payload instanceof Map ? payload : undefined;
        if (!entries) return;
        for (const [k, spo] of entries) {
            const rid = wire.asInt64(k);
            if (rid === undefined) continue;
            const items = Array.isArray(spo) ? spo : undefined;
            if (!items || items.length !== 3) continue;
            const s = wire.asInt(items[0]);
            const p = wire.asInt(items[1]);
            const o = wire.asInt(items[2]);
            const n = this.g.terms.length;
            const ridOk = rid >= 0 && rid < n;
            const spoOk =
                s !== undefined &&
                p !== undefined &&
                o !== undefined &&
                s < n &&
                p < n &&
                o < n;
            if (!ridOk || !spoOk) {
                this.diag(
                    "DamagedFrame",
                    `reifier ${rid} has bad/out-of-range ids`,
                    index,
                );
                continue;
            }
            const irid = rid;
            const newSpo: Triple = { s, p, o };
            const existing = this.g.reifier(irid);
            if (existing && !tripleEqual(existing, newSpo)) {
                this.diag(
                    "ConflictingReifier",
                    `reifier ${irid} rebound`,
                    index,
                );
                continue;
            }
            this.g.setReifier(irid, newSpo);
        }
    }

    hAnnot(payload: unknown, index: number): void {
        const rows = Array.isArray(payload) ? payload : undefined;
        if (!rows) return;
        for (const row of rows) {
            const items = Array.isArray(row) ? row : undefined;
            if (!items || items.length !== 3) continue;
            const r = wire.asInt(items[0]);
            const p = wire.asInt(items[1]);
            const v = wire.asInt(items[2]);
            const n = this.g.terms.length;
            if (
                r === undefined ||
                p === undefined ||
                v === undefined ||
                r >= n ||
                p >= n ||
                v >= n
            ) {
                this.diag(
                    "DamagedFrame",
                    "annot row has bad/out-of-range ids",
                    index,
                );
                continue;
            }
            if (this.g.terms[p].kind !== TermKind.Iri) {
                this.diag(
                    "PositionConstraint",
                    `annot predicate ${p} not an IRI`,
                    index,
                );
                continue;
            }
            this.g.annotations.push({ s: r, p, o: v });
        }
    }

    hBlob(payload: Uint8Array | null, frame: Map<unknown, unknown>): void {
        if (!payload) return;
        const digest = wire.digestStr(payload);
        const pub = wire.mapGet(frame, "pub");
        if (pub instanceof Map) {
            this.g.setBlobMeta(digest, pub);
        }
        this.g.setBlob(digest, payload);
    }

    hMeta(payload: unknown): void {
        const entries = payload instanceof Map ? payload : undefined;
        if (!entries) return;
        for (const [k, v] of entries) {
            let key = String(k);
            const s = wire.asText(k);
            if (s !== undefined) key = s;
            this.g.setMeta(key, v);
        }
    }

    hSuppress(payload: unknown): void {
        const entries = payload instanceof Map ? payload : undefined;
        if (!entries) return;
        const targetsRaw = wire.mapGet(entries, "targets");
        if (!Array.isArray(targetsRaw)) return;
        const filtered: unknown[] = [];
        for (const t of targetsRaw) {
            if (t instanceof Map) filtered.push(t);
        }
        const sup: Suppression = {
            targets: filtered,
            reason: wire.textOr(wire.mapGet(entries, "reason"), ""),
        };
        const by = wire.asInt(wire.mapGet(entries, "by"));
        if (by !== undefined) sup.by = by;
        this.g.suppressions.push(sup);
    }

    hSnapshot(payload: unknown, index: number): void {
        const entries = payload instanceof Map ? payload : undefined;
        if (!entries) return;
        const base = this.g.terms.length;
        const shift = (v: unknown): unknown => {
            const n = wire.asInt(v);
            if (n !== undefined) return n + base;
            return v;
        };
        const shiftRow = (row: unknown): unknown => {
            const items = Array.isArray(row) ? row : undefined;
            if (!items) return row;
            return items.map((it) => shift(it));
        };

        const snapTerms = wire.mapGet(entries, "terms");
        if (Array.isArray(snapTerms)) {
            const shifted = snapTerms.map((raw) => {
                const termMap = raw instanceof Map ? raw : undefined;
                if (!termMap) return raw;
                const newEntries = new Map<unknown, unknown>();
                for (const [k, v] of termMap) {
                    let nv = v;
                    const sk = wire.asText(k);
                    if (sk === "dt" || sk === "rf") nv = shift(v);
                    newEntries.set(k, nv);
                }
                return newEntries;
            });
            this.hTerms(shifted, index);
        }
        const quads = wire.mapGet(entries, "quads");
        if (Array.isArray(quads)) {
            this.hQuads(
                quads.map((row) => shiftRow(row)),
                index,
            );
        }
        const reifies = wire.mapGet(entries, "reifies");
        if (reifies instanceof Map) {
            const shifted = new Map<unknown, unknown>();
            for (const [k, v] of reifies) {
                shifted.set(shift(k), shiftRow(v));
            }
            this.hReifies(shifted, index);
        }
        const annot = wire.mapGet(entries, "annot");
        if (Array.isArray(annot)) {
            this.hAnnot(
                annot.map((row) => shiftRow(row)),
                index,
            );
        }
        const blobs = wire.mapGet(entries, "blobs");
        if (blobs instanceof Map) {
            for (const v of blobs.values()) {
                const b = wire.asBytes(v);
                if (b) this.g.setBlob(wire.digestStr(b), b);
            }
        }
        const meta = wire.mapGet(entries, "meta");
        if (meta instanceof Map) {
            for (const [k, v] of meta) {
                let key = String(k);
                const s = wire.asText(k);
                if (s !== undefined) key = s;
                this.g.setMeta(key, v);
            }
        }
    }

    hOpaque(payload: unknown): void {
        const entries = payload instanceof Map ? payload : undefined;
        if (!entries) return;
        let id: Uint8Array | undefined;
        const idv = wire.mapGet(entries, "id");
        const b = wire.asBytes(idv);
        if (b) id = b;
        this.g.opaque.push({
            id: id ?? new Uint8Array(),
            frameType: wire.textOr(wire.mapGet(entries, "type"), "opaque"),
            reason: wire.textOr(
                wire.mapGet(entries, "reason"),
                "unknown-codec",
            ),
            sigStat: wire.textOr(wire.mapGet(entries, "sigstat"), "none"),
            pubMeta: wire.mapGet(entries, "pub"),
            recipients: [],
        });
    }

    checkPositions(
        s: number,
        p: number,
        o: number,
        g: number | undefined,
        index: number,
    ): boolean {
        const n = this.g.terms.length;
        const inBounds = s < n && p < n && o < n && (g === undefined || g < n);
        if (!inBounds) {
            this.diag(
                "PositionConstraint",
                `quad (${s},${p},${o},${g === undefined ? "None" : g}) has out-of-range term ids`,
                index,
            );
            return false;
        }
        let ok = this.g.terms[p].kind === TermKind.Iri;
        if (this.g.terms[s].kind === TermKind.Literal) ok = false;
        if (g !== undefined) {
            const kind = this.g.terms[g].kind;
            if (kind === TermKind.Literal || kind === TermKind.Triple)
                ok = false;
        }
        if (!ok) {
            this.diag(
                "PositionConstraint",
                `quad (${s},${p},${o},${g === undefined ? "None" : g}) violates positions`,
                index,
            );
        }
        return ok;
    }

    opaque(frame: Map<unknown, unknown>, ftype: string, reason: string): void {
        let id: Uint8Array | undefined;
        const idv = frame.get("id");
        const b = wire.asBytes(idv);
        if (b) id = b;
        let sigstat = "none";
        if (frame.has("sig")) sigstat = "unverified";
        const recipients: unknown[] = [];
        const to = frame.get("to");
        if (Array.isArray(to)) {
            for (const it of to) {
                if (it instanceof Map) recipients.push(it);
            }
        }
        this.g.opaque.push({
            id: id ?? new Uint8Array(),
            frameType: ftype,
            reason,
            sigStat: sigstat,
            pubMeta: frame.get("pub"),
            recipients,
        });
    }
}

function diagCodeFor(reason: string): string {
    if (reason === "missing-key") return "MissingKey";
    return "UnknownCodec";
}

function termKindFromWire(k: number): TermKind {
    switch (k) {
        case 1:
            return TermKind.Literal;
        case 2:
            return TermKind.Bnode;
        case 3:
            return TermKind.Triple;
        default:
            return TermKind.Iri;
    }
}

function tripleEqual(a: Triple, b: Triple): boolean {
    return a.s === b.s && a.p === b.p && a.o === b.o;
}

function bytesEqual(a: Uint8Array, b: Uint8Array): boolean {
    if (a.length !== b.length) return false;
    for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
    return true;
}

function isHeaderItem(item: unknown): boolean {
    let inner = item;
    if (item instanceof Tagged) {
        inner = item.value;
    }
    if (!(inner instanceof Map)) return false;
    const hasGts = wire.mapGet(inner, "gts") !== undefined;
    const hasT = wire.mapGet(inner, "t") !== undefined;
    return hasGts && !hasT;
}

function catalogFrom(header: Map<unknown, unknown>): Map<number, Codec> {
    const out = new Map<number, Codec>();
    const cat = wire.mapGet(header, "cat");
    if (!(cat instanceof Map)) return out;
    for (const [cid, entry] of cat) {
        const n = wire.asInt64(cid);
        if (n === undefined) continue;
        if (!(entry instanceof Map)) continue;
        out.set(n, {
            name: wire.textOr(entry.get("name"), ""),
            cls: wire.textOr(entry.get("cls"), "encode"),
        });
    }
    return out;
}

function emptyGraph(): Graph {
    return new Graph();
}

function readSegment(items: wire.CborItem[], indexOffset: number): Graph {
    const g = emptyGraph();
    const rawHeader = items[0].item;
    let header: Map<unknown, unknown>;
    try {
        header = wire.unwrapHeader(rawHeader);
    } catch (e) {
        g.diagnostics.push({
            code: "DamagedFrame",
            detail: `invalid header: ${(e as Error).message}`,
            frameIndex: indexOffset,
        });
        return g;
    }
    const storedHid = wire.asBytes(wire.mapGet(header, "id"));
    if (!storedHid || !bytesEqual(storedHid, wire.headerId(header))) {
        g.diagnostics.push({
            code: "DamagedFrame",
            detail: "header self-hash mismatch",
            frameIndex: indexOffset,
        });
    }
    let expectedPrev = storedHid ?? new Uint8Array();

    const catalog = catalogFrom(header);
    const fld = new Folder(g, catalog);
    for (let idx = 1; idx < items.length; idx++) {
        const it = items[idx];
        const absIndex = idx + indexOffset;
        const frame = it.item instanceof Map ? it.item : undefined;
        if (!frame) {
            fld.diag("DamagedFrame", "frame is not a map", absIndex);
            continue;
        }
        const storedId = wire.asBytes(frame.get("id"));
        const computed = wire.contentId(frame);
        if (!storedId || !bytesEqual(storedId, computed)) {
            fld.diag("DamagedFrame", "frame self-hash mismatch", absIndex);
            const ftype = wire.textOr(frame.get("t"), "");
            fld.opaque(frame, ftype, "damaged");
            expectedPrev = storedId ?? computed;
            continue;
        }
        let prevOk = false;
        const prev = wire.asBytes(frame.get("prev"));
        if (prev) prevOk = bytesEqual(prev, expectedPrev);
        if (!prevOk) fld.diag("BrokenChain", "prev does not match", absIndex);
        expectedPrev = computed;
        const sig = frame.get("sig");
        if (sig !== undefined) {
            let status = "unverified";
            if (!(sig instanceof Uint8Array)) status = "invalid";
            g.signatures.push({ frameId: computed, kid: "", status });
        }
        fld.foldFrame(frame, absIndex);
    }
    g.segmentHeads.push(expectedPrev);
    g.segmentMeta.push([...g.meta]);
    g.segmentProfiles.push(wire.textOr(header.get("prof"), "generic"));
    return g;
}

/** Fold a GTS file into a Graph. */
export function Read(
    data: Uint8Array,
    allowSegments: boolean,
    expectedHead?: Uint8Array,
): Graph {
    const { items, torn } = wire.iterItems(data);
    if (items.length === 0) {
        const g = emptyGraph();
        g.diagnostics.push({
            code: "EmptyFile",
            detail: "no CBOR items",
            frameIndex: 0,
        });
        return g;
    }
    const bounds: number[] = [];
    for (let i = 0; i < items.length; i++) {
        if (isHeaderItem(items[i].item)) bounds.push(i);
    }
    const g = emptyGraph();
    if (bounds.length === 0 || bounds[0] !== 0) {
        g.diagnostics.push({
            code: "DamagedFrame",
            detail: "first item is not a header",
            frameIndex: 0,
        });
        return g;
    }
    if (bounds.length > 1 && !allowSegments) {
        const seg = readSegment(items.slice(0, bounds[1]), 0);
        seg.diagnostics.push({
            code: "SegmentBoundary",
            detail: `segment boundary at item ${bounds[1]} but reader is in pre-segment mode; remainder of file NOT folded`,
            frameIndex: bounds[1],
        });
        return seg;
    }
    const folded: Graph[] = [];
    for (let i = 0; i < bounds.length; i++) {
        const a = bounds[i];
        const b = i + 1 < bounds.length ? bounds[i + 1] : items.length;
        folded.push(readSegment(items.slice(a, b), a));
    }
    const out = folded.length === 1 ? folded[0] : unionSegments(folded);
    if (expectedHead) {
        const lastHead =
            out.segmentHeads.length > 0
                ? out.segmentHeads[out.segmentHeads.length - 1]
                : new Uint8Array();
        if (!bytesEqual(lastHead, expectedHead)) {
            out.diagnostics.push({
                code: "TruncatedLog",
                detail: "observed head does not match expected head",
            });
        }
    }
    if (torn >= 0) {
        out.diagnostics.push({
            code: "TornAppendError",
            detail: `torn at offset ${torn}`,
        });
    }
    return out;
}

/** Per-segment view of a file. */
export interface FileSegments {
    segments: Graph[];
    torn: number;
    fatal?: Diagnostic;
}

/** Fold a file segment-by-segment without unioning. */
export function ReadFileSegments(data: Uint8Array): FileSegments {
    const { items, torn } = wire.iterItems(data);
    if (items.length === 0) {
        return {
            segments: [],
            torn,
            fatal: {
                code: "EmptyFile",
                detail: "no CBOR items",
                frameIndex: 0,
            },
        };
    }
    const bounds: number[] = [];
    for (let i = 0; i < items.length; i++) {
        if (isHeaderItem(items[i].item)) bounds.push(i);
    }
    if (bounds.length === 0 || bounds[0] !== 0) {
        return {
            segments: [],
            torn,
            fatal: {
                code: "DamagedFrame",
                detail: "first item is not a header",
                frameIndex: 0,
            },
        };
    }
    const segments: Graph[] = [];
    for (let i = 0; i < bounds.length; i++) {
        const a = bounds[i];
        const b = i + 1 < bounds.length ? bounds[i + 1] : items.length;
        segments.push(readSegment(items.slice(a, b), a));
    }
    return { segments, torn };
}

interface InternKey {
    typ: number; // 0=iri, 1=lit, 2=bnode, 3=qt
    a: string;
    b: string;
    c: string;
    seg?: number;
    rf?: number;
    bnodeTid?: number;
    bnodeLabeled?: boolean;
}

class Unioner {
    out = emptyGraph();
    intern = new Map<string, number>();

    keyString(k: InternKey): string {
        return JSON.stringify(k);
    }

    keyFor(seg: Graph, segIdx: number, tid: number): InternKey {
        const t = seg.terms[tid];
        switch (t.kind) {
            case TermKind.Iri:
                return { typ: 0, a: t.value, b: "", c: "" };
            case TermKind.Literal:
                return {
                    typ: 1,
                    a: t.value,
                    b: seg.datatypeIri(t),
                    c: t.lang ?? "",
                };
            case TermKind.Bnode:
                if (t.value !== "") {
                    return {
                        typ: 2,
                        a: t.value,
                        b: "",
                        c: "",
                        seg: segIdx,
                        bnodeLabeled: true,
                    };
                }
                return {
                    typ: 2,
                    a: "",
                    b: "",
                    c: "",
                    seg: segIdx,
                    bnodeTid: tid,
                };
            case TermKind.Triple: {
                let rf: number | undefined;
                if (t.reifier !== undefined) {
                    rf = this.mapTerm(seg, segIdx, t.reifier);
                }
                return { typ: 3, a: "", b: "", c: "", rf };
            }
        }
    }

    mapTerm(seg: Graph, segIdx: number, tid: number): number {
        const key = this.keyFor(seg, segIdx, tid);
        const ks = this.keyString(key);
        if (this.intern.has(ks)) return this.intern.get(ks)!;
        const t = seg.terms[tid];
        let datatype: number | undefined;
        if (t.datatype !== undefined) {
            datatype = this.mapTerm(seg, segIdx, t.datatype);
        }
        let reifier: number | undefined;
        if (t.reifier !== undefined) {
            reifier = this.mapTerm(seg, segIdx, t.reifier);
        }
        let value = t.value;
        if (t.kind === TermKind.Bnode) {
            if (value !== "") {
                value = `s${segIdx}.${value}`;
            } else {
                value = `s${segIdx}._anon${this.out.terms.length}`;
            }
        }
        this.out.terms.push({
            kind: t.kind,
            value,
            datatype,
            lang: t.lang,
            reifier,
        });
        const newId = this.out.terms.length - 1;
        this.intern.set(ks, newId);
        return newId;
    }

    remapSuppression(
        seg: Graph,
        segIdx: number,
        sup: Suppression,
    ): Suppression {
        const n = seg.terms.length;
        const newTargets: unknown[] = [];
        for (const target of sup.targets) {
            if (!(target instanceof Map)) {
                newTargets.push(target);
                continue;
            }
            const kind = wire.textOr(target.get("kind"), "");
            if (kind === "frame" || kind === "blob") {
                newTargets.push(target);
                continue;
            }
            const newMap = new Map<unknown, unknown>();
            for (const [k, v] of target) {
                newMap.set(k, v);
                const key = wire.asText(k) ?? "";
                if ((kind === "term" || kind === "reifier") && key === "id") {
                    const tid = wire.asInt(v);
                    if (tid !== undefined && tid < n) {
                        newMap.set(k, this.mapTerm(seg, segIdx, tid));
                    }
                } else if (kind === "quad" && key === "q") {
                    const ids = Array.isArray(v) ? v : undefined;
                    if (ids) {
                        newMap.set(
                            k,
                            ids.map((x) => {
                                const tid = wire.asInt(x);
                                if (tid !== undefined && tid < n)
                                    return this.mapTerm(seg, segIdx, tid);
                                return x;
                            }),
                        );
                    }
                }
            }
            newTargets.push(newMap);
        }
        const out: Suppression = { targets: newTargets, reason: sup.reason };
        if (sup.by !== undefined && sup.by < n) {
            out.by = this.mapTerm(seg, segIdx, sup.by);
        }
        return out;
    }
}

function unionQuadKey(q: Quad): string {
    return q.g === undefined
        ? `${q.s},${q.p},${q.o}`
        : `${q.s},${q.p},${q.o},${q.g}`;
}

function unionSegments(segments: Graph[]): Graph {
    const u = new Unioner();
    const seen = new Set<string>();
    for (let segIdx = 0; segIdx < segments.length; segIdx++) {
        const seg = segments[segIdx];
        for (const q of seg.quads) {
            const uq: Quad = {
                s: u.mapTerm(seg, segIdx, q.s),
                p: u.mapTerm(seg, segIdx, q.p),
                o: u.mapTerm(seg, segIdx, q.o),
            };
            if (q.g !== undefined) uq.g = u.mapTerm(seg, segIdx, q.g);
            const key = unionQuadKey(uq);
            if (!seen.has(key)) {
                seen.add(key);
                u.out.quads.push(uq);
            }
        }
        for (const r of seg.reifiers) {
            const newRf = u.mapTerm(seg, segIdx, r.rid);
            const spo: Triple = {
                s: u.mapTerm(seg, segIdx, r.spo.s),
                p: u.mapTerm(seg, segIdx, r.spo.p),
                o: u.mapTerm(seg, segIdx, r.spo.o),
            };
            u.out.setReifier(newRf, spo);
        }
        for (const a of seg.annotations) {
            u.out.annotations.push({
                s: u.mapTerm(seg, segIdx, a.s),
                p: u.mapTerm(seg, segIdx, a.p),
                o: u.mapTerm(seg, segIdx, a.o),
            });
        }
        for (const b of seg.blobs) u.out.setBlob(b.digest, b.data);
        for (const bm of seg.blobMeta) u.out.setBlobMeta(bm.digest, bm.meta);
        for (const m of seg.meta) u.out.setMeta(m.key, m.value);
        u.out.segmentMeta.push(...seg.segmentMeta);
        for (const sup of seg.suppressions) {
            u.out.suppressions.push(u.remapSuppression(seg, segIdx, sup));
        }
        u.out.opaque.push(...seg.opaque);
        u.out.signatures.push(...seg.signatures);
        u.out.diagnostics.push(...seg.diagnostics);
        u.out.segmentHeads.push(...seg.segmentHeads);
        u.out.segmentProfiles.push(...seg.segmentProfiles);
    }
    return u.out;
}

// Re-export model types used by consumers.
export type { Graph, BlobEntry } from "./model.js";
