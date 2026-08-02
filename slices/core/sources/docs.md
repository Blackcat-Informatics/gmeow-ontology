<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Sources — carrier metadata and content-addressed identity

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/sources` · **tier: core**
> What a source artifact *is* — its bytes, its location, its clock — held apart from the claims it carries.

There is no `gmeow:Source` class, and that is the doctrine. The source/claim refactor refactor
retired the anti-rigid Source Kind: being-a-source is not what a thing *is* but a role it
plays in an act of citation — source-hood is mediated by `CitationAct` (citations slice)
or borne as a SourceRole. Likewise the old Citation locator gave way to the
Selector/EvidenceSpan model (citations and evidence slices). What remains here is exactly
what *is* intrinsic to the artifact: carrier metadata — applied to the creative-works
Manifestation/Item tiers, never to the Work — and the content-digest identity discipline.

This slice is the **Source** root of the claim spine (Source → Chunk → EvidenceSpan →
Claim, Principle 14). The spine is content-addressed from the bottom: a source is
identified by what its bytes *are* (`contentDigest`), never by where they sit
(`sourceLocation`) or when the filesystem last touched them (`sourceModifiedAt`). The
same discipline carries the GTS packages — blake3 digests are the identity spine of the
signed, append-only memory format (Principle 14).

## Carrier metadata

When a creative work is an import *envelope* — a file carrying a bundle of claims — these
properties describe the artifact itself, never the claims inside it. Keeping the two
apart is the whole point: an mtime says nothing about when a marriage happened or when a
census taker wrote it down.

### gmeow:contentDigest

A content hash of an object's bytes — `"blake3:…"`, `"sha256:…"`, `"git:…"` — the
*reliable* identity: two objects with the same bytes are the same object, regardless of
path, name, or mtime. Deliberately domain-free: creative works, source files, commits,
and distributions are all content-addressable (Principle 4: one identity mechanism, not
one per artifact kind). Not functional — an object may carry digests under several
algorithms, and they coexist.

### gmeow:sourceModifiedAt

The last-modification time of the source artifact itself (a file mtime): a
*terminus-ante-quem* on the **recording** of the claims it carries — the upper bound that
feeds `gmeow:recordedNoLaterThan` in the temporal slice's four-clock model. It is NOT
valid-time and NOT observation-time; conflating the carrier's clock with the claims'
clocks is the classic provenance error this slice exists to forbid. Advisory and
resettable, hence not functional: copies of the same bytes legitimately report different
mtimes, and those reports coexist rather than force an inconsistency (Principle 9) — the
reliable identity stays `contentDigest`.

### gmeow:sourceLocation

Where the artifact came from — a file path, original filename, or URL. Provenance and
audit only; it carries **no** identity. A file renamed is the same source; a path reused
is not the same source. Typically asserted on a Manifestation or Item alongside the
digest.

## Bridges & solver boundary

The claims a source carries enter the graph through an `ImportActivity` (provenance
slice), which stamps the transaction clock; the evidentiary reach *into* the source —
which span supports which claim, with what warrant — is the evidence and citations
slices' machinery. Digest computation, dedup-by-digest, and mtime-vs-digest conflict
resolution are solver-layer computations, never assertions (Principle 12): the slice
records the digests and timestamps; deciding that two artifacts are byte-identical is a
projection-time join on `contentDigest`.

## Dependencies

Depends on `documents` (the WEMI carrier tiers the metadata attaches to). Consumed by the
claim spine's content-addressed sources and by the GTS packages' blake3 identity spine
(Principle 14).

## The canonical ingestion plan

The slice carries a worked ingestion plan: six sequential steps from raw-root
acquisition through file staging, OCR extraction, derived-claim generation and
unresolved-lead capture, to a privacy-posture assessment whose verdict governs
how the derived claims may subsequently be surfaced.

It is modelled on the canonical process spine rather than a slice-local one. The
plan is a `logic:Plan` carrying `logic:prescriptionKindIngestion`; its steps are
`logic:ActionSchema` individuals; their order is carried by binary
`logic:ControlFlowEdge` flow edges rather than by a bespoke step-sequence
property. As a plan it declares its own success criterion through
`logic:planGoal`, so "what counts as ingested" is stated once, on the plan,
instead of being implied by the steps.

Two steps are load-bearing beyond their mechanics. OCR extraction is the step
whose capability may be absent, and when it is the plan blocks with a capability
gap rather than substituting a weaker parser. Unresolved-lead capture exists so
that what the ingestion could not resolve is recorded rather than dropped — an
ingestion that silently discards what it failed to understand cannot be audited
for completeness, which is the property the whole record exists to support.

| Artifact | Realized state |
|---|---|
| `gmeow:planCanonicalIngestion` and its six action schemas | built |
| The five control-flow edges ordering them | built |
| `gmeow:goalCanonicalIngestion` success criterion | built |
| Competency cell pinning ingestion enactments | built |
