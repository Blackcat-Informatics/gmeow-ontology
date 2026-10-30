<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Memory ownership and source boundaries

The memory item remains the `ai` owner's observation role. Its prose field adds
literal content without deriving an information-object type for the remembered
claim. The lasting bank identity is a separate information object. Existing
coreference `versionOf` binds revisions to that identity, and `supersedes`
preserves replacement history. The bank's textless invariant applies to payload
fields, not to its human-readable label.

The audience value really specializes kernel `ProjectionContext`, whose existing
meaning is the who-axis of projection. Disclosure posture remains the independent
`DisclosurePolicy`. The annotation-property form of `bankTier` preserves a
claim-local audience; an annotation-to-object subproperty assertion would change
the OWL projection's typing and is not authored. Actual eligibility, access checks
and release decisions remain consumer computations.

`BankReview` and `CurationEpoch` are event classes specializing the real
provenance `Activity`, not endurant kinds. Review completion of the current
decision and complete reading of a revision are distinct facts: the activity can
record a crisp decision instant and still have `readToEnd false`. Several review
decisions may coexist. None is modeled as proof of subsequent mutation.

The existing `Summary` supplies source derivation semantics. A paraphrase anchor
belongs to the summarizer's text, so it is neither a quotation nor a separate
source extraction. Runtime-kept appraisals reuse `accordingTo` directly, just as
other information objects do. Their beneficiary, original word source and runtime
responsibility remain distinct; no default self-assertion is fabricated.

Historical curation is modeled only on the external source-evidence side of the
existing claim-lifecycle boundary. The event produces a successor artifact from
a predecessor; a documentary description is evidence about that event. These
directions justify the actual `wasGeneratedBy` and `wasDerivedFrom` obligations
on the successor, while forbidding a spurious documentary ancestry of the event.
Original artifact identity uses the sources owner's domain-free `contentDigest`.
Epoch-specific digest fields name other artifacts' bytes and therefore are not
subproperties of an Activity's `contentDigest`.

Canonical class-local value restrictions and closed required paths state selected
record contracts without changing generic memory items, summaries or activities
that do not use these new fields. Thirteen realized integrity formulas cover
language-bearing values, textless bank identity, bank revision prose, review
lineage, summary source evidence, runtime offering standpoint, source artifact
digests, digest agreement and lexical tags, and historical successor provenance.
Every outer guard has an actual class or predicate target; disjunctions are never
unsupported OR-only outer guards. No hand-authored shape, parser, hashing engine,
review framework or store executor is introduced.

These are semantic reuse claims, not asserted exact equivalences to an external
memory or review vocabulary. The canonical observation, audience, provenance,
source, event and coreference owners supply meaningful grounding without erasing
their distinctions (Principles 4, 5, 9, 11, 17 and 19).
