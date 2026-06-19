<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — Design Evolution and Canonical-Native Endgame

> Status: canonical design narrative for `logic:`. This is the **design-evolution** member of the
> [GMEOW Logic document set](LOGIC.md#the-document-set). It accounts for how the current design
> supersedes its predecessors, identifies the conceptual ordering that governs the design (the spec
> precedes the realization), and characterizes the canonical-native endgame toward which every
> aspect of the system converges. Formal semantics are in [LOGIC-SEMANTICS.md](LOGIC-SEMANTICS.md);
> the engine is in [LOGIC-RUNTIME.md](LOGIC-RUNTIME.md); the reasoning contract and facet algebra
> are in [LOGIC-CONTRACT.md](LOGIC-CONTRACT.md).

## How the design supersedes its predecessors

The `logic:` design did not emerge complete. It passed through a sequence of framings, each
adequate for its moment and each superseded — not abandoned — by the next. Understanding that
sequence is part of understanding why the present design has the shape it does.

### The first framing: a fixed profile set

The earliest framing treated reasoning configuration as a small enumerated list of named profiles:
Horn, stratified-negation, well-founded, stable-model, procedural, probabilistic. That framing was
practical — profiles map straightforwardly onto available reasoning engines — but it embedded a
category error. The dimensions along which a reasoning request varies (what entailment relation
holds, what negation means, what closed-world means, how modality is indexed, whether state
changes) are mostly orthogonal. Collapsing orthogonal dimensions into a flat list of named points
makes the unchosen combinations either inexpressible or silently approximated by the nearest named
point. Silent approximation is exactly what the projection doctrine forbids everywhere else.

The present design supersedes that framing with the **reasoning contract**: a typed selection of
values across independent facets, where named profiles survive only as presets — bundles of facet
values that the system expands before evaluation, not indivisible alternatives. The full facet
algebra and the compatibility matrix are specified in [LOGIC-CONTRACT.md](LOGIC-CONTRACT.md). No
facet combination is silently approximated; an unsupported combination yields `unsupported`, never
a quietly substituted nearby semantics.

### The second framing: a single generalized world

The initial materialization model treated the world as a single graph: one RDF dataset, one chase,
one consistent closure. That framing handled flat, attribution-free derivation, but it could not
represent standpoint-indexed claims, modal necessity and possibility, counterfactual states that
must not leak into the base world, or the kind of multi-world reasoning that foundational rigidity
requires. Named graphs as a bare storage mechanism do not supply semantics; syntax without a
defined entailment relation is not a logic.

The present design supersedes that framing with the **typed context algebra**: a structured space
of worlds, each world-local in its entailment scope; potentially internally inconsistent under a
paraconsistent contract, with a defined entailment relation indexed to context rather than assumed
global. Cross-world reasoning — rigidity, modal closure, counterfactual
revision — is an explicit closure pass over the finite materialized world set, not an implicit
union. The semantics of that algebra are given precisely in
[LOGIC-SEMANTICS.md](LOGIC-SEMANTICS.md#worlds-modality-and-counterfactuals--a-typed-context-algebra).

### The third framing: a Datalog/Horn-only intermediate form

An intermediate framing treated the typed intermediate representation as essentially a Datalog or
Horn-clause normal form with OWL constructs bolted on. That was a coherent narrowing for the
materialization strata but conflated the IR with one of its projections. Datalog cannot express
existential rules (value invention), classical negation, backward goal resolution, or any of the
contextual/modal/probabilistic scopes that `logic:` requires. If the IR is secretly Datalog, those
scopes must either be encoded away — hiding their semantics in the representation — or refused.

The present design supersedes that framing with the **typed full-FOL IR**: a representation that
captures any `logic:` source construct faithfully, records per-construct preservation judgments,
and drives projection rather than being produced by it. Datalog materialization, Horn reasoning,
OWL projection, and N3/SHACL/Prolog targets are all generated from this IR; none is the IR.
The preservation semantics are specified in [LOGIC-SEMANTICS.md](LOGIC-SEMANTICS.md); the IR
structure and per-lowering loss judgments are in [LOGIC-IR.md](LOGIC-IR.md).

### The fourth framing: an externally-hosted secondary oracle

An earlier authority model operated a separate secondary oracle alongside the native solver: that
secondary implementation was the executable specification for materialization strata, and the
native solver was required to agree with it over a shared corpus. That model served a real purpose — it forced
explicit cross-verification and made the specification independent of the implementation — and the
cross-verification discipline it established is preserved in the conformance corpus structure.
What is superseded is the claim that the oracle is **permanent**: as the native solver acquires
derivation-graph provenance, the secondary oracle's role as authority for each discipline is
superseded discipline-by-discipline, formalized by derivation-graph golden fixtures that make
correctness concrete and engine-independent. The foundational lowering oracle, for instance, has
already been retired; the native derivation graph is the specification for that discipline. The
conformance contract and preservation polarity per projection are in
[LOGIC-CONFORMANCE.md](LOGIC-CONFORMANCE.md).

## Design-first ordering

The design is authored as the specification; the realization conforms to it. That ordering is not
incidental — it is the same canonical-source doctrine applied to logic itself. The `logic:`
vocabulary, the reasoning contract, the IR, and the conformance corpus are the canonical form; the
solver is the realization. A realization that passes the corpus inherits the design's guarantees;
one that diverges from it is wrong, not the design.

This ordering has a direct implication for the OWL-era encodings. `owl:*` and `gufo:` axioms were
originally authored directly and treated as canonical. In the endgame they are generated
projections of `logic:` source — exactly as OWL DL output is a projection in any other GMEOW
slice. The migration from hand-authored OWL to generated OWL is complete when every legacy axiom
is traceable to a `logic:` source construct and the projection round-trips identically; until then,
the adapter mechanisms (the `owl:*` normalizer and the `gufo:` projection generator) preserve
equivalence while the source-of-truth boundary moves. A construct authored in both forms must
normalize identically or the build fails; neither form is a second source of truth during the
transition.

## Current-form to target-form reconciliation

The `logic:` design targets a canonical form that differs in specific ways from the vocabulary
currently implemented in the ontology. This table makes those differences explicit so the gap is
documented as a design intention, not an oversight. Each row names the conceptual area, describes
the current-form treatment (what is in the ontology now), and states the target-form treatment
(what the canonical design requires). The columns do not indicate which is "better" — the target
column is the direction of travel, and the current column is the honest starting point.

| Conceptual area | Current form | Target form |
| --- | --- | --- |
| **Unindexed statement** | A statement asserted without a context index is treated as held according to the universal standpoint — it implicitly propagates as universally true. | Holds in `logic:UnspecifiedStandpoint`: it is unspecified, not universal. Only an explicit assertion indexed to `gmeow:universalStandpoint`, or one that explicitly quantifies over all contexts of a stated type, propagates as universal. Silence about where a statement holds licenses no propagation. |
| **Claim modality** | A single five-valued standpoint-modality (`unequivocal`, `conceivable`, `refuted`, `probable`, `bullshit`) is treated as canonical; absence of an explicit value implies unequivocal. | Factored into six orthogonal axes — **polarity** (affirm / deny / suspend), **modal force** (necessary / actual / possible / counterfactual), **credence** (graded degree of belief, distinct from probability and confidence), **assertoric force** (assert / conjecture / assume / retract), **truth-directedness** (truth-aimed / truth-indifferent / strategic), and **support-status** (supported / defeated / undetermined). The five-valued surface is a generated convenience view over these axes, produced for consumers that require a single token; it is never the canonical record. |
| **Credence vs. confidence** | The current epistemics vocabulary correctly distinguishes the two: `credence` is a degree of belief held by an agent about a content; `confidence` is a confidence score attached to a statement or report (a source-level annotation, not an agent's doxastic state). They are not interchangeable. | The target design preserves this distinction. Credence and confidence must not be folded onto a single "confidence axis." In particular, the modal-force factoring for `credence` (see row above) refers specifically to the agent's graded doxastic commitment, which is orthogonal to `logic:confidence` as a source-quality annotation and orthogonal to `logic:probability` as a probabilistic-model quantity. Any design wording that treats credence as merely a synonym for confidence on the same axis is incorrect and should be treated as an editorial error in that document. |
| **Common Logic dialects** | Described as generated outputs — as projections of the logic engine, not authoring sources. Ingest direction not addressed. | Common Logic (CLIF, CGIF, XCL) is an **interchange surface**: generated from the canonical IR for emission, and may be ingested back into the IR for import. It is never the canonical authoring source for GMEOW itself. The round-trip faithfulness gate in [LOGIC-CONFORMANCE.md](LOGIC-CONFORMANCE.md#common-logic-round-trip-as-a-faithfulness-gate) applies in both directions. |
| **Context consistency** | Contexts described as "world-locally consistent" — implying each context is internally consistent by design. | A context is **world-local in its entailment scope** (it entails only what its own rules and named accessibility relations license) and is potentially internally inconsistent under a paraconsistent contract. A witnessed contradiction is contained and surfaced as `information = both`; it is not a global inconsistency and is not silently repaired. "Consistent" is the wrong predicate for a context. |

## The canonical-native endgame

The endgame is a single coherent state toward which every aspect of the design converges. It is
not "OWL, but faster," and it is not "more profiles." It is:

**One canonical full-FOL logic.** `logic:` is the sole authoring language for axioms, rules, and
the foundational ontology. It is Turing-complete, RDF 1.2-native, and expresses every reasoning
mode GMEOW requires — description logic, logic programming, defeasible defaults, modal necessity,
contextual scope, probabilistic inference, paraconsistency, and metalevel reasoning — as a single,
coherent semantic system, not as a patchwork of separately-managed sublanguages.

**Legacy encodings become generated projections.** OWL 2 DL/EL, Datalog, SHACL, N3, Prolog,
and SPARQL are generated outputs of the logic engine, not authoring surfaces. The Common Logic
dialects (CLIF, CGIF, XCL) are an interchange surface: they are generated from the canonical IR
and may be ingested into that IR, but Common Logic is never the canonical authoring source for
GMEOW itself. gUFO, the OWL realization of UFO⁺, is the primary down-projection of the canonical
foundational theory; BFO, DOLCE, and SUMO are generated bridge views carrying their own
ontological commitments, documented as such in the loss ledger. None of these is an authoring
surface in the endgame.

**One canonical native solver.** A single solver runs forward and backward chaining over the
canonical IR; it is the normal authority for derivation, explanation, and query. Classical OWL
reasoners (operating over the OWL projection) validate the OWL fragment; they are secondary
validators, not authorities, and their removal from the required path is completed once a
projection cross-check confirms that native reasoning is equivalent over the fragment they cover.

**Discipline expressed as axioms, not external enforcement.** The OntoUML disciplines that once
lived as external lint checks — stereotype cardinality, identity overlap, anti-rigidity, relator
mediation, cross-world rigidity — are lowered into `logic:` rules and evaluated natively. The
native derivation reproduces the lint verdicts exactly; the lint becomes a regression specification,
not the enforcement mechanism. Anti-rigidity's witness obligation (the requirement that a world
exist where the instance lacks the type) belongs to Stratum C counterfactual construction;
in-world rigidity is evaluated by a bounded closure pass over the materialized world set.

**Projection loss visible and machine-readable.** Every weakening incurred in producing a
compatibility artifact is recorded in the preservation ledger with its polarity — what is lost,
in which direction, under which contract. No projection overclaims soundness or completeness.

The endgame is the same doctrine GMEOW applies to facts, applied to axioms and rules: author once
in the maximal canonical form, project to tractable surfaces for each consumer, make every loss
explicit and tested, and never promote a compatibility format above the canonical source.

## Foundation projection and discipline

UFO⁺ is authored canonically in `logic:`; the upper ontologies are generated, and they are not
all the same kind of projection.

**gUFO** is the primary generated down-projection of UFO⁺ — the OWL realization of the same UFO
lineage, truth-preserving for the fragment OWL can express, validated by running the full set of
OntoUML anti-pattern checks over the downcast. The downcast must satisfy all five disciplines:
stereotype cardinality, identity overlap, anti-rigidity, free-role integrity, and relator
mediation.

**BFO, DOLCE, and SUMO** are generated alignment/bridge views, not truth-preserving projections,
unless a specific subfragment is certified as such in the loss ledger. They carry different
ontological commitments; the maximal-source doctrine respects that rather than claiming a shared
foundation. A bridge view is labelled in [LOGIC-CONFORMANCE.md](LOGIC-CONFORMANCE.md) so no
consumer mistakes it for a sound projection.

### Five in-world disciplines lowered; cross-world rigidity evaluated; witness-world construction

The lint-to-axiom move for the OntoUML disciplines is complete for the in-world and cross-world
cases. The native evaluator derives `logic:violation` facts reproducing, class-for-class, the
offending sets the original lint checks produce — five violation labels from four checks:
`logic:StereotypeCardinality`, `logic:MixIden`, `logic:FreeRole`, `logic:MixRig`, and
`logic:RelComp`. The lowering certifies under `logic:StratifiedNAFProfile`. The external lint
checks are now the regression specification of the lowering; the lowering is the enforcement
mechanism.

Cross-world rigidity — the world-spanning universal quantifier that cannot be expressed as an
ordinary in-world Datalog rule — is evaluated as a bounded closure pass over the finite
materialized world set, emitting `logic:rigidityViolation` quads in the world where rigidity
persistence fails. This pass fires only when at least two worlds are materialized.

Anti-rigidity's witness obligation — formally requiring a world of existence where the instance
lacks the type — belongs to counterfactual construction in Stratum C. The
`"anti_rigidity_policy"` profile field governs only the instance-level obligation facet until
witness-world construction is in place.

The operational semantics of the foundation are in
[LOGIC-SEMANTICS.md](LOGIC-SEMANTICS.md#operational-semantics-modality-and-identity-supply).

## Conformance and the preservation contract

The conformance corpus is the executable specification of the entire design. Every reasoning mode,
every projection, every discipline lowering, every profile, and every design claim in this
document is either covered by a corpus case or explicitly deferred. Correctness is proven
end-to-end by the corpus; an implementation that passes the corpus inherits the design's guarantees.

Preservation polarity per projection — what is lost, what is weakened, what is absent by design —
is recorded in [LOGIC-CONFORMANCE.md](LOGIC-CONFORMANCE.md). Loss ledger entries are not
admissions of failure; they are the formal record that the projection doctrine is being applied
honestly: no compatibility artifact claims more than it preserves.

## Implementation status

The design described in this document set spans three layers of readiness. Readers should
understand which layer a given feature occupies before treating any claim in these documents as
a commitment to currently-running behavior.

**Normative semantics.** The formal definitions and design contracts described in
[LOGIC-SEMANTICS.md](LOGIC-SEMANTICS.md), [LOGIC-CONTRACT.md](LOGIC-CONTRACT.md), and
[LOGIC-CONFORMANCE.md](LOGIC-CONFORMANCE.md) are normative regardless of what is implemented.
An implementation is correct to the extent it conforms to these documents; the documents are
not descriptive summaries of the implementation.

**Currently implemented subset.** The forward materialization stratum (Horn/stratified-NAF via
Nemo), the foundation lowering to the five OntoUML disciplines, cross-world rigidity evaluation,
native EL/DL reasoning, the conformance corpus structure, and the derivation-graph golden-fixture
mechanism are all running. The OWL projection cross-check and the SARIF/divergence-ledger pipeline
are operational. Common Logic emission (CLIF/CGIF/XCL) is generated from the IR.

**Required but not yet implemented.** Backward goal resolution beyond the `logic:ProceduralPrologProfile`
preset prototype, full Stratum C generative counterfactual construction (witness-world building for
anti-rigidity), probabilistic inference under the `Uncertainty = probabilistic` facet, and complete
ingestion of Common Logic into the canonical IR are all required by the design but not yet
operationally complete.

**Required but not yet specified.** Two normative layers are forthcoming and their absence from
this document set is a recorded gap, not an oversight:

- The **teleological goal and action layer** — the formal treatment of goals, plans, and
  intentional action in `logic:` — is a forthcoming normative extension. Consumers requiring
  goal-directed reasoning should treat its absence as a deferral, not as evidence that the design
  excludes it.
- The **multidimensional cognitive-assessment model** — factored evaluation of reasoning quality,
  reliability, calibration, and metacognitive posture across agents — is a forthcoming normative
  layer. It is architecturally anticipated by the credence/confidence/assertoric-force factoring
  but has not yet been specified as a stand-alone construct set.

## Open design tensions

These are genuine, standing conceptual tensions in the design — places where complexity can
re-accumulate or the design's claims could be falsified. They are named so the design remains
honest.

| Tension | Where it bites | What holds it |
| --- | --- | --- |
| The facet compatibility matrix is itself a complexity accumulator | Each new facet value multiplies the matrix; unsupported cells must be named, not quietly avoided | The cardinal rule — `unsupported` is always explicit — and the matrix being data, not control flow; new cells are additions, not rewrites ([contract](LOGIC-CONTRACT.md)) |
| Describing the target-as-shipped | This document describes an endgame; gap between description and realization can widen silently | The conformance corpus is the executable measure of the gap; every uncovered claim is a deferred corpus case, not a silent assumption |
| Facet orthogonality is asserted, not proven | Two facets declared independent may interact semantically in an uncharted region | The disjointness checks and the compatibility matrix surface violations; the `unsupported` verdict is the safety net |
| Foundation disciplines as native rules may diverge at scale | The native lowering reproduces lint verdicts on the current corpus; a novel ontology shape could expose a gap | The lint remains the regression specification; a divergence is a defect in the lowering, reported as a conformance failure |
| Anti-rigidity witness-world obligation is not fully evaluated | Until Stratum C constructs witness worlds, anti-rigidity is only partially enforced | The `anti_rigidity_policy` field makes the obligation explicit; the gap is named in the loss ledger ([semantics](LOGIC-SEMANTICS.md#anti-rigidity-needs-a-witness-policy)) |
| Cut changes declarative answers | Prolog preset | Cut is procedural-only, confined to the `logic:ProceduralPrologProfile` preset; loss recorded on projection ([semantics](LOGIC-SEMANTICS.md#cut-is-procedural-not-canonical)) |
| Confidence mistaken for probability | Weighted/ProbLog-style inference | Four separate predicates; probability only under the `logic:ProbabilisticProfile` preset (the `Uncertainty = probabilistic` facet) ([semantics](LOGIC-SEMANTICS.md#confidence-probability-weight-and-evidence)) |
| Named graph treated as modal semantics | Worlds | World-indexed entailment relation; no implicit dataset-union ([semantics](LOGIC-SEMANTICS.md#inconsistency-across-contexts-and-context-indexed-entailment)) |
| Triple term treated as both quote and assertion | Metalogic | A triple term names a proposition; assertion is via explicit predicates ([semantics](LOGIC-SEMANTICS.md#triple-terms-reifiers-and-assertion)) |
| Counterfactual revision ties explode | Stratum C | Declared entrenchment ordering; genuine tie yields `information = undetermined`, never branches ([semantics](LOGIC-SEMANTICS.md#deterministic-revision-taming-the-agm-mutation-explosion)) |
| Undecidability / non-termination | Canonical layer | Decidability is a projection/preset property; budget exhaustion yields `completeness = incomplete` / `information = undetermined` ([semantics](LOGIC-SEMANTICS.md#turing-completeness-decidability-and-termination)) |
| BFO/DOLCE overclaimed as truth-preserving | Foundation projection | Bridge views labelled as such; not truth-preserving unless certified per fragment ([conformance](LOGIC-CONFORMANCE.md)) |
| Stale materialization or counterfactual cache | World store | Content-hash-keyed graph snapshots ([runtime](LOGIC-RUNTIME.md#graph-versioning-and-staleness)) |
