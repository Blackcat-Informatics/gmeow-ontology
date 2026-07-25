<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Work Orchestration — the domain binding of the enactment kernel

> The charter of `slices/core/work-orchestration`. It governs **one binding** of the general
> prescription → enactment → commitment kernel charted in
> [`LOGIC-ENACTMENT.md`](../../../grounding/logic/design/LOGIC-ENACTMENT.md): durable work clusters —
> continuing goals with immutable versioned prescriptions, recurring immutable enactments, versioned
> guidance, recorded context assembly, and an operator review surface that explains every action it
> shows. The kernel owns identity, lifecycle, refinement, and commitment; **this slice owns every
> domain-typed edge**, and nothing else.

## Why the binding is a slice and not an extension of the kernel

The `logic:` slice is the bottom of the dependency graph. It declares zero slice dependencies, its
terms are standalone by design — no gUFO stereotypes, no `rdfs:subClassOf` into `gmeow:` reasoned
classes — because `gmeow:` is a *generated lossy projection* of `logic:`, not its ground. A grounding
slice never depends on a non-grounding slice. Any edge from `logic:` into `gmeow:` would invert the
projection direction, and any edge into the preference vocabulary would additionally be a dependency
cycle, since preference already depends on `logic:`.

That constraint is not an obstacle to route around; it is the reason this slice exists and the reason
the kernel came out general. Every term this slice mints is a **domain-typed specialization** of a
kernel term, authored in the direction the architecture already uses: `gmeow:X rdfs:subPropertyOf
logic:Y`, with a `gmeow:` range on the `gmeow:` side. Property signatures — `rdfs:domain` and
`rdfs:range` — are not counted by the projection-vocabulary ratchet; subsumption is, so every net-new
subsumption is authored as a typed `logic:subClassOf` + `logic:formalizes` axiom node rather than a
bare structural axiom.

The slice is **core tier**, not an extension, and it meets the machine-readable core test on the
"more than a quarter of expected uses" limb: durable enactment with decision support is the shape of
the majority of consuming applications, and the slice interlinks with the goal, norm, calendar,
events, note, evidence, preference, and versioning spines simultaneously. Core tier also forbids it
reaching any extension, which is a real design constraint on every edge below and is checked rather
than assumed.

## The continuing-cluster thesis

A **work cluster** is a *continuing* thing. This single word carries the whole design.

The naive model of recurring work is a queue of tasks that are created, completed, and forgotten. It
is wrong in a way that shows up immediately in use: the recurring review that ran last week has
nothing to say to the one running this week, the goal is "achieved" every Friday and mysteriously
un-achieved every Monday, and the accumulated judgment of a year of reviews lives in a chat log.

`gmeow:WorkCluster` is instead the durable identity that *outlives* every occurrence of the work:

- **`gmeow:clusterGoal` → `gmeow:Goal`.** Typically a maintenance goal — *ADRs stay reviewed*, *the
  fleet stays within setpoint*, *the intake queue stays under a day old*. The goal does not close
  when an enactment completes; `logic:MaintenanceGoalNotClosedByEnactmentConstraint` is the kernel law
  that guarantees it, and it is the formal content of "continuing".
- **`gmeow:clusterPolicy` → `gmeow:Norm`.** The governing deontic force, carrying its modality,
  issuer, bearer, and authority level, and ordering other norms through `gmeow:overrides` with a
  recorded precedence tenure. Approval requirements and automation licence are read from here, never
  hard-coded into a step.
- **`gmeow:clusterSchedule` → `gmeow:EventSchedule`.** The generator, never the generated.
- **`gmeow:clusterActivePrescription` → `logic:PrescriptionVersion`, exactly one.** The pointer moves
  by supersession; it never mutates a version. An in-flight enactment pins the version it started
  under, so moving the pointer changes what the *next* enactment does and nothing about a running one.
- **`gmeow:clusterGuidanceSet`, `gmeow:clusterNote` → `gmeow:Note`, `gmeow:clusterHistory`.** The
  accumulated judgment: versioned guidance, retained notes and observations, and the roster of prior
  enactments that a current one may compare itself against.

The cluster is therefore the join point between a goal that never closes, a prescription that is
revised by supersession, a schedule that keeps generating, and a body of retained judgment. Remove any
one and "continuing" degrades into "repeated".

**A cluster is not a plan, and it is not an enactment.** It mints no plan concept: its prescription is
a `logic:PrescriptionVersion` over a `logic:Plan`. It mints no run concept: each occurrence generates
a `logic:Enactment`. Its own identity is the continuing goal together with the standpoint that holds
it — which is why it is a domain term and could not have been minted in the kernel, whose terms may
not name a `gmeow:Goal`.

### Fresh, resumed, and revised are three different things

The three continuations the kernel distinguishes have three distinct readings at the cluster level,
and an operator surface that blurs them is unusable:

| Continuation | At the cluster | What is pinned |
| --- | --- | --- |
| `logic:ContinuationRepeat` | this week's occurrence — a **new** enactment at `EnactmentPending`, against a **new** closed input snapshot | the cluster's active prescription version *at the moment the occurrence is generated* |
| `logic:ContinuationResume` | an occurrence that was paused or interrupted continues — the **same** enactment advances | the version and snapshot the enactment already pinned; a restore that mismatches any of the seven identity axes is refused |
| `logic:ContinuationRevise` | the prescription itself changes — a new `logic:PrescriptionVersion` supersedes its predecessor | nothing about in-flight enactments changes; they keep their pins |

The distinction is enforced by disjointness with distinct required bindings per kind, and it is what
lets "the review discovered newly added and changed ADRs since last week" be a *modelled* fact: the
new occurrence's input snapshot differs from the prior one by a `logic:SnapshotDelta`, which is
content-addressed, order-independent, and carries additions and suppressions. The same delta construct
carries a syllabus revision between two curriculum terms and a recipe change between two SCADA batches
— the cluster binding adds no machinery for it.

## Guidance versioning

Guidance is the accumulated *how we do this here*: rubrics, checklists, worked precedents, standing
cautions. It is the asset that makes a continuing cluster worth continuing, and it is the asset most
often modelled as an editable blob — at which point the record of *which guidance governed a past
decision* is destroyed the first time someone improves the wording.

`gmeow:GuidanceSet` is therefore versioned through the existing `gmeow:VersionMembership` apparatus
rather than through a bespoke version field, so a guidance lineage is a first-class version set with
authority and role, exactly as any other versioned artifact in the ontology. `gmeow:guidanceRubric`
ranges over `gmeow:Rubric` and `gmeow:guidanceChecklist` over its checklist carrier, both from the
norms vocabulary — the slice mints no second rubric or criterion model.

Two properties are load-bearing:

- **Exactly one guidance version governs an enactment.** Recorded through the enactment's context
  assembly and gated by `gmeow:GuidanceVersionRecordedConstraint`. An enactment whose governing
  guidance version is unrecorded cannot be reviewed after the fact, because the reviewer cannot know
  what the actor was told.
- **Improving guidance never rewrites history.** A new version supersedes; the prior version remains
  resolvable and remains the answer to "what governed that decision". This is the ordinary
  supersession-not-erasure discipline, applied where it is most tempting to skip.

Guidance is **advice, not authority**. A checklist item is not an approval and a rubric is not a
policy. The slice does not mint a second authority-separation mechanism for this: preference already
ships a consumer improvement gate whose promotion-is-not-activation and activation-authority
constraints say exactly this, and that gate is **generalized** rather than duplicated. Minting a
parallel gate would be a second source of truth for the single most safety-relevant rule in the
surface.

## Context assembly

`logic:ContextAssembly` is the kernel's audit primitive: *what was put in front of the decider, from
where, under what budget, and what was excluded*. This slice binds it —
`gmeow:enactmentContextAssembly`, **exactly one per enactment**, ranging over `logic:ContextAssembly`
— and that binding is what turns the class from a producer with no consumer into the record every
review reads.

The obligation is symmetric, and the second half is the one that is always dropped: an assembly
records **inclusions and exclusions**. A record of what was shown, with no record of what was
withheld, cannot distinguish a decision made in full knowledge from one made under a truncation
budget that silently dropped the contradicting note. Recorded exclusions are what make dissent
meaningful — a dissenting observation that was assembled and overridden is a different fact from one
that was never surfaced, and only the first is a judgment.

Concretely, for a work cluster the assembly answers, per enactment: which retained notes were
surfaced, which guidance version governed, which prior enactments were offered for comparison, which
candidate actions were in the frontier at decision time, what the assembly budget was, and what fell
outside it. Because the class is domain-neutral, the identical record serves an agentic tool-calling
trace, a curriculum's *what the learner had access to*, and a SCADA operator's HMI content at the
moment of a manual intervention.

## Recurrence — the schedule generates, the enactment is generated

Recurrence reuses the calendar and events vocabularies verbatim and mints nothing. The chain is:

```text
gmeow:WorkCluster —clusterSchedule→ gmeow:EventSchedule
                                      ├─ scheduleTemplateEvent → gmeow:Event
                                      ├─ scheduleRecurrenceRule → gmeow:RecurrenceRule
                                      └─ scheduleOccurrence → gmeow:Event      (the occurrence)
                                                                    │
                                                              generates
                                                                    ↓
                                                            logic:Enactment
```

The cluster's occurrence series is a `gmeow:EventSeries` reached through `gmeow:hasRecurrenceRule` /
`gmeow:seriesOccurrence`. `gmeow:EventSchedule` is the **mediator** — the generator that binds the
template event, the rules, the time zone, and the generated occurrences — and reusing it is what keeps
recurrence semantics (time zones, transitions, exceptions) in one place. A local recurrence rule
minted in this slice would be a second source of truth for calendar semantics; the structural cell
`saRecurrenceReusesCalendar` forbids it, with a fail witness that mints one.

**An enactment is not an occurrence.** `gmeow:EnactmentIsNotOccurrenceConstraint` forbids identifying
them, and the reasons are concrete rather than doctrinal:

- One occurrence may be enacted, paused, and resumed — the occurrence did not happen twice.
- A deliberately unenacted occurrence is a `gmeow:ScheduleException`, a positive recorded fact, **not
  a missing enactment**. The difference between "we chose to skip the holiday week" and "the record is
  incomplete" is the difference between a trustworthy audit and a suspicious one.
- An occurrence is placed in time by the calendar; an enactment is placed in the journal by its
  transitions. A daylight-saving transition inside an approval window is a property of the first and
  is invisible to the second.

## Decision support — candidates, contexts, and hard failure

The derived actionable frontier is kernel-owned and standpoint-indexed: "requires approval" is a claim
*under a policy*, and two governing policies may legitimately disagree, so classifications coexist as
attributed observations rather than one being elected. This slice binds the frontier into the existing
preference apparatus and mints no second recommendation model:

- **`gmeow:frontierCandidateSet` → `gmeow:CandidateSet`.** The actionable frontier's candidates *are*
  preference candidates; ranking, ties, and incomparability come from the preference vocabulary
  already shipped.
- **`gmeow:frontierComparisonContext` → `gmeow:ComparisonContext`.** A recommendation is always
  relative to a comparison context. Ties and incomparability are preserved as such — never broken by
  an arbitrary total order, and never resolved into a fabricated universal winner. Across concurrent
  enactments and vantages, a global recommendation is legitimate only when the local frontiers glue
  with the obstruction discharged, which is the shipped no-global-winner-without-consensus law and not
  a new one.
- **Blocked labels bind the hard-failure apparatus.** A blocked action is not a low-ranked one. The
  verifier-verdict / hard-constraint-class / hard-failure-not-overridden triad already distinguishes
  "scored poorly" from "failed a hard constraint and may not be overridden by a good score elsewhere",
  which is exactly what a blocked frontier label means.

**Risk severity rides `gmeow:Criterion`.** Cost, risk, and benefit are criterion values on the
explanation, quantified in `math:`. The slice does not reach for a separate risk vocabulary: a core
slice may not reach an extension at all, and more importantly, promoting a whole slice to type one
explanation field is core bloat. What the review surface actually needs is a severity *value* on a
criterion, and that is what it gets.

## Approval, contribution, and attribution

Approval is kernel-side as a social commitment with a detachment lifecycle and a checkable
authorization proof. This slice binds only what is domain-typed:
`gmeow:approvalGoverningNorm` → `gmeow:Norm` and the authority level → `gmeow:AuthorityLevel`. The six
exactly-one bindings the commitment carries — the exact dispatch-intent digest, the enactment/step,
the authorized operator identity, the governing policy, the decision, and the validity window — are
kernel obligations and are not restated here.

**Every recorded contribution is attributed.** `gmeow:ContributionKind` is a closed three-value class —
model, operator, tool — with an exactly-one binding on every recorded contribution and a negative
fixture for an unattributed one (`gmeow:ContributionAttributionConstraint`). The reason it is closed
and mandatory rather than open and optional: an unattributed contribution in a review record is
indistinguishable from a human judgment, which is precisely the confusion that makes a
model-generated draft dangerous. Attribution is carried natively as statement-level metadata rather
than as a parallel reification, so an attributed claim is one triple term and not a hand-rolled
quad-shaped stand-in.

`gmeow:preparationActivity` → `gmeow:Activity`, a sub-property of the kernel's `logic:preparationOf`,
closes the other half of preparation: the kernel says *this step prepares for that one*; the binding
says *this `gmeow:Activity` is that preparation*, so preparation work joins the ordinary activity and
provenance spine instead of living in a parallel universe.

## The operator review projection and its preservation judgment

`gmeow:WorkReviewProjection` is the comprehensive operator surface, and its completeness is gated
clause by clause (`gmeow:ReviewProjectionCompletenessConstraint`), one clause per enumerated content:

1. the continuing goal and its recurrence;
2. the current prescription and its revision lineage;
3. the current enactment and its exact input snapshot;
4. the derived frontier with recommendations and blockers;
5. approvals and pending inputs;
6. effect, reconciliation, and compensation status;
7. accumulated notes, decisions, artifacts, receipts, and prior-enactment comparison;
8. an exact accessible graph and timeline representation.

Content 4 is joined to `gmeow:frontierExplanation`, which carries, per recommended or blocked action:
the **proof term** (not a citation), the evidence, the governing policy, the cost/risk/benefit
criteria as `gmeow:Criterion` values, and **at least one dissenting attributed observation where one
exists**. An explanation that silently omits dissent is an advocacy document, not an explanation.

### The Principle-17 preservation judgment

The review projection is a **projection**, and Principle 17 requires every projection to declare what
it guarantees rather than merely what it omits. Every instance carries a `gmeow:ProjectionReceipt`
bearing **exactly one** `logic:preservationKind` (`gmeow:ProjectionReceiptRequiredConstraint`), and
for any non-exact kind a **named** `logic:expressivenessBoundary` plus a disclosed-loss entry.

The judgment for this projection is **sound under approximation**, and the reasoning is worth stating
because the tempting answer is wrong:

- **It is sound.** Every action the projection displays is genuinely derivable from the graph, with
  the axis-tuple witness that produced its label and the proof term that authorizes it. Nothing is
  displayed that the reasoner did not derive; the surface cannot invent an action.
- **It is not exact, and the boundary is standpoint collapse.** The canonical record is
  standpoint-indexed: two governing policies may classify the same action differently, and both
  classifications coexist as attributed observations. A rendered timeline has one column per moment.
  The projection therefore renders under **exactly one** declared governing standpoint and demotes the
  others to recorded dissent — which is a genuine loss of the coequal structure, not a lossless
  reordering. Naming it is what stops a consumer from reading the rendered view as the whole truth.
- **It is not complete.** The projection shows the frontier under its declared standpoint and budget,
  not every action derivable under every standpoint. Claiming completeness would re-introduce, at the
  presentation layer, exactly the "incomplete roster presented as closed" defect the kernel's
  saturation witness eliminates at the derivation layer. Content 4 carries the frontier's saturation
  witness through to the surface for this reason: closedness is displayed as *certified* or not
  displayed as closed at all.

Claiming exact preservation here would be the single most consequential overclaim in the surface,
because the operator's trust in the projection is what the whole decision-support case rests on. The
receipt makes the claim explicit, mechanical, and gated by the overclaim check, so it cannot drift
into optimism.

An accessible representation (content 8) is a first-class obligation of the projection rather than a
rendering concern: the graph and timeline must be reachable without sight, which means the axis tuple,
the labels, and the explanation must be present as structure and not only as position on a canvas.

## Non-conflation and structural bans in this surface

The kernel's non-conflation rules apply here unchanged. This slice adds the bans that are specific to
a domain binding, each with a fail witness so that none is vacuous:

- **No second plan, run, or execution concept.** The prescription is a `logic:PrescriptionVersion`;
  the occurrence is a `logic:Enactment`. A hand-authored process-model surface beside them would be a
  second source of truth, and the canonical process model is single.
- **No second budget, frontier, recommendation model, or authority-separation gate.** Each already
  exists; each is bound, not re-minted.
- **No second status enumeration.** The seven axes are kernel-owned and closed.
- **No re-mint of a term this slice depends on.** Norm, criterion, rubric, note, candidate, comparison
  context, recurrence rule, event schedule, and version membership are reused from their owning
  slices.
- **No hand-authored node or property shapes.** Declarative checks are EL-safe axioms in
  `module.ttl`; procedural and cross-node checks are a `logic:Constraint` plus a `logic:Formula`. A
  hand-authored shape is a projection-purity failure.
- **The graph never commits an external effect.** No effect attempt and no external effect receipt may
  carry a derivation provenance IRI: those records are *observed*, and a derived one would mean the
  reasoner claimed something happened in the world.

## Grounding obligations

Every quantity in this surface grounds in `math:` and every textual surface grounds in `lang:`; a
quantitative slice with no `math:` reference is carrying an ungrounded number, and a textual surface
with no `lang:` reference is carrying prose the system cannot reason about.

- **`math:`** — budgets, deadline consumption, costs, severity levels, satisfaction degrees, and every
  cost/risk/benefit criterion ground in `math:Quantity` with an explicit dimension and quantity value.
  A structural cell asserts that **every** budget, cost, deadline, severity, and satisfaction-degree
  property carries a dimension, with a fail witness carrying a bare literal.
- **`lang:`** — versioned guidance, rubrics, checklists, retained notes, dissent prose, and
  model-generated summaries and drafts ground their form and denotation surfaces in `lang:`.

The floors are **absolute, not seeded**: maximal grounding at or above `0.90` and maximal linkage at
or above `0.75`, measured on the slice in isolation. Seeding a floor from the live measurement and
then asserting measurement ≥ floor is unfalsifiable by construction; a fixed threshold is checkable
against the slice as it stands and fails when the surface regresses.
