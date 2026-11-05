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

A `RecallScope` is an open information-bearing selection specification under
declared context bindings. A `SlotBoundRecallScope` explicitly anchors selection
to actual `AttentionSlot` values through `scopeSlot`; at least one such anchor
is required, with no maximum. Recent conversations, current turns and
participants can remain distinct slot-bound specifications over the same real
anchor. Its supplied description states the requested relation and selection.
Shared parameterized scope individuals instead require their actual per-call
context inputs; they do not contain fabricated global slots. A selected bound
profile with a missing anchor stays invalid and never falls back to the parent.

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

## Shared context-parameterized scopes

The four shared RecallScope individuals describe requested selections. They are
specifications, not a returned record set, a currently occupied slot or evidence
that recall executed. Keep the actual schema's required scope, depth, presentation
and allocation parameters. Selecting a scope also requires its real declared
context inputs; missing inputs do not choose a weaker selection.

recallScopeAttendedNow requests the thread the agent's supplied state holds open
now, within the actual per-call state/view. It creates no global current thread,
slot assignment or preferred state claim. recallScopeRecentEpisodes requests
the agent's latest episodes under the selected source chronology. No fixed age
window, episode count, repaired clock or new episode identity is implied; an
actual recallLatest limit remains independently supplied.

recallScopePacket requests what the actual packet's own words are about. The
words as an artifact, their subject matter and a packet-kind specification are
different. Retain the actual wording and context; a standing thread or kind
label cannot silently replace the requested words. This selector supplies no
text analyzer, guessed topic, language or referent identity.

recallScopeAudience requests who is listening through the floor of the
relationship rings of everyone in the actual current conversation, together
with the configured warning text for that resulting ring. The actual roster,
relationship-ring ordering, floor rule and configured warning are consumer
inputs. Do not omit listeners from that declared roster, invent ring members or
defaults, interpret floor as a particular numerical minimum without its actual
rule, or supply a fallback warning when configuration is missing. Persona
audience ranks describe a different classification and do not substitute for
these relationship inputs. Naming the scope neither computes the floor nor
authenticates participants or warning selection.

Use SlotBoundRecallScope for an explicitly anchored specification, retaining its
actual scopeSlot values and their existing bearer/role/history/state contracts.
The conversation-turn example declares that subtype with its actual slot and
source facts. The generic parent is open to further declared
parameterized selectors. There is no closed four-value enumeration, IRI-specific
exception or scope executor; the recall-operation requiredness contract applies
(Principles 4, 9, 11, 12 and 17).

## Packet-kind specifications

PacketKind is an open Kind under InformationObject. Its instances are
information-bearing specifications of the thinking or context a model-call
packet is requested for. Such a kind specification is neither the packet,
the packet's words, nor a model-call Activity. Deployment-specific kinds remain
consumer-owned resources; the vocabulary seeds no fixed modes or cognition
taxonomy and does not recast these specifications as QualityValue measurements.

packetKind links the actual InformationObject packet to its supplied PacketKind
resource, never to literal packet words. A context-plan condition can refer to
that actual fact beside the selected state/slot facts. This does not assert that
the condition obtains, execute context assembly or authorize a model lifecycle
operation. The actual words and source context remain separately retained;
recallScopePacket requests what those words are about, not the packet's kind.

The source contract is one supplied kind fact per packet. A canonical qualified
maximum on InformationObject/packetKind/onClass PacketKind permits at most one
typed kind. Property-global provided-value closure checks supplied packet and
kind endpoints, so an invalid extra value cannot hide outside the qualified
count. There is no minimum or generic required path: absence is unknown, and
unrelated information objects need no packetKind. There is no invented Packet
class or complete-packet profile, and this vocabulary does not claim a gate
verifies kind presence on every consumer packet. Preserve the actual selected
input rather than invent a default mode or infer one from wording
(Principles 4, 9, 12 and 17).
