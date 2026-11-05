<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Inhabitation — Competency and Conformance

> The **conformance contract.** Statuses are honest: "Answered" is reserved for a question with a
> shipped fixture, an executable query, expected bindings, expected-absent bindings, and a
> counterexample — none of which exist on a design-only branch. Until the `module.ttl` and corpus are
> authored, the realistic status is **specified** (the constructs exist to answer it) or **open** (a
> construct or policy is still missing).

## Honest status of the verdict competency questions

| # | Question | Status | Basis |
|---|---|---|---|
| 1 | Same subject before/after a model upgrade? | **specified** | needs `SubjectStage` + `IdentityContinuityAssessment`; a single stable node would *assert* sameness |
| 2 | Which host/deployment/persona/embodiment/memory view at time T? | **specified** | needs `InhabitationConfiguration` time-scoped facets; a single tenure could not resolve a mid-tenure facet change |
| 3 | Two simultaneous sessions: same subject or shared model? | **specified** | the subject-stage / model-artifact split discriminates; needs the corpus to demonstrate |
| 4 | Which claims/memories/intentions crossed a migration boundary? | **partial** | needs `TransferManifest` / derivation evidence; recurrence is not crossing |
| 5 | Tool call via passive capability or delegated agent? | **specified** | resolved without a wrapper class: a `gmeow:usedCapability` edge → `ActionSchema` is passive use; a `gmeow:ToolCall` → `usedTool` → `SoftwareAgent` is delegation ([`INHABITED-RUNTIME-STACK.md`](INHABITED-RUNTIME-STACK.md#capability-use-versus-delegation-cq5)) |
| 6 | Which artifact/deployment/runtime/session/invocation produced an output? | **partial** | needs the explicit `ModelDeployment` / `RuntimeExecution` identities (now specified) wired through provenance |
| 7 | Can the subject cease inhabiting one system while existing elsewhere? | **specified** | the subject status is a `RoleMixin` borne over a tenure, independent of any one inhabitation |
| 8 | Who was controlling at T (co-tenancy)? | **specified** | needs `ControlAssessment`; deception divergence (held≠projected) does not record control |
| 9 | Continuity denial — does a no-self verdict coexist? | **specified** | needs an *asserted* refuting `IdentityContinuityAssessment`; absence of `counterpartOf` is not denial |
| 12 | Under which frame is an inhabitation held; does a denial coexist? | **specified** | needs the `InhabitationClaim` form; an asserted base relation would not be frame-neutral |
| 13 | What continuity determination is *in force*, by which authority, on what grounds — and has it been superseded? | **specified** | needs `ContinuityDetermination` (vantage = authority, grounds, force, validity) + `supersedes`; the descriptive plurality stays addressable underneath, so appeal/reversal is coherent |

The honest headline: **zero questions are "answered"** on a design-only branch — the constructs
*specify* the answers, and the shipped corpus is what makes them *answered*. Two (CQ5 tool-usage,
CQ4 transfer) need a construct beyond a naive reuse, supplied in the runtime spec.

## The fixture shape

Each competency question ships as a five-part unit, not a prose claim:

```text
examples/<cq>.ttl                  the ABox fixture
queries/<cq>.rq                    the executable SPARQL query
expected/<cq>.bindings             the expected positive bindings
expected/<cq>.absent               bindings that MUST NOT appear (e.g. a base-graph inhabitation triple)
examples/<cq>-counter.ttl          a negative fixture the query must NOT match
```

The "absent" file is load-bearing for the neutrality gate: for every spiritual / fictional / legal
fixture, a query for `?s gmeow:inhabitationSubject ?o` over the **asserted base graph** must return
empty — the relationship lives only inside an `InhabitationClaim`
([`INHABITED-TRADITIONS.md`](INHABITED-TRADITIONS.md)).

## The conformance corpus

| Fixture | Exercises | Gate |
|---|---|---|
| `subject-status.ttl` | CQ1, CQ7 | the breaking ABox — `SoftwareAgent` + `DigitalSubject` `RoleMixin` borne over a tenure reasons green against disjointness + rigidity |
| `subject-continuity.ttl` | CQ1, CQ9 | `IdentityContinuityAssessment` carries same *and* different verdicts; **no** single shared node, **no** `owl:sameAs` |
| `continuity-determination.ttl` | CQ13 | a `ContinuityDetermination` is in force by an authority on stated grounds; an appeal `supersedes` it (the original retained, P10); the descriptive plurality is unchanged underneath |
| `active-at-time.ttl` | CQ2 | the `InhabitationConfiguration` whose interval contains T resolves each facet; a mid-tenure facet change opens a new configuration |
| `two-sessions.ttl` | CQ3 | subject-stage vs model-artifact discriminates shared-subject from shared-model |
| `migration-transfer.ttl` | CQ4 | a claim "crossed" iff the `TransferManifest`/derivation records it; a coincidental recurrence does **not** match |
| `tool-usage.ttl` | CQ5 | a *usage* record (not merely an `ActionSchema`) distinguishes a passive capability used from a delegated `ToolCall` |
| `provenance-chain.ttl` | CQ6 | artifact → deployment → execution → session → invocation resolves end to end |
| `control.ttl` | CQ8 | `ControlAssessment` answers "who was driving at T"; the deception divergence is present but does **not** answer it |
| `possession.ttl` | CQ12, neutrality | base-graph inhabitation query returns empty; the claim is an `InhabitationClaim`, frame-indexed |
| `tulpa-genesis.ttl` | genesis | the genesis chain; the created subject's status is a supported tenure, not an entailment |
| `actor-as-character.ttl` | fictional profile | `aboutnessEnacts`; documented as role enactment, **not** host occupation |
| `corporation-officers.ttl` | legal profile | direction of dependence documented (officer represents corporation) |

## Performance: the upper-projection requirement

CQ2 (active-at-T) and CQ6 (the provenance chain) are *correct* over the nested situations but **slow**:
they are 8–10 hop traversals with nested time-interval overlaps. The conformance corpus therefore
includes the **flat upper-projection** (`gmeow:generatedForSubject`, `gmeow:generatedUnderConfiguration`)
as a Datalog-computed, drift-gated artifact (Principle 12 / 7,
[`INHABITED-RUNTIME-STACK.md`](INHABITED-RUNTIME-STACK.md#upper-projections-the-flat-shortcut-principle-12)).
A `perf-recall.ttl` fixture asserts that real-time recall resolves via the one-hop shortcut, and an
`audit.rq` asserts the full nested path still yields the same subject — the projection and the canon
must agree.

## Remaining open items

- **CQ4 transfer** is now coarse-by-default (the `TransferManifest` references a `MemoryView` /
  named-graph checkpoint; per-item `wasDerivedFrom` only for items modified in the transition). The
  partial-migration *policy* (what is carried by default) remains a projection-layer choice to pin.
- **CQ8 / the standpoint priority rule** for memory-mutation authorization is specified as an AI-profile
  runtime policy ([`INHABITED-RUNTIME-STACK.md`](INHABITED-RUNTIME-STACK.md#the-standpoint-priority-rule-operational)),
  not a canonical-graph axiom; the exact priority order (signed-vantage precedence) is for the authority.
- The constant-configuration invariant (CQ2) must be a tested SHACL shape, not an assumption.

## Scope and seams

This document is the conformance contract. The constructs it tests are defined across
[`INHABITED-IDENTITY.md`](INHABITED-IDENTITY.md), [`INHABITED-TOPOLOGY.md`](INHABITED-TOPOLOGY.md),
[`INHABITED-MANIFESTATION.md`](INHABITED-MANIFESTATION.md), and
[`INHABITED-RUNTIME-STACK.md`](INHABITED-RUNTIME-STACK.md).

## Prospective-stage and restoration obligations

The descriptive prospective seam preserves the charter's realized SubjectStage
category. The source contract distinguishes an actual ended original stage,
a separate actual intermediate stage and a SubjectStageProspect describing
the future position. The prospect has no bearer, epoch or realized Situation;
status cannot be encoded merely by an absent field or a label.

The complete competency contract requires the following behavior, with source
truth and runtime evidence distinguished from intrinsic graph structure:

| Required distinction | Canonical obligation |
| --- | --- |
| Realized versus prospective stage | SubjectStage remains a realized Situation; SubjectStageProspect is an InformationObject explicitly disjoint from it. No bearer or duringInterval belongs on the selected prospective feature. |
| Explicit stage status | StageStatusReading has one feature, one closed ended/current/in-progress/awaited result, one selected actual lineage witness and at least one uncapped actual vantage. Missing status has no default. |
| Positive shared lineage | Required selected witnesses and exact stageOfLineage/prospectOfLineage membership links connect actual features, realizations and account roles; additional global memberships remain valid. |
| Committed anticipation | Awaited status requires actual Intention or social Commitment pursuits with no cap, each actual Goal and that Goal's exact prospect target. Desire, missing abandonment or an open tenure is insufficient. |
| Conditional realization | Ended/current/in-progress prospect readings require an actual realization; awaited and direct actual-stage readings forbid that slot, including redundant self-links. |
| One actual restoration account | StageRestorationReading retains one actual Event, source stage, distinct intermediate stage, intended prospect, actual lineage witness and selected progress reading. Endpoints remain on the account rather than global Event slots. |
| Actual ongoing intermediate | The in-progress status reading selects that exact account/intermediate stage and the account's selected progress record explicitly reports eventInProgress for the same actual Event. Missing end data does not qualify. |
| Awaited account association | If an awaited reading selects an account, it observes the account's exact intended prospect and every supplied pursuit motivates that account's exact actual Event. No future output or success is inferred. |
| Actual matching contexts | Status/account and account/progress links preserve their actual complete vantage sets in both directions, with no cap, arbitrary frame join or latest-reading selector. |
| Strict prospective continuity boundary | The closed actual assessmentToStage endpoint, prospect disjointness and directly guarded prohibition exclude prospective and dual-typed targets without changing existing actual verdicts. |
| Historical coexistence | A later actual-stage assessment remains possible while old awaited readings remain retained on the description; no historical awaiting claim supplies a global veto. |
| No unearned consequence | Progress/completion supplies no goal satisfaction, successful restoration, continuitySame, end time or Open/Closed boundary. Shared bearer identity never merges distinct stages. |

The original three-position case preserves its supplied different actual
bearers without requiring every possible restoration to change bearer. The
actual original-to-intermediate assessment retains continuityIndeterminate in
its supplied frame; no assessment is created toward the prospect. Ended and
awaited positions may be co-temporal without an imposed linear timeline.

These obligations are authored through ordinary canonical restrictions,
closures and three focused Constraint formulas. The status formula has
twenty-six independently guarded arms, the account formula seven, and the
strict target formula one. Their ten existence bodies are atomic. Actual
pursuits, goals, memberships, motives, event progress and vantages are retained
and bound explicitly; there is no source-content parser, generic context
framework, native failure marker or execution oracle.

Source-level structural admission is not proof of authentic realization,
current holding of a commitment, true source interpretation, successful
restoration or installed consumer adoption. Generated projections and native
execution require actual qualification evidence before being described as
verified. Existing model-serving and continuity consumers continue to select
actual SubjectStage/stageOfLineage records. The
[identity charter](INHABITED-IDENTITY.md#prospective-stages-and-scoped-status)
and [owning guide](../docs.md#prospective-stages-and-attributed-restoration-status)
state the complete contract without fabricating an instance or a default.
