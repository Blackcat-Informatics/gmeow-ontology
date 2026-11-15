# Issue #2801: [term] Source snapshot: a retrieved copy with digest, retrieval time and immutable address

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: documentation, enhancement, foundational, cross-cutting, complexity-hard, terms

## Body

## Request

A consumer citing web sources as evidence needs to describe a source snapshot: the cited document as retrieved at one moment, with the digest of the retrieved bytes, the time of retrieval, and an immutable address for exactly those bytes (for example a content-addressed archive path).

## What exists

`gmeow:contentDigest` with the digest datatypes, `gmeow:sourceLocation` and `gmeow:sourceModifiedAt` (sources slice); `gmeow:WebPage` and the work, expression and manifestation layering (documents slice); `gmeow:CitationAct` with `gmeow:viaSelector` and `gmeow:selectorTextQuote` (citations slice); `gmeow:ingestedAt` on an import activity (provenance slice). There is no snapshot class, no retrieval-time property for a fetched copy, and no stated way to distinguish the live address a document was fetched from, from the immutable address of the retrieved bytes.

## Required

- A class for a retrieved copy of a source at one moment, placed in the existing document layering (a snapshot is a particular carrier of a manifestation; it is not a new work), with: the digest of its bytes, the instant of retrieval, the address it was retrieved from, and the immutable address of the stored bytes.
- Retrieval time is distinct from the four existing clocks (valid time, assertion time, source carrier time, record time), and the definitions say how.
- A citation can point at a snapshot, so a quoted passage is anchored to the retrieved bytes, not to the live address.
- A failed retrieval is not a snapshot; see the absence-reasons request.
- A worked example (one claim, one citation, one snapshot with digest, retrieval time and both addresses), a competency question returning the evidence chain of a claim, and translations.

Principles 2, 4, 7.


## Comments (1)

### paudley — 2026-11-15T01:49:54Z

Blocks: #2795 (Release 0.4.0), by the maintainer's decision.

