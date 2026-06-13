// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

/**
 * The spec-owned `stream` vocabulary (GTS-SPEC §13.3).
 *
 * Constants only — the streaming-index terms a streamable segment leads with
 * (§3.3) and the compaction-provenance terms a streamable rewrite records
 * (§10.1). Like the `files` profile vocabulary (§13.2), the terms are authored
 * in the spec and carried as literal IRIs; no external ontology is required.
 */

export const STREAM_NS = "https://w3id.org/gts/stream#";

// Streaming-index terms (§3.3): one Manifestation per promised blob.
export const MANIFESTATION = STREAM_NS + "Manifestation";
export const DIGEST = STREAM_NS + "digest";
export const MEDIA_TYPE = STREAM_NS + "mediaType";
export const SIZE = STREAM_NS + "size";
export const ROLE = STREAM_NS + "role";
export const ORDER = STREAM_NS + "order";

// Compaction-provenance terms (§10.1).
export const COMPACTION = STREAM_NS + "Compaction";
export const AGENT = STREAM_NS + "agent";
export const TIMESTAMP = STREAM_NS + "timestamp";
export const SOURCE_HEAD = STREAM_NS + "sourceHead";
export const SEALED_SOURCE = STREAM_NS + "sealedSource";
export const DETACHED_SIGNATURE = STREAM_NS + "DetachedSignature";
export const SOURCE_FRAME = STREAM_NS + "sourceFrame";
export const COSE = STREAM_NS + "cose";

/**
 * The fixed compactor identity recorded as `stream:agent` — a constant so
 * the rewrite is byte-reproducible across engines (§14.1 determinism).
 */
export const COMPACT_AGENT = "gts-compact";
