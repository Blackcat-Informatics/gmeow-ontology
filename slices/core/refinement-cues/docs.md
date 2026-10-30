<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Attributed premise echoes and cue lexicons

`echoesPremiseOf` relates an answer information object to the concrete `Prompt`
whose premise a refiner reads it as merely restating. This is a dependence
reading. It does not establish that either text is false, that they contradict
each other or that the answer supplies independent support.

Retain the refiner’s `accordingTo` on the RDF 1.2 assertion or its reifier.
The existing `Refinement` spine records the answer as `observedFeature`, the
versioned process as `vantage`, the reading outcome and its selected
`observationResult`. A reified result can carry the dependence statement without
asserting its content as a global fact. Preserve the answer author separately
through ordinary provenance; the refiner’s interpretation is not the author’s
own claim (Principles 3 and 9).

A `CueLexicon` is an immutable reviewed version of a named complete-word or
phrase inventory. Each `cueWord` retains its exact text, language and any RDF 1.2
base direction. A list can be empty; an empty reviewed inventory supplies no
cue. No ordering, priority, stemming, tokenization, normalization or substring
matching policy is inferred from these values. A consumer’s reading rule owns
those operations. A cue is evidence for a reading, never the text’s meaning.

Version and provenance use the existing owners:

| Relation | Recorded information |
| --- | --- |
| `versionOf` | Stable lineage distinct from the immutable release. |
| `versionLabel` | Supplied release designation; no ordering is inferred. |
| `contentDigest` | Algorithm-labelled digest of the exact released payload bytes. |
| `wasDerivedFrom` | Original source records from which the cues were derived. |
| `wasAttributedTo` | Authorship or accountability for the cue inventory. |
| `cueReview` | Completed sign-off activity for this exact release. |

The sign-off retains its responsible reviewer through `wasAssociatedWith` and
its actual sign-off instant through `eventTime`. This point-like sign-off is
distinct from any longer review episode. It records accountable review, not
universal endorsement, guaranteed accuracy or safety certification. Multiple
reviewers, sources and digest algorithms can coexist.

Changes to text, language, direction or the released payload create a new version
and digest. Keep prior releases intact; `supersedes` may relate successor releases
without deleting the old one. Review evidence must identify the changed release
when it is reviewed. The ontology checks language presence, reviewer/time evidence
and distinct version-versus-lineage identity; it does not execute review, verify
cryptographic hashes or detect changes to a historical external artifact.

The ordinary [review and finding scene](examples/review-and-echo.ttl) accompanies
an actual [RDF 1.2 cue payload](examples/cue-words-v1.ttl). Read the two files
together: the payload provides the cue values and the scene supplies their
version, source, review and process provenance. The stated digest is over the
payload file’s exact UTF-8 bytes, including its final newline. The example
records a process reading and makes no executed matching or review claim.
