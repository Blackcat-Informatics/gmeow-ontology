# Issue #1813: [term] Refinement outcomes, and coreference at volume (consumers: memetics, katamari)

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement

## Body

## Summary

Both consumers derive structured observations from records after the
fact: resolving who a mention refers to, which event a passage narrates,
what a record implies. Each such reading has an outcome: resolved, best
guess, ambiguous between candidates, or unresolved. GMEOW has no
vocabulary for that outcome. The same modelling turned up two
coreference ergonomics questions.

Checked against `main` at `a631f73` (2026-10-10).

## Consumers

- **memetics**: the memory adapter for a companion agent. Its
  refinements are observations of other records. A refiner (named and
  versioned) reads a record and states what it found. It never alters
  or annotates the source. Refinements at volume include mentions,
  coreference, time joins and story beats.
- **katamari**: the governed store. It takes memetics' four outcomes as
  its starting default for the same concept. Entities minted by a
  refinement are anchored to the observation that minted them ("a
  person is a cluster, not an id").

## 1. Refinements and their outcomes (joint proposal)

**Today.**
- `gmeow:stepIngestionUnresolvedLeads` (`core/sources`) covers only the
  unresolved case.
- `gmeow:candidateHypothesis` (`core/inference`) belongs to an
  `InferenceCommitment`, not an observation.
- `gmeow:Determinacy` (`core/kernel`) and `gmeow:SupportStatus`
  (`core/standpoint`) overlap with the outcome axis only in part:
  determinacy is about the world, support about evidence. A refinement
  outcome is about the reading.

**Proposed terms.**
- `gmeow:Refinement`, a sub-kind of `gmeow:Observation`: "An observation
  a named, versioned process makes of another record. Its vantage is the
  process, its observed feature the source record, and its results the
  observations it found there. It never alters or annotates the source."
- `gmeow:RefinementOutcome`, a value vocabulary, plus
  `gmeow:refinementOutcome`:
  - `refinementResolved`: one reading, confidently;
  - `refinementBestGuess`: one reading, not confidently;
  - `refinementAmbiguous`: several candidate readings, none chosen;
  - `refinementUnresolved`: no reading found.
- `gmeow:refinementCandidate`, an object property from a refinement to
  one candidate of an ambiguous outcome, named without being asserted.

**Why a separate axis.** The outcome says how far the reading got, which
neither determinacy nor support expresses. Recall uses it to decide
whether to show a result, to hedge it, or to ask which candidate was
meant.

**Open questions.**
- Should `refinementCandidate` reuse `candidateHypothesis`, by widening
  its domain, rather than mint a sibling?
- Should "best guess" carry a credence (`gmeow:credence`), or is the
  outcome enough?

## 2. Per-mention coreference at volume

**Today.** `gmeow:corefDenotation` (`core/coreference`) reaches a
reified `lang:Denotation`, which must name form, kind, target and
context. A `lang:Form` must itself be situated. So every mention costs
two records.

**Evidence.** One story pass over a few weeks of records produced 55
mentions, which became 110 records.

**Ask.** Is there a lighter sanctioned pattern for per-mention
coreference at volume? For example, a mention with surface text, span
and target, and the full denotation only when a mention is disputed.

## 3. A chunk that also narrates

**Today.** `gmeow:narratedIn` (`extensions/narrative`) ranges over
`gmeow:ContentSegment`, while a retrieval chunk is a `gmeow:Chunk`
(`core/ai`). A record that is both a chunk read and a segment narrating
an event must be typed twice.

**Ask.** Is a `Chunk` meant to be a `ContentSegment` (subclass), or
should `narratedIn`'s range widen?

## Local stand-ins

memetics' local observations slice mints `gmeow:Refinement`,
`gmeow:RefinementOutcome` (with its four members),
`gmeow:refinementOutcome` and `gmeow:refinementCandidate`, in GMEOW's
namespace. Upstreaming is a move. katamari has no stand-in yet and
adopts the upstream terms when they land.


## Comments (0)

