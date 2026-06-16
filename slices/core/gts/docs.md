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

## Terms

### gmeow:GTSDocument · gmeow:GTSSegment · gmeow:gtsSegment · gmeow:gtsSegmentOf · gmeow:gtsSegmentIndex

A GTS file as a first-class entity and the segments that compose it. `gtsSegment`
(with its inverse `gtsSegmentOf`) relates a document to its segments, and
`gtsSegmentIndex` fixes their order — the document's composite identity is the
ordered list of its segments' chain heads.

### gmeow:gtsHeadId · gmeow:GTSProfile · gmeow:gtsProfile

The chain head (`gtsHeadId`, ⊑ `gmeow:versionFingerprint`) that transitively
commits to a segment's history; and the transport profile (`GTSProfile`) a
document declares, attached by `gtsProfile` — evidence-profile documents, for
instance, warn on compaction.

### gmeow:GTSCompaction

A rewrite that re-authors only segment ordering: content claims survive
untouched, while transport claims over the source chain become detached evidence
cited through the compaction's `wasDerivedFrom` lineage.

### gmeow:TransformCodec · gmeow:usesTransformCodec · gmeow:CodecClass · gmeow:codecClass

A transform codec applied to bytes in transit, attached by `usesTransformCodec`;
`CodecClass` is the open value vocabulary of codec families a codec belongs to
via `codecClass`.

### gmeow:OpaqueFrame · gmeow:opaqueFrameIn · gmeow:sealedRecipient · gmeow:OpacityReason · gmeow:opacityReason

A frame whose payload is sealed: `opaqueFrameIn` situates it within a document,
`sealedRecipient` names the vantages to which it is transparent, and
`opacityReason` (an `OpacityReason` value) records why it is opaque — opacity
being vantage-relative epistemic state (Principle 9).
