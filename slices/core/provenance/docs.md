<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Provenance — activities, attribution, and the statement clocks' epistemic siblings

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/provenance` · **tier: core**
> The PROV-O superset layer: who made it, what produced it, and how sure we are — on every statement in every slice.

A claim without provenance is the failure mode this ontology was written against
(Principle 14: an LLM output is a claim, not a truth). This slice supplies the two halves
of the answer. The **individual** half is a compact PROV-O-aligned activity model
(Principle 5): activities that generate entities, agents that are attributed or
associated, derivation chains from source to extraction to summary. The **statement** half
is a set of cross-cutting annotation properties (`confidence`, `importanceLevel`,
`mappedFrom`) that ride on the RDF-1.2 reified-statement layer (Principles 2–3), so any
fact in any slice carries its epistemic weight without a single new relator.

On the claim spine (Source → Chunk → EvidenceSpan → Claim, Principle 14) this slice is the
*chain of custody*: the `ImportActivity` that ingested the Source, the `wasDerivedFrom`
chain from Source to Chunk to extraction, and the `confidence` annotation on the final
Claim. The three epistemic axes never bridge (Principle 9): `gmeow:accordingTo` says whose
frame holds a claim, the `wasDerivedFrom` chain back through its `ImportActivity` says
which source recorded it, and `gmeow:confidence` says how sure we are — a neutral archive
can record a partisan claim, and a claim can be high-confidence yet from a frame we
dispute. (`gmeow:wasAttributedTo` plays a different role entirely: it ascribes
responsibility for an *entity* to an *agent* — see below.)

## Activities

### gmeow:Activity

Something that occurs over time and acts upon entities — creating, transforming, using,
attributing. A `gufo:EventType` under `gmeow:Event`, so the temporal slice's clocks and
the events slice's participation machinery apply unchanged (Principle 4: one canonical
event model).

### gmeow:ImportActivity

The ingestion act: an activity that consumed an external envelope (a vCard file, a mail
archive, a GTS package) and recorded the claims it carried. Carries `gmeow:ingestedAt` —
the *transaction* clock: when the system learned the claims, held strictly apart from
`assertedAt` (when someone claimed them) and `validFrom`/`validUntil` (when they hold).
Three clocks, never collapsed.

### gmeow:wasGeneratedBy

Entity → Activity: the activity that produced an entity. The provenance leg every derived
artifact (extraction, summary, embedding, reasoned graph) must carry; pairs with
`gmeow:observationEvent` in the observations slice, which gives the *event* perspective
(when) rather than the *generation* perspective (by what process).

### gmeow:wasAttributedTo · gmeow:wasAssociatedWith

The two attribution directions: `wasAttributedTo` ascribes an endurant Entity to a
responsible Agent; `wasAssociatedWith` relates an Activity to the agent carrying it out
(e.g. the software agent that ran an import). Keep them straight — attributing the
*output* and crewing the *process* are different facts.

### gmeow:wasDerivedFrom

The derivation backbone, deliberately domain-free so events, endurants, and information
objects all participate: a `TextExtraction` from its PDF, a `Summary` from its source, an
embedding from its chunk, a commit from its parent, a trajectory from its sample stream.
Confidence and generating agent ride alongside via `gmeow:confidence` and
`gmeow:wasGeneratedBy` — a derivation is itself a claim about lineage.

### gmeow:Summary · gmeow:TextExtraction

The two canonical derived information objects: a condensed (often machine-generated)
account, and the text content pulled from a source artifact. Both link back with
`wasDerivedFrom`; both are claims about their source, not replacements for it — the
source stays, suppression never erasure (Principle 10).

## The statement-level annotations

### gmeow:confidence

Epistemic confidence in a claim, in [0,1], attached to the statement it qualifies. An
annotation property so the OWL downcast stays DL-clean (Principle 3). Confidence is
orthogonal to standpoint and to source (the standpoint doctrine three-axis doctrine) — it answers
"how sure", never "who says" or "who recorded".

### gmeow:importanceLevel · gmeow:mappedFrom

The remaining statement-level audit pair: relative importance on a 0–10 scale (projection
takes the maximum across imports — a solver-layer fold, Principle 12), and the source
property a claim was mapped from during ingestion, recorded so every mapping step is
auditable end to end (Principle 7: verified by construction).

### gmeow:provenance

The Dublin Core custody statement: a free-text account of ownership and
custody changes significant for authenticity and interpretation. The human-readable
complement to the structured activity chain, not a substitute for it.

## Solver boundary & alignment

Trust-weighted aggregation, confidence combination across sources, and transitive
derivation closure are computations, never assertions (Principle 12): the slice records
the inputs (activities, attributions, per-statement confidence); ranking and fusing them
is projection policy. Aligned by reference to **PROV-O** (`prov:Activity`,
`prov:wasGeneratedBy`, `prov:wasAttributedTo`, `prov:wasAssociatedWith`,
`prov:wasDerivedFrom`) — GMEOW's superset adds the statement-level annotation axis and
the transaction clock.

## Dependencies

Depends on `kernel`, `documents`, and `events`. Consumed by every attributed fact in the
graph: the statement-layer attribution axes, import pipelines, and the GTS package
chain-of-custody (Principle 14).

## The source content a summary condenses

`summaryOf` identifies the particular information object whose content a
`Summary` condenses. Its range includes a source work, a `ContentSegment`
division or another `InformationObject`; using a source work does not classify
that work as a structural division. Multiple identified sources may coexist,
and no unique target or required path is imposed.

```turtle
ex:summary gmeow:summaryOf ex:division .
```

The property specializes `wasDerivedFrom`, so this assertion also supplies the
generic lineage link. Keep any other actual derivation inputs separately:
production instructions or context in the lineage do not automatically become
summarized content. Preserve source, viewpoint and confidence on each exact
assertion. The relation does not certify fidelity, complete coverage, model
execution or source-byte identity.

The source role remains distinct from retrieval. `chunkOf` names the carrier of
the retrieved text; the source a generated summary condenses does not become
that carrier or a division containing the generated text. A source that actually
carries text remains a valid summarized source. One object can be both Summary
and Chunk only when it independently satisfies both existing meanings. Keep the
actual carrier, source pins, digests and span coordinates unchanged; the
specialized source role introduces no parthood, contiguous-cut or offset claim
(Principles 2, 4, 7, 9, 12, 17 and 19).

## Whole provenance bookkeeping kept elsewhere

`provenanceDigest` is the Activity record's pointer to the separate provenance
bookkeeping item its producer stamped on that record and then set aside whole
outside the knowledge graph for review. Evidence identifiers, substrate and
contract digests, episode kind, producing component and schema version can be
machinery carried by that item rather than facts expanded leaf by leaf in the
graph. The pointer identifies the whole retained item; it does not turn that
item into a graph node or a derivation source of the Activity.

Keep the supplied literal as `xsd:hexBinary`, with exactly sixty-four bare
lowercase hexadecimal digits. SHA-256 is fixed by this particular property's
meaning, not inferred from an arbitrary binary literal. The canonical lexical
Constraint checks the supplied spelling; the property's provided-value closure
separately validates the Activity and exact datatype endpoints. Uppercase,
prefixes, whitespace, wrong widths and a different datatype are not alternate
representations accepted by this contract. Do not normalize, silently retype
or substitute another algorithm when an input fails.

The mathematical SHA-256 meaning already has an owner: see
[`gmeow:sha256Digest`](../../grounding/math/docs.md#algorithm-tagged-digest-literals).
That named datatype carries algorithm identity in its own literal values.
This property's supplied `xsd:hexBinary` terms remain distinct from those values;
the documentary reference does not change their datatype or declare equivalence.
No new hash algorithm, datatype, grounding seam or manifest edge is introduced.

The preimage is the exact identified byte representation of the separate
set-aside item. It is not the Activity record's own bytes: that different role
belongs to `contentDigest`. Preserve the actual item/byte identification and
source attribution without guessing an encoding, canonical serialization or
store URI. For example, a producer can retain the record's evidence identifiers
and contract bookkeeping as one separate item and supply its SHA-256 pointer,
without expanding that bookkeeping into graph facts. The pointer reports the
digest of that item; syntax acceptance neither computes the digest nor verifies
that the value agrees with the bytes.

The consumer's actual external store can use the pointer to find the item while
it is retained. This relation supplies no locator or retrieval implementation,
performs no lookup and guarantees no retention period or permanent availability.
The historical pointer does not certify that its item is still retrievable.
Provenance already stated as facts keeps its actual structured provenance paths;
this pointer does not replace them or imply `wasDerivedFrom`.

The role is optional and nonfunctional. The reported one-pointer-per-record
workflow keeps one whole bookkeeping item rather than individual leaf pointers;
it does not require a pointer on every Activity or merge independent source
assertions into one globally preferred value. There is no new wrapper, required
record profile, artifact node, source/force classifier, byte verifier, native
failure marker or runtime engine (Principles 2, 4, 7, 9, 12, 17 and 19).

## Approval of a body of work

### gmeow:Approval

`gmeow:Approval` records that a named agent approved a body of work on a given day. It
is a relator, declared like `gmeow:Attestation` and `gmeow:Contribution`, mediating two
roles: the approver (`gmeow:approvedBy`, an agent) and the work approved
(`gmeow:approvalOf`, an entity). The second role is what makes the approval about
something; a relator needs two relata. `gmeow:approvedOn` carries the calendar day as an
`xsd:date`, with no invented time of day. `gmeow:approvalScope` optionally names the
part of the work covered, either a component reached by `gmeow:partOf` or a citation
selector for a pinpoint; when it is absent the scope is unstated, never "the whole work".
`gmeow:reportedBy` optionally names the agent who put the approval on record.

```turtle
ex:approval1 a gmeow:Approval ;
    gmeow:approvalOf ex:handbook ;
    gmeow:approvedBy ex:drOkafor ;
    gmeow:approvedOn "2026-11-05"^^xsd:date ;
    gmeow:approvalScope ex:handbookChapter3 .
```

An approval is never a holder. Who holds the claims in a work (`gmeow:accordingTo`), who
is responsible for it (`gmeow:wasAttributedTo`) and who is credited for making it
(`gmeow:Contribution`) are separate facts, and `gmeow:approvedBy` is not a subproperty of
any of those properties. Both kinds of fact can be stated about the same agent without
one implying the other. `gmeow:reportedBy` says only who recorded the approval; it
differs from `gmeow:wasAttributedTo`, which ascribes responsibility for an entity.

The class allows at most one approved work, one approver and one approval day per
`gmeow:Approval` node, stated as qualified cardinality restrictions on the class. A joint
approval, an approval of several works and an approval on another day are therefore
several `gmeow:Approval` nodes. A later approval, or a
correction, adds a node; the identifier and label of an existing node are never changed.
That last rule is a usage contract that no axiom enforces.

Use `gmeow:Attestation` for signed or policy-bound vouching, `logic:ApprovalCommitment`
for authorizing an effect before dispatch, and `gmeow:Contribution` for credit for
making the work (Principles 7, 9, 10, 14).
