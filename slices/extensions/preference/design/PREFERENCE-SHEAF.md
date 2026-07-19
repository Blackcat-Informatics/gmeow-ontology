<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# PREFERENCE-SHEAF — the design charter of the preference extension

*Manifesto voice. Declarative present tense is normative. Every hard rule maps to a gate.*

## Thesis

A preference is never automatically a global winner. It is a `gmeow:vantage`-indexed,
defeasible, proof-carrying claim. Conflicting evaluators coexist as co-equal cells;
no cell is privileged, and there is **no `preferredRank`, no context-free global
winner, no single slot to win** (Constitution Principle 9). Every derived ranking —
pairwise, listwise, DAG/hypergraph, Pareto, scalarized, and any optimizer view such
as DPO — is a **lossy projection**, disclosed and content-addressed, never the
canonical preference model.

The canonical object is a **vantage-indexed family of strict partial orders over a
candidate set**, compiled into a `math:CellComplex` and carried by a
`math:CellularSheaf`: candidates are 0-cells, preference/tie/incomparability edges are
1-cells, agreement among vantages is a 2-cell; a single evaluator's order is a
`math:LocalSection`. The whole apparatus makes Principle 9 **computable**:

> A global winner over a candidate set is legitimate **iff** the local orders glue
> into a `math:GlobalSection` (H⁰) over the **whole** base **and** the
> `math:GluingObstruction` (H¹) is discharged.

A Condorcet cycle is literally a non-vanishing H¹ class — equivalently, non-trivial
**holonomy** of the preference `math:connectionOfSheaf` (the sheaf's restriction maps
*are* a connection; a cycle whose transport does not close is curvature). Pareto
incomparability is a disconnected/void region of the same complex. This is the
isomorphic sibling of `semantic-topology`'s "a computed feature is never automatically
a global truth," built over the **same** shared `math:`/`logic:` grounding.

## Hard rules → gates

1. **No context-free global winner.** `gmeow:assertsGlobalWinner true` is legal only
   with a typed `gmeow:PreferenceClaim` that `extendsToConsensusSection` over the whole
   base and `dischargesDisagreement`.
   → `gmeow:NoGlobalWinnerWithoutConsensusConstraint` (`logic:Constraint`).
2. **Consensus reconciles ALL vantages, not just all candidates.** The consensus
   section must reconcile every `gmeow:PreferenceObservation` registered on the
   `gmeow:ComparisonContext` (closed-world over the context's vantages, topped at
   `gmeow:universalStandpoint`). A lone or cherry-picked section fails.
   → the whole-base `logic:ClosureEntry` clause of the same constraint; fail witnesses
   `single-evaluator-claims-universal.ttl`, `cherry-picked-consensus-omits-dissenter.ttl`.
3. **No hidden winner via a projection's top pick.** A `gmeow:selectedCandidate` bound
   at whole-`CandidateSet` scope is routed through the same consensus gate — it is
   projection-scoped and cannot ground a global assertion.
   → `gmeow:SelectedCandidateRequiresConsensusConstraint`; fail witness
   `projection-argmax-winner-no-consensus.ttl`.
4. **No `preferredRank` / `primary*`.** The slice mints no such term as any OWL
   class/property.
   → `gmeow:StructuralAssertion` `gmeow:mustNot` in `tests/structural.ttl`; fail witness
   `preferred-rank-reused.ttl`.
5. **Hard-constraint failure is never overridden by a soft score.** A candidate that
   hard-fails a verifier contract is unrankable-as-winner within the vantages under
   that contract; the hard and soft axes are structurally disjoint.
   → `gmeow:HardFailureNotOverriddenConstraint` (per-(candidate, contract), not global)
   + `gmeow:HardSoftDisjointnessConstraint`; fail witness `soft-overrides-hard.ttl`.
6. **A projection discloses every dropped relation and is content-addressed and
   deterministic.** For every canonical edge the projection cannot realize there is a
   `gmeow:DisclosedLossEntry`; a lossy receipt carries a non-exact `logic:preservationKind`;
   a deterministic receipt carries a `gmeow:tieBreakRule`, and same inputs ⇒ same
   `gmeow:contentDigest`.
   → `gmeow:CompleteLossDisclosureConstraint`, `gmeow:PreservationConsistencyConstraint`,
   `gmeow:DeterministicProjectionConstraint`, `gmeow:DeterministicDigestConstraint`,
   `gmeow:ScalarizationDisclosureConstraint`, `gmeow:ProjectionMustBeContentAddressed`.
7. **DPO is a derived consumer, not the canonical model.** A canonical
   `gmeow:PreferenceObservation` may not be derived from a `gmeow:DpoView`.
   → `gmeow:DpoNotCanonicalConstraint` (`logic:ForbiddenPatternConstraint`).
8. **A model/config delta binds its full identity tuple.** Base artifact, tokenizer,
   codebook, dataset projection, selection query, prompt, rubric, engine, optimizer,
   seed, and evaluation evidence are each mandatory; the delta's identity is the
   content digest over the whole tuple.
   → `gmeow:ModelConfigDeltaCompletenessConstraint`; one `model-delta-no-<leg>.ttl` fail
   witness per leg.
9. **Promotion grants no activation authority.** A promotion receipt carries no
   activation-granting edge; activation of a `model-serving:ModelDeployment` requires a
   `gmeow:ConsumerImprovementGate` verdict held — the consumer gate is authoritative.
   → `gmeow:PromotionNotActivationConstraint` + `gmeow:ActivationAuthorityConstraint`;
   fail witness `promotion-grants-activation.ttl`.
10. **Compose, never duplicate.** No term owned by norms, learning, AI, model-serving,
    provenance, evidence, or semantic-topology is redeclared here; each is reused by
    reference (`gmeow:TermEquivalence`) and at the instance level.
    → `tests/structural.ttl` `mustNot` bans + one `duplicate-<slice>-term.ttl` fail
    witness per named slice.

## DAG discipline (Principle 16 — inviolable)

Axioms (`rdfs:subClassOf`/`domain`/`range`/`subPropertyOf`) anchor **only** on core +
`logic`/`math` grounding. Norms, model-serving, semantic-topology, and
embedding-projection are extensions: reused **only** at the instance level and named
by-reference in `mappings/equivalences.ttl`. Where a typed range would come from an
extension class, it is typed to that class's **core superclass** (`Rubric ⊑ Norm,
SocialObject` → range `gmeow:SocialObject`; `Criterion ⊑ InformationObject`;
`ModelArtifact ⊑ InformationObject`). No property `rdfs:range`/`domain`/`subClassOf`
onto `embedding-projection:VectorSpaceContract` or `model-serving:ModelDeployment` —
those appear only as IRIs in example data or inside a `logic:Constraint`/SPARQL body.
The manifest `gmeow:sliceDependsOn` set is exactly `{core… , logic, math}`.

## Cell-scoped strict order (design decision, not a weakness)

`gmeow:preferredOver` is a strict partial order (irreflexive + asymmetric + transitive)
**within one `gmeow:PreferenceProjection` cell only**. Only its **irreflexivity** is a
global characteristic (`gmeow:preferredOverIrreflexivity`, a `logic:PropertyCharacteristicAssertion`
— safe, since nothing is strictly preferred to itself under any vantage); its **asymmetry
and transitivity** are enforced **cell-scoped** by `gmeow:VantageScopedStrictOrderConstraint`
(G3), **never** as global characteristics. It is *not* enforced globally: two evaluators
legitimately assert `preferredOver(A,B)` and `preferredOver(B,A)` (Principle 9 coexistence),
and human/aggregate judgments are legitimately cyclic across vantages — that is the H¹
obstruction, not an error. A global asymmetry marker would reject that legal conflict, and
a global transitivity marker would *conflate* vantages (composing one evaluator's `A≻B` with
another's `B≻C` into a spurious `A≻C`); cross-vantage judgments ride the reified,
vantage-indexed `gmeow:observationPrefers`, not bare edges. The
reified `gmeow:PreferenceObservation`/`gmeow:PreferenceClaim` is the cross-cell
coexistence carrier. Enforcing the strict-order characteristics globally would falsely
reject the required conflicts. Tie (`gmeow:preferentiallyEquivalentWith`) is symmetric
via the native `logic:symmetricProperty` marker, **never** `owl:SymmetricProperty`
(EL-illegal); incomparability (`gmeow:incomparableWith`) is a **positive** first-class
relation, distinct from "not yet compared" and from a tie.

## Honest boundaries (Principle 12 / Principle 17)

+ General k-criterion Pareto dominance and general acyclicity of a *projected* order
  are **second-order** (∀ over an open criterion set / a finiteness appeal); they are
  carried as `logic:expressivenessBoundary logic:SecondOrder ; logic:preservationKind
  logic:Unsupported`, never a faked first-order `logic:Formula`. Concrete, fixed-arity
  frontiers are enforced by bounded per-fixture Formulas.
+ Selection queries are stored, never executed (Principle 12); reproducibility of a
  projection is a digest-bound, disclosed claim. Fixture digest-consistency is the
  ontology's job (`DeterministicDigestConstraint`); bit-reproducibility of the
  recomputation is the consumer's.
+ A first-class `math:CliffordBundle`/`AssociatedBundle`/`SpinorBundle` object does
  **not** exist in `math:` grounding (only the prose "secondary reading" of
  `math:Connection`). An extension may not author grounding, so this slice expresses the
  Cl(12)-bundle-over-E8 structure **compositionally** (`math:CellularSheaf` with
  Clifford-module `math:SheafStalk`s over an E8-organized `math:CellComplex` +
  `math:connectionOfSheaf`) and the gap is **surfaced to the maintainer as a separate
  grounding follow-up issue** — it is a scope-ownership boundary, not a descope of any
  issue-#1525 acceptance criterion (none of which needs a named bundle object; the
  geometry is the consumer's substrate, referenced by content digest).
