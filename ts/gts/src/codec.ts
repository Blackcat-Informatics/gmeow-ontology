// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

import { gunzipSync, gzipSync } from "node:zlib";
import { decompress as zstdDecompress } from "fzstd";

/** A catalog entry (§5, §8.5). */
export interface Codec {
    name: string;
    /** "encode" | "compress" | "encrypt" */
    cls: string;
}

export interface CodecError {
    reason: string;
    detail: string;
    failed: boolean;
}

export function codecError(err: CodecError): Error {
    const e = new Error(err.detail) as Error & CodecError;
    e.reason = err.reason;
    e.detail = err.detail;
    e.failed = err.failed;
    return e;
}

export function isCodecError(err: unknown): err is Error & CodecError {
    if (!(err instanceof Error)) return false;
    const e = err as Error & Record<string, unknown>;
    return (
        typeof e.reason === "string" &&
        typeof e.failed === "boolean" &&
        typeof e.detail === "string"
    );
}

function toUint8Array(buf: Buffer): Uint8Array {
    return new Uint8Array(buf.buffer, buf.byteOffset, buf.byteLength);
}

function decodeOne(codec: Codec, data: Uint8Array): Uint8Array {
    if (codec.cls === "encrypt") {
        throw codecError({
            reason: "missing-key",
            detail: `no key for encrypt codec '${codec.name}'`,
            failed: false,
        });
    }
    switch (codec.name) {
        case "identity":
            return data;
        case "gzip": {
            try {
                return toUint8Array(gunzipSync(data));
            } catch (e) {
                throw codecError({
                    reason: "damaged",
                    detail: `gzip decode failed: ${(e as Error).message}`,
                    failed: true,
                });
            }
        }
        case "zstd":
        case "zstd-rsyncable": {
            try {
                return zstdDecompress(data);
            } catch (e) {
                throw codecError({
                    reason: "damaged",
                    detail: `zstd decode failed: ${(e as Error).message}`,
                    failed: true,
                });
            }
        }
        default:
            throw codecError({
                reason: "unknown-codec",
                detail: `unknown codec '${codec.name}'`,
                failed: false,
            });
    }
}

/**
 * Reverse a resolved codec chain, last to first (§6.1, §8.2).
 * The baseline carries no keys, so every encrypt-class codec degrades to
 * missing-key (matching the Python reader with keys=None).
 */
export function decodeChain(chain: Codec[], data: Uint8Array): Uint8Array {
    let current = data;
    for (let i = chain.length - 1; i >= 0; i--) {
        current = decodeOne(chain[i], current);
    }
    return current;
}

/** Baseline codec helpers exposed for tests and future writers. */
export const identity = {
    name: "identity" as const,
    cls: "encode" as const,
    encode: (data: Uint8Array): Uint8Array => data,
    decode: (data: Uint8Array): Uint8Array => data,
};

export const gzip = {
    name: "gzip" as const,
    cls: "compress" as const,
    encode: (data: Uint8Array): Uint8Array => toUint8Array(gzipSync(data)),
    decode: (data: Uint8Array): Uint8Array => toUint8Array(gunzipSync(data)),
};

export const zstd = {
    name: "zstd" as const,
    cls: "compress" as const,
    encode: (_data: Uint8Array): Uint8Array => {
        throw codecError({
            reason: "unsupported",
            detail: "zstd encode is not implemented in the baseline TypeScript engine",
            failed: false,
        });
    },
    decode: (data: Uint8Array): Uint8Array => zstdDecompress(data),
};
