<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Refinement cue grounding

The existing AI owner supplies `Prompt`; observations supplies the actual
`Refinement` reading spine. `echoesPremiseOf` adds the specific dependence
relation while statement-layer `accordingTo` retains whose reading it is.
No global contradiction, truth or independent-evidence consequence is added.
Attribution belongs on the statement or its reifier, not on the answer in place
of its author. The ordinary example uses a selected reified result rather than
a new echo-observation framework.

`CueLexicon` inherits `InformationObject` and its logical upper sort. Its release
identity uses coreference’s `versionOf`, versions’ `versionLabel` and sources’
domain-free `contentDigest`. Source descent and responsible authorship use
`wasDerivedFrom` and `wasAttributedTo`. The new `cueReview` relation has an
existing `Activity` as its endpoint, with existing reviewer association and
point-like sign-off time. No general review lifecycle or approval framework is
introduced (Principles 4 and 5).

Existing review APIs were inspected: `logic:reviewedBy` belongs to a
`FormalizationCandidate`, and `WorkReviewProjection` is an operator view of a
work cluster. Neither describes an arbitrary cue-list sign-off. The small
activity link avoids retyping cue lists as either of those specialized objects.

Language grounding was inspected before choosing a carrier. `lang:Lexeme` is an
abstract dictionary word, including its morphological and sense identity;
`lang:SurfaceForm` is a concrete realization with sign-system and representation
obligations. A reviewed word-or-phrase list does not assert those analyses.
`Glossary` is exclusively generated from an ontology’s translation catalog, and
`GmnDictionary` is a specialized codebook alias table. Neither is a hand-authored
cue inventory. Language-tagged and directional RDF 1.2 literals preserve the
actual supplied evidence without a false lexical sort or second terminology
source. Existing structured forms can remain in consumer evidence records when
they genuinely exist; this slice manufactures none.

Canonical `logic:` domains, ranges, class-local restrictions and closure entries
make release identity and provenance required once an object is typed
`CueLexicon`. Three realized integrity laws enforce language presence for every
cue, reviewer/time evidence for every supplied review link, and a release
identity distinct from its lineage. Each outer guard directly targets a class
or property. Existing version and provenance properties retain their own
semantics; no new global functionality or universal reviewer is imposed.

Immutability is the released artifact’s identity contract. Static source laws do
not reconstruct prior bytes, authenticate a reviewer, recompute a digest or run
a linguistic matcher. Empty reviewed inventories are legal and imply no hidden
fallback. Matching details remain explicit responsibilities of consumer-owned
rules, without any new parser or execution surface.

External semantic correspondences remain with the logic, language and mathematics
owners under [the grounding contract](../../../../docs/GROUNDING.md). The
inherited upper/provenance owners supply meaningful grounding; this source review
found no justified exact external equivalent for a premise-echo interpretation
or a reviewed cue release. No downstream alignment or false equivalence is added
(Principles 17 and 19).
