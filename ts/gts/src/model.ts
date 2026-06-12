// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

/** Well-known datatype IRIs used by the literal-defaulting rule (§7.1). */
export const XSD_STRING = "http://www.w3.org/2001/XMLSchema#string";
export const RDF_LANG_STRING =
    "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";

/** The kind of an RDF term, matching the wire "k" field (§7.1). */
export enum TermKind {
    Iri = 0,
    Literal = 1,
    Bnode = 2,
    Triple = 3,
}

/** Parse the wire "k" value; an unknown kind defaults to IRI (§7.1). */
export function termKindFromWire(k: number): TermKind {
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

/** A single RDF term carried by append-order id. */
export interface Term {
    kind: TermKind;
    /** IRI string, literal lexical form, or blank-node label (file-local). */
    value: string;
    /** Term-id of the literal's datatype IRI, when explicit. */
    datatype?: number;
    /** Literal language tag (BCP 47). */
    lang?: string;
    /** Term-id of the reifier of a quoted triple (kind == Triple). */
    reifier?: number;
}

/** A tuple of term-ids; the graph slot is undefined for the default graph. */
export interface Quad {
    s: number;
    p: number;
    o: number;
    g?: number;
}

/** A triple of term-ids. */
export interface Triple {
    s: number;
    p: number;
    o: number;
}

/** A frame the reader could not decode (§7.6). */
export interface OpaqueNode {
    id: Uint8Array;
    frameType: string;
    /** "unknown-codec" | "missing-key" | "damaged" | "unknown-frame-type" */
    reason: string;
    /** "none" | "valid" | "invalid" | "unverified" */
    sigStat: string;
    pubMeta: unknown;
    recipients: unknown[];
}

/** A recorded suppress directive (§11). */
export interface Suppression {
    targets: unknown[];
    reason: string;
    by?: number;
}

/** A machine-observable reader diagnostic (§2.3). */
export interface Diagnostic {
    code: string;
    detail: string;
    frameIndex?: number;
}

/** The verification outcome for a signed frame (§9.2). */
export interface Signature {
    frameId: Uint8Array;
    kid: string;
    /** "none" | "valid" | "invalid" | "unverified" */
    status: string;
}

/** A single key/value metadata pair. */
export interface MetaEntry {
    key: string;
    value: unknown;
}

/** A single inline blob. */
export interface BlobEntry {
    digest: string;
    data: Uint8Array;
}

/** Declared blob metadata by digest. */
export interface BlobMetaEntry {
    digest: string;
    meta: unknown;
}

/** Reifier-id → triple binding. */
export interface ReifierEntry {
    rid: number;
    spo: Triple;
}

/** The folded result of a GTS log. */
export class Graph {
    terms: Term[] = [];
    quads: Quad[] = [];
    reifiers: ReifierEntry[] = [];
    annotations: Triple[] = [];
    blobs: BlobEntry[] = [];
    blobMeta: BlobMetaEntry[] = [];
    meta: MetaEntry[] = [];
    suppressions: Suppression[] = [];
    opaque: OpaqueNode[] = [];
    signatures: Signature[] = [];
    diagnostics: Diagnostic[] = [];
    segmentHeads: Uint8Array[] = [];
    segmentProfiles: string[] = [];
    segmentMeta: MetaEntry[][] = [];

    /** Look up a reifier binding. */
    reifier(rid: number): Triple | undefined {
        for (const r of this.reifiers) {
            if (r.rid === rid) return r.spo;
        }
        return undefined;
    }

    /** Bind a reifier, replacing in place (Python dict assignment). */
    setReifier(rid: number, spo: Triple): void {
        for (const r of this.reifiers) {
            if (r.rid === rid) {
                r.spo = spo;
                return;
            }
        }
        this.reifiers.push({ rid, spo });
    }

    /** Set a meta key, replacing in place. */
    setMeta(key: string, value: unknown): void {
        for (const m of this.meta) {
            if (m.key === key) {
                m.value = value;
                return;
            }
        }
        this.meta.push({ key, value });
    }

    /** Record a blob's declared metadata, replacing in place. */
    setBlobMeta(digest: string, meta: unknown): void {
        for (const bm of this.blobMeta) {
            if (bm.digest === digest) {
                bm.meta = meta;
                return;
            }
        }
        this.blobMeta.push({ digest, meta });
    }

    /** Store an inline blob under its digest, replacing in place. */
    setBlob(digest: string, data: Uint8Array): void {
        for (const b of this.blobs) {
            if (b.digest === digest) {
                b.data = data;
                return;
            }
        }
        this.blobs.push({ digest, data });
    }

    /** The effective datatype IRI of a literal, applying §7.1 defaulting. */
    datatypeIri(t: Term): string {
        if (t.datatype !== undefined) {
            const dt = this.terms[t.datatype];
            if (dt && dt.value) return dt.value;
            return XSD_STRING;
        }
        if (t.lang) return RDF_LANG_STRING;
        return XSD_STRING;
    }
}
