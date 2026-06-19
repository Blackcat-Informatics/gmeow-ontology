<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — Conformance Design

> Status: canonical target architecture for `logic:` conformance. This is the **conformance** member
> of the [GMEOW Logic document set](LOGIC.md#the-document-set). It defines how correctness is
> established: the conformance corpus as the enforcement contract, the loss ledger and preservation
> polarity, the two orthogonal correctness axes, the Common-Logic round-trip faithfulness gate, the
> divergence ledger as a public benchmark surface, and the design of tests-as-ontology-data. The
> engine under test is described in [`LOGIC-RUNTIME.md`](LOGIC-RUNTIME.md); the semantics it must
> satisfy are in [`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md); the typed IR whose preservation each
> projection reports is specified in [`LOGIC-IR.md`](LOGIC-IR.md).

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
  consequence facet, decidability class, and resource bound;
- **expected outputs** — derived quads, world-indexed verdicts, contradiction witnesses, projection
  artifacts, and goal or counterfactual answers, each in a canonical form against which an
  implementation's output is compared by graph isomorphism or canonical-sorted comparison.

Cases are organized by category. Each category tests a distinct design invariant:

| Category | Core invariant under test |
| --- | --- |
| Foundation | The OntoUML disciplines (rigidity, identity-supply, mediation) derive the same verdicts as the structural checks they replace over the generated downcast |
| Worlds-A | Contested claims coexist in distinct named context graphs without privileging either standpoint |
| Worlds-B | Type-level modal reasoning generates exactly zero token occurrences (the no-occurrence gate) |
| Worlds-C | A counterfactual construction does not leak into the base world graph; a genuine tie returns `unknown` |
| Projections | Each generated projection matches its declared preservation polarity and decidability class |
| Decidability | A contract that falls within a certified fragment receives a `complete-for-fragment` result; a violating one is flagged |
| Reasoning semantics | Answers under each consequence-facet value match the semantics declared by the contract |
| Explanation | Every IRI cited in a generated explanation appears in the proof trace — no justification outside the derivation |
| Paraconsistency | A cross-world contradiction is confined to separate context graphs; a within-world contradiction emits witnesses |

No category is optional for a conforming implementation. An engine that passes some categories while
skipping others is not a conforming engine; it is a partial prototype.

**Comparison discipline.** No case may depend on iteration order. RDF outputs compare by graph
isomorphism; verdict and answer outputs compare as canonical sorted structures with normalized
literals; explanation outputs compare on the set of cited axiom and rule IRIs, never on surface prose.
The faithful-by-construction property for explanations (defined in
[`LOGIC-RUNTIME.md`](LOGIC-RUNTIME.md)) is enforced at the IRI skeleton, not at the wording.

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
to produce the same correct answers will produce a regression-stability signal of zero changes, but
the correctness axis was what licensed confidence in the correctness to begin with.

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
preserve; the gate does not require that lossy constructs survive. The preservation polarity (defined
in the section below) governs which constructs are in scope for the round-trip check.

## The loss ledger and preservation polarity

Every projection from the canonical IR to a target dialect carries a **preservation judgment** — a
machine-readable declaration of what the target preserves, what it approximates, and what it cannot
express. The judgment is not a commentary annotation; it is a typed value that travels with the
generated artifact and is checked by the conformance corpus.

The preservation polarities are:

| Polarity | Meaning |
| --- | --- |
| `logic:ExactPreservation` | The target answers the same questions as canonical form for the declared query class |
| `logic:SoundUnderApproximation` | Everything the target entails is canonically valid; it may miss answers |
| `logic:CompleteOverApproximation` | The target will not miss answers; it may add some |
| `logic:ValidationOnly` | The target detects some invalidity but is not an entailment relation |
| `logic:InconsistencyPreserving` | A canonical inconsistency is visible in the projection |
| `logic:InconsistencyReflecting` | A projection inconsistency implies a canonical inconsistency |

Overclaiming preservation is a conformance failure, treated identically to dropping a fact silently.
A bridge view between foundational ontologies is typically `ValidationOnly` or carries no
preservation claim at all; it is never `ExactPreservation` unless a specific subfragment is
explicitly certified.

The ledger aggregates these judgments across all projections for a given program, giving the consumer
a complete picture of which parts of the canonical reasoning are available at each target and under
which guarantees. A result produced through a lowering that did not preserve every construct carries
the `projection-loss-affected` computation-status in the reasoning result (as defined in
[`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md)), and the downstream consumer can inspect the ledger to
see exactly which formulas were not evaluated.

## The divergence ledger as a public benchmark surface

The canonical engine is not the only reasoner. OWL reasoners, Datalog engines, Prolog solvers, and
other external reasoners each implement a fragment of the logic space and may be run over the same
inputs. When the canonical engine and an external reasoner are given inputs that fall within both
engines' declared supported fragments, they must agree. When they disagree, the divergence is
recorded explicitly.

The **divergence ledger** is a persistent, public record of such disagreements:

- the input case and the contract under which both engines were run;
- the canonical engine's output and the external reasoner's output;
- the declared preservation polarity of each — which one claims to be `ExactPreservation` for this
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

Three kinds of slice-resident conformance data are defined:

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

Together these three kinds of slice-resident data mean that a slice is **self-contained with respect
to its own correctness claims**. Importing a slice and passing its structural assertions, competency
questions, and validation cases is the definition of "this slice works in your implementation."
Failures are local to the slice that declared them, and the slice author is responsible for keeping
the declarations honest.

## Constitutional alignment

One conformance corpus; two orthogonal correctness axes; preservation judgments that travel with
every generated artifact. The Common-Logic round-trip gate enforces faithfulness from the inside;
the divergence ledger enforces agreement from the outside; tests-as-ontology-data ensures every
slice carries its own correctness claims. The design refuses the two failure modes that afflict most
reasoning systems: the system that is self-consistent but wrong, and the system that is correct at
one moment but drifts silently thereafter.
