<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Recall context

This slice records how a memory service selects context, which records it
actually returned, and how the asking agent judged the answer. It describes
consumer-owned operations; it introduces no executor or model lifecycle authority.

## Precomposed context plans

Use the existing `logic:Plan` with
`logic:prescriptionKind gmeow:prescriptionKindContextPlan`. Its
`contextPlanCondition` names the `logic:Situation` under which the plan is
selected. Naming the condition does not assert `logic:situationObtains`.
The existing program combinators and `logic:planBody` describe its precomposed
steps. An agent-flow kind instead describes steps the agent chooses at run time.

Each `RecallOperation` is a plan-local `logic:ActionSchema` with an allocation
identity. `recallSchema` identifies its reusable template. It carries the requested
`recallScope`, `recallDepth` and `recollectionForm`, and its `budgetShare`.
`recallLatest` optionally limits selection to a positive number of newest records;
it is neither an age window nor a guarantee that enough records exist.

A `RecallScope` is a specification anchored to an `AttentionSlot` through
`scopeSlot`. Recent conversations, the turns of the current conversation and
its participants can be different specifications over the same anchor. The
scope's own description states the relation and selection it requests.

The open depth vocabulary initially distinguishes:

- `recallDepthQuick`: attended and recent records, using lexical retrieval only.
- `recallDepthOrdinary`: every declared surface, using lexical and nearest-neighbour
  retrieval followed by reranking.
- `recallDepthDeep`: ordinary recall, then link expansion and distant bands.

Selecting a profile requires its declared capabilities. Missing capabilities
are not permission to substitute a weaker profile. A profile describes the
operation requested, not successful execution or the existence of an answer.
`answerDepth` separately records the profile reported for the actual answer.

Presentation is independent of depth. `recollectionFormVerbatim` preserves the
source wording; `recollectionFormSummaryHandle` presents a short attributed
summary and a resolvable handle while retaining the original. Neither form
asserts the truth of recalled material.

## Count budget allocations by identity

A `RecallBudget` names its existing plan through `budgetForPlan` and explicitly
enumerates the finite allocation roster through `budgetOperation`. It is not
another program body. One plan has one such budget record, and an operation
allocation belongs to one roster. Reusing a schema twice requires two operation
identities, even if all their parameter values happen to match.

For the declared finite operation set O, the laws are:

```text
for every operation o in O:  0 < budgetShare(o) <= 1
sum over operation identities o in O of budgetShare(o) <= 1
```

The canonical `RecallBudgetSumRule` joins `budgetOperation` to `budgetShare`,
retains the operation variable in each full substitution, and uses the existing
native `SUM` reduction grouped by budget identity. Two operations with share
0.4 contribute 0.8, even when they use the same template. There is no numeric
`DISTINCT` and no second authored list of share literals. `recallBudgetTotal`
is the derived view checked by `RecallBudgetTotalBoundConstraint`; consumers
must not author an independent total or omit selected operations from the roster.
These allocation records describe shares of one enactment's token budget,
not measured token consumption or permission to exceed a selected resource bound.

## Slots, positions and assignment claims

An `AttentionSlot` is the continuing identity of an agent's named slot. Its
`slotBearer`, consumer-defined `slotRole` and `slotHistory` identify the bearer,
role and history path. The vocabulary deliberately chooses generic slots rather
than inventing a universal list of consumer-specific roles.

`SlotPosition` specializes `gmeow:State`: it is an immutable position on that
history, not the mutable slot or a situation's content. `positionInSlot` names
its slot, and `slotPointer` names an existing record by IRI. The pointer does not
copy the record or coerce its target into an information object. Resolution is
a consumer obligation; checking that a value is an IRI does not establish that
the record is currently accessible.

`slotState` selects a position in a consumer's current view. Each assertion has
its own `SlotAssignmentClaim` RDF 1.2 reifier and writer attribution. The
`rdf:reifies` tuple itself identifies the slot and position; no duplicate role
fields can disagree with it. A successor claim names the claims it replaces
through existing `gmeow:supersedes`; it does not supersede or delete the pointed
record. The consumer reads those actual tuples to maintain each slot's own history
and checks that its predecessors concern the same slot and bearer. The canonical
predecessor constraint requires an actual assignment reifier as predecessor; it
does not compare tuple components or equate the recorder with the slot bearer.
Recorder attribution can change between claims. Concurrent claims remain
attributed and retained. The consumer selects its current view; the ontology does
not manufacture a global winner or infer causation merely from a supersession edge.

## Give each excluded record its reason

Select the grounding-owned `logic:PerRecordExclusionAssembly` profile for an
assembly whose exclusions each need an explanation. Every `assemblyExcluded`
membership then requires its own `logic:AssemblyExclusion` reifier and reason.
An aggregate explanation does not discharge that requirement, and this profile
does not additionally require an aggregate explanation.

`gmeow:exclusionReason` is the domain-facing specialization of
`logic:assemblyExclusionReason`. Assert one reason fact on the actual reifier;
the canonical subproperty relationship supplies the generic reading. Preserve
the reason's language and any RDF 1.2 base direction. Budget exclusion and
already-surfaced exclusion remain explanations of their respective records.
Assemblies that do not select the per-record profile keep the existing
aggregate-reason obligation.

## Answers and the asker's judgement

`RecallAnswer` is an Activity. Retrieval activities can name it with the existing
`subEventOf` relation. Its `answers` edge names the actual `Question`, whose
`questionText` carries the cue wording. The existing answer relation has an open
domain; this use does not turn the activity into a Proposition. `recallAsker`
identifies the agent who holds that Question open with `asks`.

`recalledRecord` names actual selected records, preserving their existing sorts.
Each membership has its own `RecalledRecordClaim` reifier carrying a positive
`recallRank`. The rank orders that membership in this answer; it is never a
confidence score or a property of the record in every answer.

The outcome and completeness fields keep these reports distinct:

| Outcome | Meaning | Completeness |
| --- | --- | --- |
| `recallOutcomeRecollections` | At least one selected record | Reported independently |
| `recallOutcomeEmptyComplete` | Declared scope searched completely, no returned records | True |
| `recallOutcomeDeadline` | Deadline interrupted the search | False |
| `recallOutcomePartial` | Some declared scope was not searched | False |

A complete empty result does not prove absence outside its scope. An incomplete
answer can retain useful selected records. No outcome asserts their truth.

A `RecallVerdict` is a separate information object: `verdictOn` identifies the
particular answer, `wasAttributedTo` identifies the same agent as its
`recallAsker`, and `recallVerdict` records found, partly found, not found or wrong.
`verdictWords` preserves that asker's exact language-bearing wording. Another
agent's judgement requires another attributed record; neither a verdict nor
matching cue text silently resolves someone else's question. Verdicts do not
overwrite the answer's search-completeness report.

## Prescription identity remains explicit

Prescription versions keep the content-addressed default. An absent identity
selector still requires `logic:prescriptionDigest`.
`logic:prescriptionIdentityMode logic:ImmutableIriPrescriptionIdentity` explicitly
selects a different contract: an external owner maintains immutable IRI resolution,
durable retention and revision by supersession. This supplies no hash comparison
and detects no edited bytes. Any independently selected digest-bearing checkpoint,
approval or other admission still requires its original evidence. Supplying a
digest retains the existing immutability comparison against a run's pinned digest.

The [ordinary scene](examples/context-recall.ttl) records two equal allocations,
a slot change, two separately explained exclusions, selected records and the
asker's judgement. The [design notes](design/RECALL-CONTEXT.md) identify the
canonical source and projection boundaries.

## Recency-graded requests and one-line gists

recollectionFading selects a requested presentation strategy on the actual
ActionSchema through recollectionForm: newest source records verbatim, older
records as summaries, and oldest records as a line with a handle. It does not
classify one delivered record as having all those forms, implement a
summarizer or select numeric age cutoffs. Retain any source-specified recency
policy and time reference separately; preserve actual original records,
handles and produced-output evidence independently.

recollectionFormOneLineGist is a separate uniform presentation choice. It
requests a concise one-line gist without requiring a handle or claiming
verbatim wording. It is distinct from both recollectionFormSummaryHandle,
which includes a resolvable handle, and the recency-dependent fading strategy.
Neither choice changes recallDepth or demonstrates that retrieval or
presentation ran (Principles 2, 4 and 9).

The [graded and gist request scene](examples/graded-and-gist-recollection.ttl)
records actual source choices without creating an executor or fabricated
output.
