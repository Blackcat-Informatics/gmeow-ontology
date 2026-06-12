// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

import cbor, { Tagged } from "cbor";
import { hash as blake3Hash } from "blake3";

/** CBOR self-describe tag (RFC 8949 §3.4.6); MAY prefix the Header item (§3). */
export const SelfDescribeTag = 55799;

export const Magic = "GTS1";
export const Version = 1;

/** Encode a value to canonical CBOR bytes (RFC 8949 §4.2). */
export function encode(value: unknown): Uint8Array {
    const buf = cbor.encodeCanonical(value) as Buffer;
    return new Uint8Array(buf.buffer, buf.byteOffset, buf.byteLength);
}

/** Encode a value, throwing on error. */
export function mustEncode(value: unknown): Uint8Array {
    return encode(value);
}

/** Decode a single CBOR item, preferring Maps over plain objects. */
export function decodeFirst(data: Uint8Array): unknown {
    return cbor.decodeFirstSync(Buffer.from(data), { preferMap: true });
}

/** Return the 32-byte BLAKE3-256 digest of data. */
export function blake3_256(data: Uint8Array): Uint8Array {
    const buf = blake3Hash(data) as Buffer;
    return new Uint8Array(buf.buffer, buf.byteOffset, buf.byteLength);
}

/** Lowercase hex encoding. */
export function hex(data: Uint8Array): string {
    return Buffer.from(data).toString("hex");
}

/** Content digest string for inline blob addressing (§12). */
export function digestStr(data: Uint8Array): string {
    return "blake3:" + hex(blake3_256(data));
}

/** Look up a text key in a decoded CBOR map (first match). */
export function mapGet(
    m: Map<unknown, unknown> | undefined,
    key: string,
): unknown | undefined {
    if (!m) return undefined;
    if (m.has(key)) return m.get(key);
    for (const [k, v] of m) {
        if (k === key) return v;
    }
    return undefined;
}

/** Coerce a decoded CBOR value to a string. */
export function asText(value: unknown): string | undefined {
    if (typeof value === "string") return value;
    return undefined;
}

/** Coerce a decoded CBOR value to a byte string. */
export function asBytes(value: unknown): Uint8Array | undefined {
    if (value instanceof Uint8Array) return value;
    if (Buffer.isBuffer(value)) {
        return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
    }
    if (value instanceof ArrayBuffer) return new Uint8Array(value);
    return undefined;
}

/** Coerce a decoded CBOR value to a non-negative integer. */
export function asInt(value: unknown): number | undefined {
    if (typeof value === "number") {
        if (Number.isInteger(value) && value >= 0) return value;
    }
    if (typeof value === "bigint") {
        if (value >= 0n && value <= Number.MAX_SAFE_INTEGER)
            return Number(value);
    }
    return undefined;
}

/** Coerce a decoded CBOR value to a signed integer. */
export function asInt64(value: unknown): number | undefined {
    if (typeof value === "number") {
        if (Number.isInteger(value)) return value;
    }
    if (typeof value === "bigint") {
        if (
            value >= Number.MIN_SAFE_INTEGER &&
            value <= Number.MAX_SAFE_INTEGER
        ) {
            return Number(value);
        }
    }
    return undefined;
}

/** Return a text value or a default. */
export function textOr(value: unknown, def: string): string {
    return asText(value) ?? def;
}

function cloneMap(m: Map<unknown, unknown>): Map<unknown, unknown> {
    const out = new Map<unknown, unknown>();
    for (const [k, v] of m) out.set(k, v);
    return out;
}

function hashExcluding(
    m: Map<unknown, unknown>,
    excluded: string[],
): Uint8Array {
    const content = cloneMap(m);
    for (const k of excluded) {
        if (content.has(k)) content.delete(k);
    }
    return blake3_256(encode(content));
}

/** Compute a frame's "id" over its content, excluding "id" and "sig". */
export function contentId(frame: Map<unknown, unknown>): Uint8Array {
    return hashExcluding(frame, ["id", "sig"]);
}

/** Compute the Header's genesis "id", excluding only "id". */
export function headerId(header: Map<unknown, unknown>): Uint8Array {
    return hashExcluding(header, ["id"]);
}

/** Parse a CBOR additional-info length descriptor starting at data[offset]. */
function readLength(
    data: Uint8Array,
    offset: number,
    info: number,
): { length: number; extra: number } {
    switch (info) {
        case 24:
            if (offset >= data.length) throw new Error("unexpected EOF");
            return { length: data[offset], extra: 1 };
        case 25: {
            if (offset + 2 > data.length) throw new Error("unexpected EOF");
            const n = (data[offset] << 8) | data[offset + 1];
            return { length: n, extra: 2 };
        }
        case 26: {
            if (offset + 4 > data.length) throw new Error("unexpected EOF");
            const n =
                (data[offset] << 24) |
                (data[offset + 1] << 16) |
                (data[offset + 2] << 8) |
                data[offset + 3];
            // Treat as unsigned; for lengths we only expect positive values.
            return { length: n >>> 0, extra: 4 };
        }
        case 27: {
            if (offset + 8 > data.length) throw new Error("unexpected EOF");
            let n = 0n;
            for (let i = 0; i < 8; i++) {
                n = (n << 8n) | BigInt(data[offset + i]);
            }
            if (n > BigInt(Number.MAX_SAFE_INTEGER)) {
                throw new Error("length exceeds safe integer range");
            }
            return { length: Number(n), extra: 8 };
        }
        default:
            throw new Error(`unsupported additional info for length: ${info}`);
    }
}

/** Return the byte length of the next well-formed CBOR item at data[offset]. */
export function cborItemLength(data: Uint8Array, offset: number): number {
    if (offset >= data.length) throw new Error("EOF");
    const start = offset;
    const stack: { major: number; remaining: number }[] = [];

    const complete = () => {
        while (stack.length > 0) {
            const top = stack[stack.length - 1];
            if (top.remaining > 0) top.remaining--;
            if (top.remaining === 0) {
                stack.pop();
            } else {
                break;
            }
        }
    };

    for (;;) {
        if (offset >= data.length) throw new Error("unexpected EOF");
        const b = data[offset];
        const major = b >> 5;
        const info = b & 0x1f;
        offset++;

        let extra = 0;
        let length = -1;

        if (info <= 23) {
            length = info;
        } else if (info === 24 || info === 25 || info === 26 || info === 27) {
            const res = readLength(data, offset, info);
            length = res.length;
            extra = res.extra;
        } else if (info >= 28 && info <= 30) {
            throw new Error(`reserved additional info ${info}`);
        } else if (info === 31) {
            // Indefinite length.
            switch (major) {
                case 2:
                case 3: {
                    for (;;) {
                        if (offset >= data.length)
                            throw new Error("unexpected EOF");
                        const nb = data[offset];
                        if (nb === 0xff) {
                            offset++;
                            break;
                        }
                        const nmajor = nb >> 5;
                        const ninfo = nb & 0x1f;
                        if (nmajor !== major || ninfo === 31) {
                            throw new Error("invalid indefinite string chunk");
                        }
                        let nlen: number;
                        let nextra = 0;
                        if (ninfo <= 23) {
                            nlen = ninfo;
                        } else {
                            const res = readLength(data, offset + 1, ninfo);
                            nlen = res.length;
                            nextra = res.extra;
                        }
                        offset += 1 + nextra;
                        if (data.length - offset < nlen)
                            throw new Error("unexpected EOF");
                        offset += nlen;
                    }
                    complete();
                    if (stack.length === 0) return offset - start;
                    continue;
                }
                case 4:
                case 5:
                    throw new Error(
                        `indefinite-length ${major === 5 ? "map" : "array"} not supported`,
                    );
                default:
                    throw new Error(
                        `indefinite length for major type ${major}`,
                    );
            }
        }

        if (length < 0 && extra > 0) {
            throw new Error("unreachable");
        }

        offset += extra;

        switch (major) {
            case 0:
            case 1:
            case 7:
                complete();
                break;
            case 2:
            case 3:
                if (data.length - offset < length)
                    throw new Error("unexpected EOF");
                offset += length;
                complete();
                break;
            case 4:
                if (length === 0) {
                    complete();
                } else {
                    stack.push({ major, remaining: length });
                }
                break;
            case 5:
                if (length === 0) {
                    complete();
                } else {
                    stack.push({ major, remaining: length * 2 });
                }
                break;
            case 6:
                stack.push({ major, remaining: 1 });
                break;
        }

        if (stack.length === 0) return offset - start;
    }
}

/** One decoded CBOR item and its byte offset. */
export interface CborItem {
    offset: number;
    item: unknown;
}

/**
 * Decode a CBOR Sequence into (offset, item) pairs plus a torn marker.
 * Returns the intact prefix and the torn offset, or -1 for a clean end.
 */
export function iterItems(data: Uint8Array): {
    items: CborItem[];
    torn: number;
} {
    const items: CborItem[] = [];
    let torn = -1;
    let offset = 0;
    while (offset < data.length) {
        const start = offset;
        let length: number;
        try {
            length = cborItemLength(data, offset);
        } catch {
            torn = start;
            break;
        }
        const end = offset + length;
        let item: unknown;
        try {
            item = cbor.decodeFirstSync(
                Buffer.from(data.subarray(offset, end)),
                { preferMap: true },
            );
        } catch {
            torn = start;
            break;
        }
        items.push({ offset: start, item });
        offset = end;
    }
    return { items, torn };
}

/** Unwrap an optional CBOR self-describe tag from the Header item. */
export function unwrapHeader(item: unknown): Map<unknown, unknown> {
    let inner = item;
    if (item instanceof Tagged) {
        if (item.tag !== SelfDescribeTag) {
            throw new Error(`unexpected CBOR tag ${item.tag} on header item`);
        }
        inner = item.value;
    }
    if (inner instanceof Map) return inner;
    throw new Error("header item is not a CBOR map");
}
