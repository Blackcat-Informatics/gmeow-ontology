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
frame holds a claim, `gmeow:wasAttributedTo` says which source recorded it, and
`gmeow:confidence` says how sure we are — a neutral archive can record a partisan claim,
and a claim can be high-confidence yet from a frame we dispute.

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
orthogonal to standpoint and to source (the issue #51 three-axis doctrine) — it answers
"how sure", never "who says" or "who recorded".

### gmeow:importanceLevel · gmeow:mappedFrom

The remaining statement-level audit pair: relative importance on a 0–10 scale (projection
takes the maximum across imports — a solver-layer fold, Principle 12), and the source
property a claim was mapped from during ingestion, recorded so every mapping step is
auditable end to end (Principle 7: verified by construction).

### gmeow:provenance

The Dublin Core custody statement (issue #60): a free-text account of ownership and
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
