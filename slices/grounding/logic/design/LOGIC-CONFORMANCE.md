<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — Conformance Design

> The **conformance** member
> of the [GMEOW Logic document set](LOGIC.md#the-document-set). It defines how correctness is
> established: the conformance corpus as the enforcement contract, capability-relative conformance
> and the capability manifest, the loss ledger and multidimensional preservation claims, the two
> orthogonal correctness axes, the Common-Logic round-trip faithfulness gate, the divergence ledger
> as a public benchmark surface, the design of tests-as-ontology-data and its isolation rule, and
> the coherence certificate for paraconsistent systems. The engine under test is described in
> [`LOGIC-RUNTIME.md`](LOGIC-RUNTIME.md); the semantics it must satisfy are in
> [`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md); the typed IR whose preservation each projection
> reports is specified in [`LOGIC-IR.md`](LOGIC-IR.md).

## The conformance corpus as contract

Conformance is established by a **shared, language-neutral corpus of cases**, where each case is a
static pair of input artifact and expected output. No implementation may substitute its own derived
assertions for the corpus; it must produce outputs that agree with the corpus files. This is the same
design principle that governs the rest of GMEOW's conformance work: the corpus is the executable
specification of the hardest invariants, and any engine that drifts from them fails to conform.

A conformance case is the atomic unit:

- an **input** — a `logic:` source, optionally together with adapter vocabulary or a declared
  reasoning contract;
- a **contract** — the `logic:ReasoningContract` (as defined in
  [`LOGIC-CONTRACT.md`](LOGIC-CONTRACT.md)) under which the case is to be evaluated, including its
  model-semantics, negation, truth, and uncertainty facets, decidability class, and resource bound;
- **expected outputs** — derived quads, world-indexed verdicts, contradiction witnesses, projection
  artifacts, and goal or counterfactual answers, each in a canonical form against which an
  implementation's output is compared by graph isomorphism or canonical-sorted comparison.

Cases are organized by category. Each category tests a distinct design invariant:

| Category | Core invariant under test |
| --- | --- |
| Foundation | The OntoUML disciplines (rigidity, identity-supply, mediation) derive the same verdicts as the structural checks they replace over the generated downcast |
| Worlds-A | Contested claims coexist in distinct named context graphs without privileging either standpoint |
| Worlds-B | Type-level modal reasoning generates exactly zero token occurrences (the no-occurrence gate) |
| Worlds-C | A counterfactual construction does not leak into the base world graph; a genuine tie returns `information = undetermined` (no unique context selected), never a branch |
| Projections | Each generated projection matches its declared preservation claim and decidability class |
| Decidability | A contract that falls within a certified fragment receives a `complete-for-fragment` result; a violating one is flagged |
| Reasoning semantics | Answers under each model-semantics value — and its composed negation, truth, and uncertainty facets — match the semantics declared by the contract |
| Explanation | Every IRI cited in a generated explanation appears in the proof trace — no justification outside the derivation |
| Paraconsistency | A cross-world contradiction is confined to separate context graphs; a within-world contradiction emits witnesses |
| Correspondence | Each `logic:Correspondence` lowering matches its declared preservation and rung; the get/put round-trip holds for iso/section claims; a mnemomorphic witness actually recovers the source; composition only weakens claims; no overclaim (see [Correspondence gates](#correspondence-gates)) |

**Comparison discipline.** No case may depend on iteration order. RDF outputs compare by graph
isomorphism; verdict and answer outputs compare as canonical sorted structures with normalized
literals; explanation outputs compare on the set of cited axiom and rule IRIs, never on surface prose.
The faithful-by-construction property for explanations (defined in
[`LOGIC-RUNTIME.md`](LOGIC-RUNTIME.md)) is enforced at the IRI skeleton, not at the wording.

## Capability-relative conformance

Not every implementation of `logic:` is a full runtime. A compiler-only surface, a validator-only
surface, a projection-only transcoder, a lightweight consumer, or a wasm-constrained deployment each
operates over a proper subset of the full capability space. Requiring every such implementation to
pass every conformance category would make conformance meaningless as a signal and would falsely
exclude legitimate specialised implementations.

Conformance is therefore **capability-relative**: a conforming implementation publishes a
**capability manifest** declaring exactly what it supports and what it does not. The manifest is a
machine-readable RDF document that asserts, for each recognized capability dimension, one of:

- `logic:conformsTo` — the implementation fully supports this capability and must pass all corpus
  cases that exercise it;
- `logic:unsupported` — the implementation explicitly does not support this capability; it must
  return `unsupported` for any input that requires it, and corpus cases for that capability are
  excluded from the conformance gate.

The recognized capability dimensions are a **versioned, explicitly enumerated set** — the *capability
registry*, identified by a registry version. They are not open-ended: a new dimension enters only by a
registry-version increment, precisely so that a certification issued against one version is never
retroactively invalidated by a later addition. At a given registry version the dimensions are, for
example: IR parsing, exact RDF 1.2 projection, structural validation, Horn forward-chase
materialization, stable-model semantics, backward-goal evaluation, generative counterfactuals,
transaction execution, and explanation generation.

A conforming implementation **must pass every mandatory corpus case within the capability set it
claims via `logic:conformsTo`**, and **must return `unsupported` for every case that requires a
capability it has listed as `logic:unsupported`**. Returning a result for a case whose required
capability is declared `unsupported` is a conformance failure; returning `unsupported` for a case
whose required capability is declared `conformsTo` is equally a failure.

**Full-runtime conformance** is a distinguished top-level certification, and it is always **relative to
a capability-registry version**: a full runtime declares `logic:conformsTo` for every capability
dimension *in registry version v*, passes all corpus categories for that version without any
`unsupported` exclusions, and is the only surface that may carry the full-runtime conformance label.
The certificate names the registry version *v* it was issued against. A later registry version that adds
a dimension does **not** retroactively invalidate a certificate issued against *v* — it defines a new,
higher bar that a fresh certification may target. A partial implementation that passes all cases within
its declared capability set is conforming at its surface level, but it is not a full runtime and may not
claim that label.

## The two orthogonal correctness axes

Conformance has two independent dimensions. They answer different questions and neither substitutes
for the other.

**Regression-stability (internal golden corpus).** An internal corpus authored by the
implementers alongside the engine documents what the engine *consistently produces* for a fixed set
of inputs under fixed contracts. When the engine changes, every case in this corpus is re-evaluated.
If a case that previously passed now fails, the change is a regression. Regression-stability means
the engine stays consistent with itself across versions.

This axis tells you nothing about whether the engine is correct with respect to the logic it claims
to implement. An engine that consistently produces wrong answers has perfect regression-stability.

**Correctness and soundness (external, independently-authored corpus).** An external corpus is
authored independently of any particular engine — by the `logic:` design, by the standards community,
or by domain experts — and represents ground truth: the answers a semantically correct engine *must*
produce for these inputs under these contracts. An engine passes this axis only when it agrees with
decisions the authors made without knowing how any engine is implemented.

This axis tells you nothing about regression between versions. A correct engine that is refactored
to produce the same correct answers produces a regression-stability signal of zero changes, but
the correctness axis is what licenses confidence in the correctness to begin with.

**Both axes are required.** Regression-stability without an external correctness corpus means the
project can only detect *change*, never establish *rightness*. An external corpus without internal
golden tracking means correctness can silently erode between releases. The two axes are
complementary; together they form the full conformance picture.

The design therefore requires both:

- **Internal regression goldens** — content-addressed derivation graphs pinned per version, checked
  on every modification;
- **External correctness corpus** — cases whose expected outputs are decided by the logic's
  community-agreed semantics, authored to be engine-agnostic, and periodically extended as the
  community sharpens its expectations.

When the two axes disagree — when an engine change passes the external corpus but breaks a regression
golden — the regression golden must be updated, with the update explicitly reviewed. When an engine
change passes the regression goldens but fails the external corpus, the engine has a correctness
defect that no amount of self-consistency can paper over.

### Retired backward-engine reference lane

The native backward engine is checked against versioned, captured SLD answer
goldens rather than a live embedded Prolog runtime. The backward benchmark cases
name `captured-sld-goldens/v1` as their independent reference, pin the complete
canonical answer digest in each `expected/result.json`, and require both the
`native` profile and that digest before the benchmark corpus accepts the case.
This preserves the independently decided SLD witness while keeping the retired
engine, its transitive dependency graph, and its process-global runtime lock out
of production and CI execution.

Cut and SLD-only n-ary arithmetic cases are not retained as executable positive
claims. Cut remains parseable solely so every profile returns a stable typed
retirement diagnostic; n-ary arithmetic reaches a typed unsupported-fragment
diagnostic. The positive cut case and SLD-only list-query fixtures were deleted.
The `docs/test-retention/` dossier rule does not apply because it governs living
pytest survivors, and this repository retains neither a pytest survivor nor an
embedded-SLD test. The captured answer corpus is the durable reference evidence.

**The external FOL soundness oracle.** For the full first-order fragment of the canonical IR, the
external correctness corpus is instantiated by problems drawn from the TPTP library, each carrying a
community-decided SZS status as engine-agnostic ground truth. A problem is parsed natively into the
full-FOL formula core, reduced by FOL-negation to a refutation question, and — for the
EL/DL-expressible fragment — lowered to a world-scoped OWL-RDF ABox/TBox and decided by the
DL-consistency clash machinery. The native verdict is projected to the coarse three-bucket runner
outcome only at the comparison gate; the raw SZS token is preserved verbatim as provenance (maximal
information flow), so `ContradictoryAxioms` stays distinct from `Unsatisfiable` and
`CounterSatisfiable` from `Satisfiable` in the ledger. A problem whose constructs fall outside the
decidable fragment — function symbols, existentials under universals, non-binary predicates,
genuinely disjunctive refutation — is recorded as an explicit capability-gap ledger row and is
**never** silently reported as decided: a capability gap is a hard fail, not an `incomplete`
swallowed into agreement, and a malformed source is a corpus defect, not a capability gap. The
resulting divergences (agreement, native-only, corpus-only, and capability-gap rows) fold into the
reasoned bundle as `gmeow:Finding` individuals, dogfooding the divergence ledger described below.
This gate validates the EL/DL-expressible fragment of the full-FOL IR against SZS ground truth; the
first-order-beyond-DL boundary is a declared, ledgered limitation, not an implicit gap.

**The external foundation-discipline soundness oracle.** For the OntoUML foundation disciplines, the
external correctness corpus is instantiated by models authored in the OntoUML metamodel vocabulary
(the serialization the FAIR OntoUML/UFO model catalog uses), each carrying a community-decided
anti-pattern verdict as engine-agnostic ground truth. A model is parsed natively, lowered to the
world-scoped stereotype ABox (stereotype puns, subclass edges, mediation roles), and decided by the
same native disciplines that run over the whole ontology. The fired discipline set is compared to the
model's documented anti-pattern; the specific label is preserved verbatim as provenance (maximal
information flow) and projected to the coarse pass/gap comparison only at the gate. A clean model that
fires any discipline is a soundness-breaking false positive (a hard fail); a documented anti-pattern
the native disciplines cannot reproduce — an out-of-fragment stereotype (a capability gap) or a
pattern no discipline checks (a coverage gap) — is recorded as an honest gap row, never a wrong
verdict, and feeds the harvested-axiom formalization backlog. Divergences fold into the reasoned
bundle as `gmeow:Finding` individuals. The OntoUML metamodel stereotype vocabulary is itself subsumed
by reference through the alignment stack (`skos:exactMatch` stereotype puns lowered to SSSOM/EDOAL),
dogfooding the catalog vocabulary as aligned individuals rather than a mere fixture set.

The native reader accepts both the FAIR catalog's own mediation serialization — relation ends as
`ontouml:Property` nodes whose functionality is read from `ontouml:cardinality`/`ontouml:upperBound`
(a mediated end of upper bound 1 is the RelComp shape) — and the self-authored
`ontouml:functionalMediation` convenience form; the two are covered by sibling Lane-A cases. The
documented anti-pattern label travels on the carrier the case type affords: a graded Lane-A case
carries it as `documented_antipattern` in `profile.json`, while a source-only `-divergence` case
(which has no verdict to freeze) carries it in a `# documented-antipattern:` model-header comment. Both
are the same verbatim provenance; only the carrier differs.

## Common-Logic round-trip as a faithfulness gate

One projection the canonical IR supports is emission to a Common Logic dialect. One ingestion path
the canonical IR supports is parsing from a Common Logic source. Together these two paths define a
**faithfulness gate**:

> A program is written in `logic:`, compiled to the canonical IR, emitted to a Common Logic dialect,
> then re-ingested. The re-ingested program must compile to the **same canonical IR** as the original.

This gate tests that the CL emission is not lossy in the round-trip direction: the emitted artifact
carries enough information that nothing is destroyed by the transit. It does not require that the CL
surface is the canonical form — CL is one projection, and a `SoundUnderApproximation` projection is
allowed to drop things. What it requires is that, *for constructs the CL projection declares it
preserves exactly*, the round-trip is an identity at the IR level.

The canonical IR's identity is content-addressed and alpha-normalized (as specified in
[`LOGIC-IR.md`](LOGIC-IR.md)). Two programs with the same canonical IR are the same program; the
round-trip gate therefore reduces to: *does the re-ingested program's canonical form match the
original's?* This is a decidable graph-isomorphism check, not a semantic-equivalence search.

The gate applies per-construct class. A program that uses only the `ExactPreservation` subset of the
IR must round-trip perfectly through CL. A program that uses constructs the CL projection marks as
`SoundUnderApproximation` or `unsupported` is checked only on the constructs the projection does
preserve; the gate does not require that lossy constructs survive. The preservation claim (defined
in the section below) governs which constructs are in scope for the round-trip check.

## Correspondence gates

The correspondence calculus ([`LOGIC-CORRESPONDENCE.md`](LOGIC-CORRESPONDENCE.md)) **generalizes the
Common-Logic round-trip gate above to any `logic:Correspondence`.** Five gates govern the
Correspondence corpus category, each a decidable check over the content-addressed canonical IR
(graph-isomorphism, not semantic-equivalence search):

- **Law gate** — a correspondence may not claim a law (GetPut / PutGet / PutPut / the section law) it
  fails; an `ObligationDischarged` verdict under `logic:DischargeCertifiedFragment` is permitted only
  when the conformance witness passes, otherwise the claim degrades to `ObligationUnknown` (an
  unverified or inconclusive law), and a failed witness is `ObligationViolated`.
- **Overclaim gate** — a claimed rung must be satisfiable by the lowered legs: a bridge view may not
  emit `owl:equivalentClass`; a caveated overlap may not emit `sssom exactMatch`. Overclaiming is a
  build failure, treated identically to dropping a fact silently (the preservation-overclaim rule
  below).
- **Round-trip gate** — `iso` and `section/retraction` claims must pass canonical-IR identity:
  `put ∘ get = id` (and, for iso, `get ∘ put = id`) over the declared query class. This is the CL
  round-trip gate applied to a correspondence's get/put legs.
- **Mnemomorphism gate** — if a correspondence is declared `mnemomorphic`, its retained witness must
  actually recover the source (the in-band complement reconstructs `S ∖ im(get)`); a
  declared-recoverable cell that cannot recover is a failure.
- **Composition gate** — composing correspondences may only preserve or weaken claims (class by
  lattice-join, law-status by weakest-dominates, loss by union of unsupported-construct sets); a
  composite claiming more than its parts license is a failure.

A correspondence that uses only its `ExactPreservation` / iso subset round-trips perfectly; a
lossy-lens or prism cell is checked only on the constructs its preservation claim declares it
preserves — the same per-construct scoping as the CL gate.

## The loss ledger and preservation claims

Every projection from the canonical IR to a target dialect carries a **preservation claim** — a
machine-readable declaration of what the target preserves, what it approximates, and what it cannot
express. The claim is not a commentary annotation; it is a typed structure that travels with the
generated artifact and is checked by the conformance corpus.

The preservation polarity values are:

| Polarity | Meaning |
| --- | --- |
| `logic:ExactPreservation` | The target answers the same questions as canonical form for the declared query class |
| `logic:SoundUnderApproximation` | Everything the target entails is canonically valid; it may miss answers |
| `logic:CompleteOverApproximation` | The target does not miss answers; it may add some |
| `logic:ValidationOnly` | The target detects some invalidity but is not an entailment relation |
| `logic:InconsistencyPreserving` | A canonical inconsistency is visible in the projection |
| `logic:InconsistencyReflecting` | A projection inconsistency implies a canonical inconsistency |

**Polarity values are not mutually exclusive.** A single projection may hold multiple polarity
properties simultaneously — for example, a projection that drops answers but faithfully reflects
contradictions is BOTH `SoundUnderApproximation` AND `InconsistencyReflecting`. A projection that
neither misses nor adds answers within its certified subfragment while also reflecting any
inconsistency present is simultaneously `ExactPreservation` and `InconsistencyReflecting` for that
subfragment.

The preservation claim is therefore a **structured record**, not a single-valued enumeration. It is
indexed by:

- **query class** — the class of queries (e.g. ground-atom entailment, conjunctive query, counting
  query) for which the stated polarity holds;
- **contract** — the `logic:ReasoningContract` under which the projection was generated; polarity
  may differ across contracts even for the same projection logic;
- **construct set** — the subset of canonical IR constructs covered by the claim; constructs outside
  this set are `unsupported` in the projection.

Each cell of this indexed structure carries the set of applicable polarity values. Overclaiming on
any cell — asserting `ExactPreservation` for a query class where answers may be missed, or omitting
`InconsistencyReflecting` where contradictions are faithfully transmitted — is a conformance failure,
treated identically to dropping a fact silently. A bridge view between foundational ontologies is
typically `ValidationOnly` for its applicable query class or carries no preservation claim at all;
it is never `ExactPreservation` unless a specific subfragment is explicitly certified.

The ledger aggregates these structured claims across all projections for a given program, giving the
consumer a complete picture of which parts of the canonical reasoning are available at each target
and under which guarantees. A result produced through a lowering that did not preserve every construct
records the affected polarities and the unsupported-construct set in its `preservation` field (as
defined in [`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md)), and the downstream consumer can inspect the
ledger to see exactly which formulas were not evaluated.

### The DAG-workflow profile and cyclic-plan preservation

The DAG-workflow profile (the `logic:DagWorkflowResource` resource policy a `logic:DagWorkflowContract`
requests — see [`LOGIC-CONTRACT.md`](LOGIC-CONTRACT.md), *The DAG-workflow resource*) is the
loss-ledger discipline applied to **process** rather than rule recursion. A `logic:Plan` reaches its
flow graph through `logic:planFlowEdge`; the shared certifier runs the acyclicity check over that
graph (the same authority the build pipeline delegates to):

- an **acyclic** plan lies in the certified fragment, so the verdict is `logic:EvaluationCompleted`
  with `logic:CompleteForFragment` — a conclusive, complete-for-fragment evaluation;
- a **cyclic** plan stays valid canonically (under a non-DAG contract) but resolves to
  `logic:EvaluationUnsupported`, with the offending cycle members disclosed by `logic:dagCycleWitness`.
  This is the cardinal rule of the reasoning contract applied to the Resource facet: the loop is
  **never silently truncated** to a nearby acyclic approximation; it is reported as unsupported and
  named.

When a plan is then *lowered* to an external workflow-engine surface (Airflow, CWL, WDL, Temporal,
Nextflow — by-reference bridges, `logic:ValidationOnly`), the recurring drops that engine imposes on
the canonical model are recorded as **structured** `gmeow:lossyDrop` notes in the per-correspondence
residue of `generated/logic/projection-report.ttl`, not left to prose: a `logic:Iteration` loop is
unrolled or rejected (most DAG engines forbid cycles), a `logic:ConcurrentComposition` is serialized
where the engine has no parallel gateway, and a per-outcome `logic:compensation` is omitted where the
engine has no compensation primitive. The strong-cyclic-vs-acyclic-projection contrast and its
recorded loss are pinned end-to-end by the conformance case
`conformance/logic/cases/teleology/strong-cyclic-plan`.

## The divergence ledger as a public benchmark surface

The canonical engine is not the only reasoner. OWL reasoners, Datalog engines, Prolog solvers, and
other external reasoners each implement a fragment of the logic space and may be run over the same
inputs. When the canonical engine and an external reasoner are given inputs that fall within both
engines' declared supported fragments, they must agree. When they disagree, the divergence is
recorded explicitly.

The **divergence ledger** is a persistent, public record of such disagreements:

- the input case and the contract under which both engines were run;
- the canonical engine's output and the external reasoner's output;
- the declared preservation claim of each — which one claims `ExactPreservation` for this
  fragment, which claims only `SoundUnderApproximation`, and which makes no claim;
- a classification of the divergence: whether it represents a defect in the canonical engine, a
  limitation of the external reasoner, a genuinely unsettled semantic question, or a fragment
  boundary where the external reasoner's declared scope does not include the constructs at issue.

The divergence ledger is not a bug tracker. It is a **public agreement and benchmark surface** — a
record that lets the broader community verify the canonical engine's behavior against independent
implementations, and that gives integrators honest guidance about where the engines are aligned and
where they differ. An item in the ledger that is classified as "genuinely unsettled" is an explicit
invitation to the community to decide the ground truth; once the community does, the entry moves to
the external correctness corpus.

The ledger is the mechanism by which the two correctness axes are kept honest over time: as external
reasoners evolve and community consensus forms, the external correctness corpus grows, the ledger
shrinks, and the conformance guarantee strengthens.

## Tests as ontology data

A slice that introduces `logic:` terms does not stand apart from its own conformance checks. Each
slice carries its conformance and competency questions **as ontology data** — authored as RDF
assertions resident in the slice itself, not as external test scripts or prose descriptions. This is
the same discipline applied everywhere else in GMEOW: declare the structure, generate what can be
generated, and let the ontology be its own specification.

**Graph isolation rule.** Test individuals, negative fixtures, contradiction witnesses, and
deliberately-malformed examples MUST reside in dedicated specification or test graphs that are
**never included in the normal ontology closure**. The production `owl:imports` chain must not reach
any test graph. This rule prevents conformance artifacts from polluting reasoning over the real
ontology: a contradiction witness authored to verify paraconsistency handling must not cause the
ontology itself to appear inconsistent when loaded by a conforming consumer, and a deliberately
ill-formed individual must not trigger validation failures in a deployment that has nothing to do with
conformance testing. Graphs that contain test data carry the `logic:TestGraph` type declaration and
are excluded from the standard bundle by construction.

**Non-entailment obligations are gate-checked.** Beyond the positive competency questions, the typed
formalization governance (see [LOGIC-FOUNDATION.md, §Typed formalization
governance](LOGIC-FOUNDATION.md#typed-formalization-governance)) contributes an *executable negative*
surface: each `logic:NonEntailmentObligation` declares a forbidden predicate the closure must never
derive, and the verify gate discharges it by syntactic reachability over the rule strata and by
finite closure over the materialized derivation graph. An obligation that is violated — or that
declares a discharge condition the engine does not wire — is a hard error, exactly like a failing
competency question. The reviewer gate (an accepted `logic:FormalizationCandidate` with no recorded
reviewer decision) and the per-category candidate-coverage report run in the same pass.

Four kinds of slice-resident conformance data are defined:

**Declarative competency questions.** A competency question is a query that the slice's vocabulary
should be able to answer. Each is represented as a `logic:` goal — a formal query whose answer
shape, expected bindings, and governing contract are all declared. The competency question is not
a test that passes or fails; it is a design commitment. The conformance corpus picks it up and
verifies that the canonical engine produces the declared expected bindings under the declared
contract.

**Structural assertions.** A structural assertion is a claim about the OWL structure of the slice
that is expected to hold after classification: a class that must be non-empty in the canonical
model, a property chain that must produce a specific entailment, a disjointness that must not be
violated. These are authored as `logic:` constraints with their expected verdicts declared alongside
them, and the foundation conformance case verifies the slice against them.

**Expected validation outcomes.** For each SHACL-shaped validation shape in the slice, the slice
carries at least one positive witness (data that must pass) and at least one negative witness (data
that must fail). These are authored in the slice as `logic:` individuals with their expected
validation result declared. They are the closed-world equivalent of the competency question:
where the competency question tests open-world entailment, the validation case tests closed-world
constraint detection. A slice that ships a shape without both witnesses is incomplete; the
conformance runner reports the gap.

**Counter-example depth — native authority with a projection floor.** A flagship acceptance
scenario pairs its worked example with a *guarding counter-example*: the minimal malformed input
that must raise the scenario's named conformance-failure class. The structural / SHACL check remains
the projection floor, but a `gmeow:reasonerDrivenDischarge` marker is licensed only when the same
native producer used by the worked example also executes the malformed case and observes exactly the
declared failure class. The logic flagships therefore exercise five distinct negative judgments: a
completed closure missing its demanded entailment, an executed get/put pair violating its section
law, an incomparable counterfactual tie selecting no outcome, a claimed refutation carrying no
concrete clash witness, and a cyclic existential chase receiving an uncertified admission. The
shared runner treats the marker set as closed and hard-fails missing, duplicate, unknown, or
marker/execution-mismatched declarations. Parse failures, unsupported constructs, infrastructure
errors, and exhausted budgets are harness failures, never aliases for an expected semantic failure.
This preserves the SHACL negative witness while proving the negative space in the canonical core:
the judgment lives in `logic:` (Principle 17), and the native solver is its authority (Principle 18).

Together these four kinds of slice-resident data mean that a slice is **self-contained with respect
to its own correctness claims**. Importing a slice and passing its structural assertions, competency
questions, and validation cases is the definition of "this slice works in your implementation."
Failures are local to the slice that declared them, and the slice author is responsible for keeping
the declarations honest.

## The coherence certificate

In a paraconsistent system, coherence does not mean the absence of all contradiction. A contract
may explicitly permit witnessed, disclosed contradictions to coexist in separate context graphs
without this constituting a defect. Asserting that a paraconsistent bundle is "incoherent" because
it contains a contradiction witness is a category error; asserting that it is "coherent" in the
sense of being contradiction-free is false and misleading.

The outcome of a coherence check is a contract-scoped assertion with the following structure:

> No forbidden integrity violation and no undisclosed contradiction was found under contract **C**,
> over certified fragment **F**, within resource budget **B**, against bundle hash **H**.

Each component of this assertion is load-bearing:

- **Contract C** identifies the `logic:ReasoningContract` that defines what counts as a forbidden
  violation and what is a permitted disclosed contradiction. An assertion issued under one contract
  does not transfer to another.
- **Fragment F** identifies the subset of the bundle that was actually inspected. An assertion
  over fragment F makes no claim about constructs or graphs outside F.
- **Budget B** records the resource bound (time, depth, or iteration limit) under which the check
  ran. An assertion issued at budget B does not certify behaviour beyond that bound.
- **Bundle hash H** content-addresses the exact artifact that was checked. The assertion is invalid
  for any bundle whose canonical hash differs from H.

**Two distinct artifacts record this outcome, differing precisely in how complete the inspection was —
a completeness gate, because only a conclusive check can *certify* coherence:**

- a **`logic:CoherenceCertificate`** — issued **only** when the governing check ran to
  `evaluation = completed` with `completeness = complete-for-fragment` (see
  [`LOGIC-SEMANTICS.md` § The reasoning result](LOGIC-SEMANTICS.md#the-reasoning-result)). It certifies
  that, over fragment **F**, the inspection was *complete* and found no forbidden integrity violation
  and no undisclosed contradiction. A complete check is the only thing entitled to the word *certify*;
- a **`logic:CoherenceCheckAttestation`** — issued for a **bounded or incomplete** inspection
  (`evaluation = budget-exhausted`, or `completeness = incomplete`). It records the strictly weaker fact
  that *none was found within the completed search*. A budget-exhausted run produces an attestation,
  **never** a certificate, because it cannot rule out a contradiction it never reached. An attestation
  is honest evidence; it is not a certification of coherence.

**Undisclosed contradiction, defined.** An *undisclosed* contradiction is one whose witness was **not
captured, classified, and surfaced** under the governing contract — a glut the evaluation reached but did
not record as a typed, attributable witness. Disclosure is about *capture and surfacing*, **not** about
*where the data lives*: an intentional, production-world disagreement (two standpoints that genuinely
conflict) is **disclosed** the moment its witness is captured and classified under the contract, even
though it is real production data and not a `TestGraph` fixture. Conversely, a contradiction confined to
a `TestGraph` is still *undisclosed* if no witness for it was ever captured. The `TestGraph` isolation
rule governs *contamination of the production closure*; it does not define disclosure.

A contradiction that is witnessed, typed, and disclosed under a paraconsistent contract is **coherent**:
it is exactly the behaviour the contract anticipates, and — when it lives in a dedicated test graph —
the graph isolation rule additionally ensures it does not contaminate the production closure. The
assertion "no forbidden integrity violation" is satisfied because the contradiction is not forbidden
under contract C — it is captured and accounted for. The certificate therefore does NOT mean "this
bundle contains no contradiction." It means "every contradiction present is either permitted under the
contract and disclosed, or has been reported as a violation."

The coherence certificate is the conformance artifact that closes the paraconsistency loop: a bundle
that passes the paraconsistency corpus category, satisfies the graph isolation rule, and receives a
coherence **certificate** (from a complete check) under its governing contract is conforming with respect
to contradiction handling, regardless of how many disclosed contradiction witnesses it contains. A bundle
that has only an **attestation** from a bounded check is conforming *as far as the search reached*, and
the gap is explicit rather than papered over.

## Constitutional alignment

One conformance corpus; capability-relative conformance with a published capability manifest;
two orthogonal correctness axes; multidimensional preservation claims indexed by query class,
contract, and construct set. The Common-Logic round-trip gate enforces faithfulness from the inside;
the divergence ledger enforces agreement from the outside; tests-as-ontology-data with strict graph
isolation ensures every slice carries its own correctness claims without contaminating the production
closure; and the coherence certificate gives paraconsistent bundles a precise, contract-scoped
correctness assertion. The design refuses the two failure modes that afflict most reasoning systems:
the system that is self-consistent but wrong, and the system that is correct at one moment but drifts
silently thereafter.
