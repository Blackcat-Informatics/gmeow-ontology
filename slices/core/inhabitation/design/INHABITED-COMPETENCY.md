<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Inhabitation — Competency and Conformance

> The **conformance contract**, revised after the foundational review
> ([`INHABITED-REVIEW.md`](INHABITED-REVIEW.md)). The first draft marked questions "Answered" that the
> proposed graph could not yet answer; the statuses are downgraded to honest values here. "Answered"
> is reserved for a question with a shipped fixture, an executable query, expected bindings, expected-
> absent bindings, and a counterexample — none of which exist on a design-only branch. Until the
> `module.ttl` and corpus are authored, the realistic status is **specified** (the constructs exist to
> answer it) or **open** (a construct or policy is still missing).

## Honest status of the verdict competency questions

| # | Question | Status | Basis |
|---|---|---|---|
| 1 | Same subject before/after a model upgrade? | **specified** (was "answered") | needs `SubjectStage` + `IdentityContinuityAssessment`; a single stable node would have *asserted* sameness, so the first draft's "answered" was wrong |
| 2 | Which host/deployment/persona/embodiment/memory view at time T? | **specified** (was "answered") | needs `InhabitationConfiguration` time-scoped facets; a single tenure could not resolve a mid-tenure facet change |
| 3 | Two simultaneous sessions: same subject or shared model? | **specified** | the subject-stage / model-artifact split discriminates; needs the corpus to demonstrate |
| 4 | Which claims/memories/intentions crossed a migration boundary? | **partial** | needs `TransferManifest` / derivation evidence; recurrence is not crossing |
| 5 | Tool call via passive capability or delegated agent? | **specified** (was "open") | resolved without a wrapper class: a `gmeow:usedCapability` edge → `ActionSchema` is passive use; a `gmeow:ToolCall` → `usedTool` → `SoftwareAgent` is delegation ([`INHABITED-RUNTIME-STACK.md`](INHABITED-RUNTIME-STACK.md#capability-use-versus-delegation-cq5)) |
| 6 | Which artifact/deployment/runtime/session/invocation produced an output? | **partial** | needs the explicit `ModelDeployment` / `RuntimeExecution` identities (now specified) wired through provenance |
| 7 | Can the subject cease inhabiting one system while existing elsewhere? | **specified** | the subject status is a `RoleMixin` borne over a tenure, independent of any one inhabitation |
| 8 | Who was controlling at T (co-tenancy)? | **open → specified** | needs `ControlAssessment`; the first draft's deception-divergence reuse did not record control |
| 9 | Continuity denial — does a no-self verdict coexist? | **specified** (was "answered") | needs an *asserted* refuting `IdentityContinuityAssessment`; absence of `counterpartOf` is not denial |
| 12 | Under which frame is an inhabitation held; does a denial coexist? | **specified** (was "answered") | needs the `InhabitationClaim` form; the first draft asserted the base relation |

The honest headline: **zero questions are "answered"** on a design-only branch, several were
mis-marked, and two (CQ5 tool-usage, CQ4 transfer) needed a construct the first draft lacked. The
revised constructs make all of them *specified*; the corpus makes them *answered*.

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
[`INHABITED-RUNTIME-STACK.md`](INHABITED-RUNTIME-STACK.md); the review disposition is
[`INHABITED-REVIEW.md`](INHABITED-REVIEW.md).
