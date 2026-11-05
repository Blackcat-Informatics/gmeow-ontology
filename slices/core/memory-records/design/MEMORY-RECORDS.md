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

## Trace-retention classification axis

MemoryRecords owns the retention-domain classification vocabulary and consumes
the Notes owner’s actual Annotation carrier. `TraceMarkClass` specializes
`logic:QualityValue`; its open members classify annotations rather than events
or source content. The optional, nonfunctional `traceMarkClass` role names
actual supplied values on an Annotation, without a preferred category or a
closed local taxonomy (Principles 4 and 9).

The single property-global closure validates supplied Annotation subjects and
TraceMarkClass values. No class-local requiredness, cardinality, functionality,
Constraint, generic annotation Kind or partially specified mark class is added.
Existing Annotation target and motivation obligations remain intact. The actual
curator and standpoint use existing provenance rather than an Observation
vantage; no target, motivation or body is manufactured to satisfy a parent
contract. The direct Notes dependency reflects this real formal reuse.

The high-friction correction value classifies a teller’s reading of an earlier
unsuccessful attempt followed by another attempt of the same tool and arguments
in an episode. It supplies no formal class or event type for that reading and
no rule that derives attempts, equality, chronology, truth or endorsement from
the value. Evidence and attribution remain on the actual source reading and
annotation. The category neither carries confidence nor executes retention.

The classification foundation is separate from packet identity and trace
finality. Existing record identifiers and ordinals do not supply a packet’s
digest algorithm, preimage or comparator. A finalized committed journal head,
distributed-ledger settlement and a trace watermark below a continuing packet
order are different contracts. No alias, generic protocol carrier or finality
claim is introduced by these classification terms (Principles 5, 11, 17 and 19).
