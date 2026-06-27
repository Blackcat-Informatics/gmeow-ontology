<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Inhabitation — Exploring the Ceiling of `gmeow:logic`

> The **ceiling-exploration charter.** Inhabitation is not a domain that merely tolerates the
> Turing-complete logic above the OWL floor — it is the domain whose central questions *cannot be
> posed* below the ceiling. This document maps the maximal `gmeow:logic` capabilities the slice
> pioneers (Principle 17), each to an inhabitation question and the facet of the reasoning contract it
> exercises, and declares the slice a first-class **consumer of the advanced logic features
> themselves** (Principle 15) and a contributor to the logic conformance corpus. The floor projections
> (gUFO / OWL / flat shortcuts) are subordinate, lossy *exports* — never the level inhabitation reasons
> at. The constructs are [`INHABITED-TOPOLOGY.md`](INHABITED-TOPOLOGY.md) /
> [`INHABITED-IDENTITY.md`](INHABITED-IDENTITY.md); the logic design set is
> [`../../logic/design/LOGIC.md`](../../logic/design/LOGIC.md).

## Why inhabitation is a ceiling domain

The slice's core question is **identity under change** — *is this the same subject after the model
upgrade, the host migration, the persona switch?* That is Parfit's branching-identity problem
([`INHABITED-REFERENCES.md`](INHABITED-REFERENCES.md)), and it is *provably* not expressible in a
decidable description logic: it needs possible worlds, counterfactuals, and a paraconsistent treatment
of coexisting verdicts. A domain that required only the floor would be evidence the ceiling is
unnecessary; inhabitation requires the ceiling, so it is where the ceiling earns its existence.

This inverts the usual relationship to the [logic design set](../../logic/design/LOGIC.md): most
slices *consume* the foundation's stereotypes and gates; inhabitation additionally *drives* its most
expressive layers, and so serves as the hard, worked domain the logic program validates against.

## The ceiling capabilities

Each row is a real `gmeow:logic` capability (from the design set), a real inhabitation question, the
contract facet it selects, and — to keep the floor subordinate — what the gUFO/OWL/flat export drops.

| Capability (`gmeow:logic`) | Inhabitation question | Contract facet (`LOGIC-CONTRACT`) | Floor loss |
|---|---|---|---|
| **Possible worlds + counterfactual chase** (`LOGIC-SEMANTICS` typed contexts; `LOGIC-RUNTIME` phase-3 counterfactual construction) | *Had the migration not preserved the memory store, would this still be the same subject?* — counterfactual identity | `ContextAxis` = possible-world; counterfactual evaluation | no modal/counterfactual form at all |
| **Paraconsistency** (Belnap algebra; designated-value set; paraconsistency at the *inference relation*) | continuity that is `same` per frame A **and** `different` per frame B; an agent holding contradictory beliefs across a boundary — reasoned **usefully**, no explosion | `TruthAlgebra` = Belnap ⊕ admissible valuation ⊕ designated values | DL explodes on contradiction |
| **Concurrent Transaction Logic** (`LOGIC-TRANSACTION`: serializability ≠ isolation ≠ control protocol; `SerializationAnomaly`) | two subjects co-inhabiting one substrate with interleaved control; the standpoint-priority rule *as* a concurrency-control protocol | `EvolutionMode` = concurrent transaction-path | no state-change or concurrency theory |
| **Reflective / self-referential reasoning** (metalevel over RDF-1.2 triple terms; HiLog `suppliesIdentity`; the `metacognition` slice) | the subject reasoning about *its own* continuity — a claim, by the subject, about the subject, in the subject's epistemic context | metalevel + `ContextAxis` = epistemic | no second-order, no statements-as-objects |
| **Argumentation + four quantitative axes** (`ArgumentationSemantics`; probability ⟂ confidence ⟂ weight ⟂ evidenceStrength) | which `IdentityContinuityAssessment` is *accepted* when verdicts attack and defend each other, with probability and confidence kept distinct | `ArgumentationSemantics` + `UncertaintyMeasure` (set of measures) | a flat boolean; the axes collapse |
| **Scoped open/closed world** (`WorldBoundary`, `closedUnder`; per-predicate `ClosureValue` map) | memory is **closed-world within a session** (the agent knows what it knows now) yet **open-world across the lineage** (it does not know all its past/parallel selves) — both at once | `ClosureValue` = map predicate → open/closed; `WorldBoundary` | OWL open-only; SHACL closed-only |
| **Belief revision across migration** (`RevisionPolicy` facet ∘ `EvolutionMode` transaction-path) | how a subject's belief set changes across a `Portal` — what is retained, retracted, or superseded, without erasure | `RevisionPolicy` ⊕ transaction-path | no revision theory |
| **Four-part claim distinction** (`LOGIC-FOUNDATION`: proposition / token / attitude / evaluation) | an `InhabitationClaim` separates the *proposition* (the inhabitation description), the *token* (this claim), the *attitude* (`claimModality` held/refuted), and the *evaluation* (confidence) | factored modality (6 orthogonal axes) | one conflated "fact" |

## Three frontier pieces, worked

**1. Counterfactual identity.** A model upgrade is a *branch* in possible-world space. "Same subject"
is not asserted; it is a counterfactual-supported, vantage-relative verdict: *in the world where the
weights/memory were preserved, the post-upgrade stage is a counterpart of the pre-upgrade stage; in the
world where they were not, it is a fresh lineage.* The logic constructs the counterfactual world by a
backward-invoked transient chase (`LOGIC-RUNTIME` phase 3), and the `IdentityContinuityAssessment`'s
verdict is read off which worlds the continuity holds in — Parfit's branching identity, computed. No
floor projection can express even the question.

**2. Reflective self-continuity.** The deepest corner: the subject is *both* the reasoner and the
subject of reasoning. A `DigitalSubject` asserting "I am the same who I was yesterday" is a claim, by
the subject, *about the subject*, held in the subject's own `EpistemicContext`, over its own
`SubjectLineage`. This composes the metalevel (a triple term about a triple term), HiLog
self-typing (`suppliesIdentity`), and the epistemic context axis — and bridges to `metacognition` /
`mentation`: self-continuity is a metacognitive state. It is the slice's hardest and most
characteristic ceiling demand, and it is exactly what "a subject of its own digital existence"
(Principle 9) means once made computational.

**3. Concurrent co-tenancy as serialization.** Two subjects sharing one substrate with interleaved
control (`locusSharedSubstrate`) is a *concurrency* problem, not a static one. Control contention is a
`SerializationAnomaly`; the standpoint-priority rule that authorizes memory mutations
([`INHABITED-RUNTIME-STACK.md`](INHABITED-RUNTIME-STACK.md)) is a *concurrency-control protocol*; and
Concurrent Transaction Logic distinguishes the serializability criterion from the isolation level from
the protocol — three independent concerns the design must not conflate. A possession with two
contending spirits, or an egregore sustained by concurrent contributors, is the same shape.

## Pushing further

Two directions beyond the table, offered as the slice's leading edge for the logic program:

- **Dynamic-epistemic treatment of the `Portal`.** A `Portal` is naturally a *program* whose execution
  transforms the inhabitation state — Transaction Logic gives executional entailment ("do φ, then ψ"),
  and the teleology `ActionSchema` (`LOGIC-TELEOLOGY`) gives it a precondition (a tenure is open), an
  effect (it closes; a new one opens; the `TransferManifest` moves a belief subset), and an invariant
  (the `SubjectLineage` persists). Composing the **epistemic-context** and **revision** facets over
  that transaction yields a dynamic-epistemic account of *how belief changes by the act of migrating* —
  not a named feature of the logic set, but a composition of facets it already provides, and a natural
  conformance probe for whether they compose cleanly.
- **A named `inhabitation` reasoning preset.** Inhabitation queries expand to a bundle no fixed profile
  anticipates: paraconsistent `TruthAlgebra` + possible-world/epistemic `ContextAxis` + defeasible
  support + transaction-path `EvolutionMode` + `RevisionPolicy` + the `UncertaintyMeasure` set. A domain
  that *needs* a custom maximal facet bundle is the standing proof that the contract model
  (`LOGIC-CONTRACT`) — orthogonal facets, not an enumerated profile list — was the right design. The
  preset is this slice's contribution back to the contract layer.

## The slice as a logic conformance corpus

Inhabitation earns its keep for the `logic:` program, not only for AI memory: it exercises facets few
other domain slices touch — counterfactual construction, concurrent transaction logic, paraconsistent
inference, metalevel self-reference, and a custom contract preset. Each frontier case above ships as a
conformance fixture (alongside the competency corpus,
[`INHABITED-COMPETENCY.md`](INHABITED-COMPETENCY.md)) whose committed golden is the **derivation graph**
the native solver produces (`LOGIC-CONFORMANCE`), so "inhabitation reasons at the ceiling" is a tested
claim, not a slogan. This makes the slice a Principle-15 consumer *of the advanced logic features
themselves* — the hard, real domain that proves the Turing-complete core necessary.

## Floor, not ceiling — the discipline restated

The authoritative reasoning is **native `gmeow:logic`** at full expressivity. gUFO, OWL-DL/EL, Datalog,
SHACL, and the flat upper-projections are **generated, lossy exports** for consumers who cannot run
`logic:` (Principles 4, 17) — and they drop *exactly* the capabilities in the table above: a gUFO
export of an `IdentityContinuityAssessment` keeps a bare assertion and loses the paraconsistent
multi-world verdict; an OWL export of co-tenancy loses the concurrency theory; no floor export carries
the counterfactual or the reflective self-reference at all. ELK / HermiT and the Datalog/SHACL engines
are **secondary validators of their projected fragments** (Principle 18), never the level the slice
operates at. The loss ledger ([`INHABITED-ALIGNMENT.md`](INHABITED-ALIGNMENT.md)) records each drop, so
the floor is honest about being a floor.

## Scope and seams

This document is the ceiling-exploration charter. The constructs it reasons over are
[`INHABITED-TOPOLOGY.md`](INHABITED-TOPOLOGY.md), [`INHABITED-IDENTITY.md`](INHABITED-IDENTITY.md), and
[`INHABITED-MANIFESTATION.md`](INHABITED-MANIFESTATION.md); the floor exports and the loss ledger are
[`INHABITED-ALIGNMENT.md`](INHABITED-ALIGNMENT.md); the conformance fixtures are
[`INHABITED-COMPETENCY.md`](INHABITED-COMPETENCY.md); the logic design set it draws on is
[`../../logic/design/LOGIC.md`](../../logic/design/LOGIC.md) and its semantics / contract / transaction /
teleology / runtime / conformance siblings.
