// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

/**
 * Streamable compaction (GTS-SPEC §10.1): re-author the ordering, only the
 * ordering.
 *
 * `compactStreamable` rewrites an accretive GTS file (or multi-segment
 * composition) into ONE delivery-ordered segment in the streamable layout
 * state (§3.3): a leading streaming index in the `stream` vocabulary (§13.3),
 * the content graph, blobs most-significant-first, and a trailing offset
 * `index` footer. Content signatures ride through untouched; frame
 * signatures are carried *detached* in compaction provenance; the ordering
 * commitment is re-issued — the compactor is the sole attester of the new
 * ordering.
 *
 * The rewrite is byte-deterministic for the same input and parameters
 * (§14.1): blob order is ascending decoded size with digest tie-break, the
 * agent string is a constant, and the timestamp is a parameter — never
 * ambient time.
 */

import {
    Graph,
    Quad,
    Term,
    TermKind,
    type ReifierEntry,
    type Suppression,
    type Triple,
} from "./model.js";
import { Read, ReadFileSegments } from "./reader.js";
import { Writer } from "./writer.js";
import * as wire from "./wire.js";
import * as stream from "./stream.js";

const RDF_TYPE = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const XSD_INTEGER = "http://www.w3.org/2001/XMLSchema#integer";
const XSD_DATETIME = "http://www.w3.org/2001/XMLSchema#dateTime";

/** The input is not safely compactable (§10.1/§14.1 refuse-don't-trust). */
export class CompactRefusedError extends Error {
    constructor(message: string) {
        super(message);
        this.name = "CompactRefusedError";
    }
}

function targetKind(target: unknown): string {
    if (!(target instanceof Map)) return "";
    return wire.textOr(wire.mapGet(target, "kind"), "");
}

function blobData(g: Graph, digest: string): Uint8Array {
    for (const b of g.blobs) {
        if (b.digest === digest) return b.data;
    }
    return new Uint8Array();
}

function blobMetaText(
    g: Graph,
    digest: string,
    key: string,
): string | undefined {
    for (const bm of g.blobMeta) {
        if (bm.digest !== digest) continue;
        if (!(bm.meta instanceof Map)) continue;
        return wire.asText(wire.mapGet(bm.meta, key));
    }
    return undefined;
}

/** Verify the input cleanly and return its union fold + single profile. */
function refusalGate(
    data: Uint8Array,
    sealOriginal: boolean,
): { g: Graph; profile: string } {
    const fs = ReadFileSegments(data);
    if (fs.fatal) {
        throw new CompactRefusedError(
            `input is not a clean GTS file: ${fs.fatal.code}: ${fs.fatal.detail}`,
        );
    }
    if (fs.torn >= 0) {
        throw new CompactRefusedError(
            `input has a torn append at byte ${fs.torn}`,
        );
    }
    for (let idx = 0; idx < fs.segments.length; idx++) {
        const seg = fs.segments[idx];
        if (seg.diagnostics.length > 0) {
            const first = seg.diagnostics[0];
            throw new CompactRefusedError(
                `segment ${idx} does not verify cleanly: ${first.code}: ${first.detail}`,
            );
        }
    }
    const profiles = new Set<string>();
    for (const seg of fs.segments) {
        for (const p of seg.segmentProfiles) profiles.add(p);
    }
    if (profiles.size > 1) {
        const quoted = [...profiles]
            .sort()
            .map((p) => `'${p}'`)
            .join(", ");
        throw new CompactRefusedError(
            `mixed segment profiles [${quoted}] are not compactable (v1)`,
        );
    }
    const profile = profiles.size === 1 ? [...profiles][0] : "generic";
    if (profile === "evidence" && !sealOriginal) {
        throw new CompactRefusedError(
            "an 'evidence' artifact's signed chain IS the artifact; refusing " +
                "to re-order it without --seal-original (§10.1)",
        );
    }
    const g = Read(data, true);
    for (const sup of g.suppressions) {
        for (const target of sup.targets) {
            if (targetKind(target) === "frame") {
                throw new CompactRefusedError(
                    "input carries a frame-addressed suppression; the rewrite " +
                        "assigns new frame ids, so the target would silently " +
                        "dangle (§10.1)",
                );
            }
        }
    }
    return { g, profile };
}

/** Accumulates the streaming-index terms and quads with stable ids. */
class GraphBuilder {
    terms: Term[] = [];
    quads: Quad[] = [];

    add(term: Term): number {
        this.terms.push(term);
        return this.terms.length - 1;
    }

    literal(value: string, datatype?: number): number {
        return this.add({ kind: TermKind.Literal, value, datatype });
    }

    quad(s: number, p: number, o: number): void {
        this.quads.push({ s, p, o });
    }
}

/** Build the leading streaming index + compaction provenance (§3.3, §13.3). */
function streamingIndex(
    g: Graph,
    blobOrder: string[],
    timestamp: string,
    sealedDigest: string | undefined,
    sealedSize: number | undefined,
): GraphBuilder {
    const b = new GraphBuilder();
    const iri = (value: string): number => b.add({ kind: TermKind.Iri, value });
    const bnode = (value: string): number =>
        b.add({ kind: TermKind.Bnode, value });
    // Fixed vocabulary block — constant ids across engines for determinism.
    const tType = iri(RDF_TYPE);
    const tInt = iri(XSD_INTEGER);
    const tDt = iri(XSD_DATETIME);
    const tManifestation = iri(stream.MANIFESTATION);
    const tDigest = iri(stream.DIGEST);
    const tMt = iri(stream.MEDIA_TYPE);
    const tSize = iri(stream.SIZE);
    const tRole = iri(stream.ROLE);
    const tOrder = iri(stream.ORDER);
    const tCompaction = iri(stream.COMPACTION);
    const tAgent = iri(stream.AGENT);
    const tTimestamp = iri(stream.TIMESTAMP);
    const tSourceHead = iri(stream.SOURCE_HEAD);
    const tSealedSource = iri(stream.SEALED_SOURCE);
    const tDetachedSig = iri(stream.DETACHED_SIGNATURE);
    const tSourceFrame = iri(stream.SOURCE_FRAME);
    const tCose = iri(stream.COSE);

    // One Manifestation per promised blob, in delivery order.
    for (let order = 0; order < blobOrder.length; order++) {
        const digest = blobOrder[order];
        const m = bnode(`m${order}`);
        const sealed = digest === sealedDigest;
        const size = sealed ? sealedSize : blobData(g, digest).length;
        const mt = sealed ? "application/gts" : blobMetaText(g, digest, "mt");
        b.quad(m, tType, tManifestation);
        b.quad(m, tDigest, b.literal(digest));
        if (mt !== undefined) {
            b.quad(m, tMt, b.literal(mt));
        }
        if (size !== undefined) {
            b.quad(m, tSize, b.literal(String(size), tInt));
        }
        b.quad(m, tRole, b.literal(sealed ? "source" : "primary"));
        b.quad(m, tOrder, b.literal(String(order), tInt));
    }

    // The Compaction provenance node (§10.1).
    const c = bnode("c");
    b.quad(c, tType, tCompaction);
    b.quad(c, tAgent, b.literal(stream.COMPACT_AGENT));
    b.quad(c, tTimestamp, b.literal(timestamp, tDt));
    for (const head of g.segmentHeads) {
        b.quad(c, tSourceHead, b.literal("blake3:" + wire.hex(head)));
    }
    if (sealedDigest !== undefined) {
        b.quad(c, tSealedSource, b.literal(sealedDigest));
    }

    // Detached frame signatures (§10.1): checkable claims about the original log.
    let j = 0;
    for (const sig of g.signatures) {
        if (!sig.cose) continue;
        const node = bnode(`s${j}`);
        j++;
        const coseB64 = Buffer.from(sig.cose).toString("base64url");
        b.quad(node, tType, tDetachedSig);
        b.quad(
            node,
            tSourceFrame,
            b.literal("blake3:" + wire.hex(sig.frameId)),
        );
        b.quad(node, tCose, b.literal(coseB64));
    }
    return b;
}

/** Shift a term's id references into the output id space. */
function shiftTerm(t: Term, base: number): Term {
    if (t.datatype === undefined && t.reifier === undefined) return t;
    return {
        ...t,
        datatype: t.datatype !== undefined ? t.datatype + base : undefined,
        reifier: t.reifier !== undefined ? t.reifier + base : undefined,
    };
}

/** Carry suppressions forward, one output suppression per input (§10.1).
 *
 * Re-authoring of the ordering only: each original suppression keeps its own
 * frame with its `reason`/`by` metadata intact — blob targets verbatim
 * (content-addressing is layout-independent), id-addressed targets and `by`
 * shifted into the output id space.
 */
function shiftedSuppressions(g: Graph, base: number): Suppression[] {
    const out: Suppression[] = [];
    for (const sup of g.suppressions) {
        const targets: unknown[] = [];
        for (const target of sup.targets) {
            if (!(target instanceof Map)) {
                targets.push(target);
                continue;
            }
            const kind = targetKind(target);
            const t = new Map<unknown, unknown>();
            for (const [k, v] of target) {
                t.set(k, v);
                const key = wire.asText(k) ?? "";
                if ((kind === "term" || kind === "reifier") && key === "id") {
                    const tid = wire.asInt(v);
                    if (tid !== undefined) t.set(k, tid + base);
                } else if (kind === "quad" && key === "q") {
                    if (Array.isArray(v)) {
                        t.set(
                            k,
                            v.map((x) => {
                                const n = wire.asInt(x);
                                return n !== undefined ? n + base : x;
                            }),
                        );
                    }
                }
            }
            targets.push(t);
        }
        const shifted: Suppression = { targets, reason: sup.reason };
        if (sup.by !== undefined) shifted.by = sup.by + base;
        out.push(shifted);
    }
    return out;
}

/** Rewrite a GTS file into one streamable segment (§10.1).
 *
 * `data` must verify cleanly (refuse-don't-trust). `timestamp` is the
 * rewrite time recorded as `stream:timestamp` — an explicit parameter so the
 * output is byte-reproducible. `sealOriginal` carries the verbatim source
 * bytes as a nested GTS blob (§12.1), role `"source"` — REQUIRED for
 * `evidence` input.
 *
 * Throws `CompactRefusedError` on any §10.1/§14.1 refusal condition.
 */
export function compactStreamable(
    data: Uint8Array,
    opts: { timestamp: string; sealOriginal?: boolean },
): Uint8Array {
    const sealOriginal = opts.sealOriginal ?? false;
    const { g, profile } = refusalGate(data, sealOriginal);

    // Delivery plan: most-significant-first — ascending decoded size, digest
    // tie-break; the sealed original (least significant) always travels last.
    let blobOrder = g.blobs
        .map((b) => b.digest)
        .sort((a, b) => {
            const sa = blobData(g, a).length;
            const sb = blobData(g, b).length;
            if (sa !== sb) return sa - sb;
            return a < b ? -1 : a > b ? 1 : 0;
        });
    let sealedDigest: string | undefined;
    if (sealOriginal) {
        sealedDigest = wire.digestStr(data);
        blobOrder = blobOrder.filter((d) => d !== sealedDigest);
        blobOrder.push(sealedDigest);
    }

    const index = streamingIndex(
        g,
        blobOrder,
        opts.timestamp,
        sealedDigest,
        sealedDigest !== undefined ? data.length : undefined,
    );
    const base = index.terms.length;

    const w = new Writer(profile, "streamable");
    // Leading streaming index: the catalog presages everything below it.
    w.addTerms(index.terms);
    w.addQuads(index.quads);
    // Content graph, re-emitted from the folded union (ids shifted by `base`).
    if (g.terms.length > 0) {
        w.addTerms(g.terms.map((t) => shiftTerm(t, base)));
    }
    if (g.quads.length > 0) {
        w.addQuads(
            g.quads.map((q) => {
                const nq: Quad = {
                    s: q.s + base,
                    p: q.p + base,
                    o: q.o + base,
                };
                if (q.g !== undefined) nq.g = q.g + base;
                return nq;
            }),
        );
    }
    if (g.reifiers.length > 0) {
        w.addReifies(
            g.reifiers.map(
                (r): ReifierEntry => ({
                    rid: r.rid + base,
                    spo: {
                        s: r.spo.s + base,
                        p: r.spo.p + base,
                        o: r.spo.o + base,
                    },
                }),
            ),
        );
    }
    if (g.annotations.length > 0) {
        w.addAnnot(
            g.annotations.map(
                (a): Triple => ({
                    s: a.s + base,
                    p: a.p + base,
                    o: a.o + base,
                }),
            ),
        );
    }
    for (const sup of shiftedSuppressions(g, base)) {
        w.addSuppress(sup.targets, sup.reason || undefined, sup.by);
    }
    // Blobs in delivery order; declared metadata rides along.
    for (const digest of blobOrder) {
        if (digest === sealedDigest) {
            w.addBlob(data, "application/gts", "source");
            continue;
        }
        w.addBlob(
            blobData(g, digest),
            blobMetaText(g, digest, "mt"),
            blobMetaText(g, digest, "rep"),
        );
    }
    // The re-issued ordering commitment: the compactor is its sole attester.
    w.addIndex();
    return w.toBytes();
}
