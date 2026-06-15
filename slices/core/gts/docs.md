<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GTS Transport Module

GMEOW describing its own transport (GTS transport design, `docs/GTS-SPEC.md`): a GTS file as a
first-class entity — its segments, profiles, chain heads, transform codecs,
opaque frames, and compaction lineage. Term-level documentation lives in
`module.ttl`; this note carries the doctrine.

## Transport claims vs content claims

**GTS gives TRANSPORT claims; GMEOW gives CONTENT claims.** A frame signature
or head commitment is a `gmeow:Attestation` whose `attestedSubject` is the
`gmeow:GTSSegment`: it proves bytes, order, and signer key control — never the
truth of the carried statements. Content claims ("this is true, signed by
Bob") are statement-level machinery (standpoint, attestation over claims)
riding as ordinary quads — opaque cargo to the transport, and therefore
invariant under transport operations *by construction*.

The corollary is `gmeow:GTSCompaction`: a rewrite re-authors only the
ordering. Content claims survive untouched; transport claims over the source
chain become detached evidence about the source document, cited through the
compaction's `wasDerivedFrom` lineage and the source's `gtsHeadId`s.
Evidence-profile documents warn on compaction (see `shapes.ttl`); sealing the
source verbatim as a nested GTS blob (spec §12.1) preserves its attestation
intact.

## Identity

Three identities, three properties, no overlap: `gmeow:contentDigest`
(sources) is byte-exact identity of a file or blob; `gmeow:gtsHeadId`
(⊑ `gmeow:versionFingerprint`) is the chain head that transitively commits to
a segment's history; the document's composite identity is the ordered list of
its segments' heads (`gtsSegmentIndex` order).

## Reuse map (no parallel mechanisms)

| transport concept | reused machinery |
|---|---|
| frame/head signatures, signers | attestation (`hasSignature`, `Attestation`) |
| blob and file byte identity | sources (`contentDigest`) |
| compaction / production lineage | provenance (`wasGeneratedBy`, `wasDerivedFrom`) |
| suppress frames | kernel P10 (`displayable`) |
| annot-frame standpoints | standpoint (`accordingTo`, `standpointModality`) |
| media payloads | documents (`MediaObject`) |

Opacity is vantage-relative epistemic state (Principle 9): the same sealed
frame is transparent to its `sealedRecipient`s.
