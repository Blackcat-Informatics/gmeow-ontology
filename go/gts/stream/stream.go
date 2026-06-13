// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

// Package stream provides the spec-owned stream vocabulary (GTS-SPEC §13.3).
//
// Constants only — the streaming-index terms a streamable segment leads with
// (§3.3) and the compaction-provenance terms a streamable rewrite records
// (§10.1). Like the files profile vocabulary (§13.2), the terms are authored
// in the spec and carried as literal IRIs; no external ontology is required.
package stream

// NS is the stream vocabulary namespace (§13.3).
const NS = "https://w3id.org/gts/stream#"

// Streaming-index terms (§3.3): one Manifestation per promised blob.
const (
	Manifestation = NS + "Manifestation"
	Digest        = NS + "digest"
	MediaType     = NS + "mediaType"
	Size          = NS + "size"
	Role          = NS + "role"
	Order         = NS + "order"
)

// Compaction-provenance terms (§10.1).
const (
	Compaction        = NS + "Compaction"
	Agent             = NS + "agent"
	Timestamp         = NS + "timestamp"
	SourceHead        = NS + "sourceHead"
	SealedSource      = NS + "sealedSource"
	DetachedSignature = NS + "DetachedSignature"
	SourceFrame       = NS + "sourceFrame"
	Cose              = NS + "cose"
)

// CompactAgent is the fixed compactor identity recorded as stream:agent — a
// constant so the rewrite is byte-reproducible across engines (§14.1
// determinism).
const CompactAgent = "gts-compact"
