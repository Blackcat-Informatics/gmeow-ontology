<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Mathematics — Vision and Doctrine

> The **manifesto** of the GMEOW Mathematics design set; it carries the vision, doctrine, and
> lineage. The expression calculus, probability layer, statistics layer, and projection contract
> live in the sibling documents below. Where this document states a thesis once, the siblings make
> it precise — repetition is replaced by cross-reference on purpose. The cross-slice contract
> binding this slice to its co-foundational peers (`logic:`, `lang:`) — the seam registry, shared
> disciplines, and acceptance bar — is [`docs/GROUNDING.md`](../../../../docs/GROUNDING.md).

## The document set

| Document | Genre | Contents |
|---|---|---|
| `MATHEMATICS.md` (this) | manifesto | vision, doctrine, lineage; the position of the slice against `logic:` and `observations`; the external-vocabulary-as-projection posture |
| [`MATHEMATICS-EXPRESSIONS.md`](MATHEMATICS-EXPRESSIONS.md) | mathematical core | the mathematical reference layer (concepts, symbols, notations, theories, contexts); the typed expression AST (literals, symbol references, variables, application, binding, indexed argument slots; framed operator signatures and closed-form-function expression algebra; formula identity independent of rendering; the bound/free discipline; normalization and declared equivalence); the object/structure layer (numbers, sets, functions, spaces, algebraic and category-theoretic structures); and the statement/proof/theory layer (axioms, theorems, proofs, verification results) |
| [`MATHEMATICS-PROBABILITY.md`](MATHEMATICS-PROBABILITY.md) | probability layer | probability spaces, sample spaces, σ-algebras, events, measures, random variables, distributions and families, mandatory parameterization, dependency models (independence, Bayesian networks, factor graphs, Markov kernels), and the seam into `logic:probabilityModel` |
| [`MATHEMATICS-STATISTICS.md`](MATHEMATICS-STATISTICS.md) | statistics layer | populations, samples, sampling frames, variables, dataset matrices, models, assumptions, estimators, estimates, hypotheses, tests, p-values, intervals, effect sizes, diagnostics, experimental designs, missingness; the frequentist/Bayesian parity and the provenance-heavy result contract |
| [`MATHEMATICS-PROJECTIONS.md`](MATHEMATICS-PROJECTIONS.md) | projection contract | the generated lossy lowerings — MathML, OpenMath/OMDoc/MMT content, RDF Data Cube/SDMX, STATO-style method references, QUDT quantity references, Wikidata authority links — each carrying a preservation judgment in the loss ledger |
| [`MATHEMATICS-RUNTIME.md`](MATHEMATICS-RUNTIME.md) | runtime & ingestion | ingestion as projection run backwards (parser-compilers lifting R/LaTeX/MathML/OpenMath into the canonical AST; DSL/API authoring; both converging on one AST); content-addressed expression interning against ABox density; the solver-profile handoff to external engines with results returned as inference-run observations; and the Rust-first implementation posture and acceptance gates |
| [`MATHEMATICS-CONFORMANCE.md`](MATHEMATICS-CONFORMANCE.md) | conformance | the gate matrix — every hard rule mapped to its enforcing gate (OWL axiom / SHACL Core / SHACL-SPARQL / source-lint / Rust validator / competency query / projection test) and a named failure class; the reuse of the `logic:` `preservationKind` vocabulary; the positive/negative fixture corpus |
| [`MATHEMATICS-REFERENCES.md`](MATHEMATICS-REFERENCES.md) | references appendix | the classified survey of ~70 external standards, ontologies, formalisms, classification schemes, and engines — each tagged subsume/project/link/reference × license × kind; the primary anchors (mathlib, Wikidata QID, QUDT/DCC); and the original surface GMEOW authors where no external ontology exists; staged for the `metadata/references.ttl` ledger |
| [`MATHEMATICS-NUMBERS-AND-SETS.md`](MATHEMATICS-NUMBERS-AND-SETS.md) | bedrock objects | number systems and exactness (including the signed extended real line and its two infinite poles), arithmetic, sets and their construction (extensional/intensional, and ordered intervals with declared endpoint inclusion), relations and functions (including piecewise functions over interval pieces) — the primitives every other charter quantifies over |
| [`MATHEMATICS-MEASURE-AND-DIMENSION.md`](MATHEMATICS-MEASURE-AND-DIMENSION.md) | measure & dimension | measurable spaces, measures, and integration (the probability foundation; reified subset measure evaluation μ(A)); dimensional analysis as a cross-cutting homogeneity gate; the quantity/unit/dimension grounding (QUDT/OM/D-SI) |
| [`MATHEMATICS-ALGEBRA.md`](MATHEMATICS-ALGEBRA.md) | algebra | the structure hierarchy (groups→rings→fields→Lie), homomorphisms as declared laws, root systems and Weyl groups (the E8 flagship), and ring homomorphisms under encryption (the homomorphic-encryption flagship) |
| [`MATHEMATICS-ANALYSIS-AND-GEOMETRY.md`](MATHEMATICS-ANALYSIS-AND-GEOMETRY.md) | analysis & geometry | calculus as AST-native binders (including structured limit results and the law-backed analytic properties, smoothness among them carried as an honest second-order boundary), special functions, topology, and differential geometry / manifolds (Lorentzian metrics — the math side of the physical-frame case; conformal compactification and the boundary at infinity); the named-complement gate |
| [`MATHEMATICS-LINEAR-ALGEBRA-AND-LEARNING.md`](MATHEMATICS-LINEAR-ALGEBRA-AND-LEARNING.md) | linear algebra & learning | inner-product spaces and decompositions, PCA (the KG-projection flagship), embeddings and latent spaces, and the tensor structure of AI systems (the AI-self-structure flagship) |
| [`MATHEMATICS-BRIDGES.md`](MATHEMATICS-BRIDGES.md) | ingestion bridges | the parse-into front-ends — R→`math:` (any script), ONNX→`math:`, and proof-as-process — each splitting input across the `logic:` and `math:` grounding layers and hard-failing on the unliftable |

> **Reading this design set.** The declarative present tense is normative: "X is" means a conforming
> realization implements X, established by canonical `module.ttl` axioms and `logic:Constraint`
> records, competency queries, and the
> projection loss ledger. It is not a claim that any particular implementation already realizes X
> except as those gates demonstrate. The sibling documents named above are companion charters,
> written to the same voice; together with this manifesto they constitute the complete mathematics
> design set.

## The thesis

GMEOW must be able to talk about mathematical and statistical artifacts with the same claim,
provenance, and preservation rigor it applies to every other kind of knowledge. Probabilities,
estimates, formulas, distributions, and theorems routinely carry decisive inferential weight, and
they are routinely reduced — in ordinary practice and in most ontologies — to bare numbers, opaque
strings, and untyped labels. That reduction discards exactly the structure that makes them
trustworthy.

The mathematics slice supplies **mathematical and statistical domain objects** — structured, typed,
framed, sourced, and projection-audited — that the native `logic:` reasoning layer and the
`observations` spine consume, reason over, project, and transport. Its core commitment:

> A probability is not a confidence score. A statistical estimate is not a bare number. A formula is
> not an opaque string. A distribution is not a label. A theorem is not merely prose. Every
> mathematical or statistical artifact that carries inferential weight is structurally represented,
> typed, framed, sourced, and projection-audited.

This is the project's own doctrine applied to mathematics: author the maximal, explicit,
factored form in the canon, then project lossily and visibly to every consumer — MathML for
notation, OpenMath for content, RDF Data Cube for statistical cubes, QUDT for units, STATO for
statistical methods, Wikidata for named-concept identity. None of those becomes a second source of
truth; each is a generated view with a recorded preservation judgment.

## The grounding layers

GMEOW has three co-foundational grounding layers (CONSTITUTION.md Principle 19), and mathematics
is one of them; the third, `lang:` — meaning and expression — is chartered in its own design set
([`../../lang/design/LANG.md`](../../lang/design/LANG.md)) and meets `math:` at two registered
seams (rendering, math → lang; quantity, lang → math; see
[`docs/GROUNDING.md`](../../../../docs/GROUNDING.md)), so this manifesto's concern remains the
`logic:`/`math:` relationship it fixes here. `logic:`
grounds **reasoning** — truth, inference, proof, modality — as a Turing-complete computational
substrate built on a relational core (predicates, rules, resolution). `math:` grounds **quantity
and structure** — number, space, operation, measure, dimension — as the canonical structural
substrate. Neither reduces to the other, and almost every real artifact needs both: a statistical
inference is a `math:` object *reasoned over* by `logic:`; a physical law is a `math:` tensor
equation carrying `logic:` modal content; a proof is a `logic:` derivation over `math:` statements.

| | `logic:` — logical grounding | `math:` — mathematical grounding |
|---|---|---|
| **Grounds** | reasoning, truth, inference, proof | quantity, structure, measure, dimension |
| **Core** | relational (predicates, rules, resolution) | structural (objects, operations, morphisms) |
| **Character** | Turing-complete computational substrate | canonical structural/quantitative substrate |
| **Canonical IR** | the full-FOL typed IR | the expression AST + object/structure model |
| **Projects out to** | OWL, Datalog, SHACL, Prolog, N3, gUFO | MathML, OpenMath, RDF Data Cube, QUDT, STATO |
| **Dogfoods** | GMEOW's axioms and foundation ground in it | GMEOW's quantities, counts, dimensions, probabilities ground in it |

The two layers **interlock at a declared bridge, not by merger.** A mathematical expression *denotes*
into a `logic:` term, formula, type, or proof object, with its denotation kind and lowering
preservation declared ([`MATHEMATICS-EXPRESSIONS.md`](MATHEMATICS-EXPRESSIONS.md)); the shared
lingua franca is category theory, present as `math:Category`/`math:Functor`/`math:NaturalTransformation`
and already used by `logic:`'s correspondence calculus. The bridge runs **one way**: `math:` → `logic:`
(expressions lower into the IR; probability-model objects *satisfy* `logic:probabilityModel`).
`logic:` never depends back on `math:` — its quantitative facets stay abstract requirements that
`math:` objects satisfy — so the complementarity is realized without a dependency cycle and the slice
DAG stays acyclic.

The symmetry is made concrete in the namespace. The grounding layer's terms live in the **`math:`**
namespace (`https://blackcatinformatics.ca/math/`), peer to **`logic:`**
(`https://blackcatinformatics.ca/logic/`) — the grounding layers each own a namespace, distinct
from the general `gmeow:` ontology namespace the domain slices share. Terms this layer *borrows* from
other slices keep their home namespace — the `observations` spine (`gmeow:Observation`,
`gmeow:vantage`, and measurement qualifiers over the math-owned `math:Quantity`),
`provenance`/`events` (`gmeow:Activity`, `gmeow:wasGeneratedBy`),
and the alignment vocabulary (`gmeow:TermEquivalence`) — and the slice is still *declared* with the
`gmeow:` slice-manifest vocabulary (`gmeow:Slice`, `gmeow:sliceTier`, `gmeow:sliceDependsOn`). A
worked example therefore mixes all three namespaces on purpose: a `math:` object *held via* a
`gmeow:Observation` and *denoting into* a `logic:` formula is the grounding-layer composition made
visible.

## Why This Exists

The current ontology can *hold* a scalar — a value with a unit, a reference frame, a determinacy,
an uncertainty, and provenance — through the `observations` spine. That is the right substrate, and
the mathematics slice builds on it rather than around it. But holding a scalar is not the same as
saying **what the scalar means** or **how it was produced**, and three structural gaps follow.

First, **a number is not its meaning.** `p = 0.03` is a scalar; a *p-value* is that scalar together
with a hypothesis, a test, a test statistic, a null model, the data and sampling frame it was
computed over, and the assumptions the test requires. Strip those away and the number is
uninterpreted. The observation spine can carry the scalar; it cannot, by itself, say the scalar is a
p-value of *this* test over *this* data under *these* assumptions. The mathematics slice supplies
that meaning as first-class structure.

Second, **a formula is not a string.** A display string — TeX, MathML presentation markup, prose —
is a rendering. A computable formula is a structured expression: an operator, ordered operands in
indexed slots, bound and free variables with declared type and domain context, and symbol
references that resolve to a theory. Storing the string and discarding the structure makes the
formula un-checkable, un-normalizable, and un-projectable except at the fidelity it was ingested at.
Canonical computable content is the expression AST; the string is one of its renderings.

Third, **a probability is not a confidence.** The `logic:` layer already draws — as a hard design
boundary — a distinction the rest of the world routinely collapses: between a **probability** (a
measure over an event in a probability space), a **confidence** assigned by a source or process, a
**solver/ranking weight**, and **evidential support** (`slices/grounding/logic/design/LOGIC-SEMANTICS.md`).
The mathematics slice names the probability spaces, measures, distributions, and dependency models
that give probability its meaning — and it must never let that machinery silently make `confidence`
mean `probability`. A probability value that cannot name its probability frame is ill-formed, not
merely under-annotated.

## Position within GMEOW

The mathematics slice is a **core** slice. It sits below the scientific, research, finance, risk,
AI, and analysis layers that consume mathematical and statistical structure, and above the base
identity, claim, provenance, and reasoning slices it depends on. Its dependency set is drawn
entirely from existing core slices — `kernel` and `entities` for base type vocabulary; `logic` for
formula, reasoning, proof, and probability-facet semantics; `observations` for claims,
measurements, quantities, uncertainty, and held results; `evidence`, `provenance`, and `citations`
for warrant, method, and bibliographic grounding; `temporal` and `versions` for time-scoped and
evolving models; and `notation` for symbolic and presentation notation. Where `notation` is
currently oriented toward general notation, this slice **reuses and deepens it** rather than forking
a mathematical-notation twin.

Two boundaries are load-bearing and stated once here.

**It does not replace `logic:`.** The `logic:` layer owns formal reasoning semantics — proof
traces, model semantics, the typed reasoning result, paraconsistency, state change, and the
probability-as-reasoning-facet vocabulary (`logic:ProbabilityModel`, `logic:probabilityModel`,
`logic:ProbabilisticFramework`). The mathematics slice creates **no alternate reasoning language**.
It supplies mathematical objects that become the inputs, operands, constraints, labels, assumptions,
and explanations for the reasoning layer. When a probabilistic reasoning request references
probabilistic facts, it points at an explicit probability-model *object* the mathematics slice
defines; if that object is absent or structurally invalid, the engine reports unsupported or
not-evaluated rather than silently assuming independence.

**It does not replace `observations`.** The observation spine owns claim acts and the qualifiers that
place a measured result in a unit/reference frame with determinacy, uncertainty, provenance, and a
true-magnitude link. The canonical dimensioned result itself is `math:Quantity`, with its numeric
value on `math:quantityValue`; observations qualifies that same object rather than minting aliases.
The mathematics slice also gives quantities mathematical and statistical *meaning* — a probability-measure value, a
p-value, an effect size, an estimate, a posterior mean, an interval bound, a distribution
parameter — while still grounding every held result as an observation when it is measured, derived,
inferred, or asserted from a vantage. A statistical estimate is therefore *both* a structured
statistical object *and* the result of an inference/analysis act recorded as an observation.

## Slice placement, tier, and manifest

The slice is placed at `slices/grounding/math/` — the `grounding` group is the grounding layers'
home, and the directory name matches the `math:` namespace prefix — and declares `gmeow:tierCore` —
the manifest, not the directory, is the source of tier (the build reads the tier from
`gmeow:sliceTier`; the `core`/`extensions`/`grounding` path segment is human organization it never
reads). Core tier is the deliberate
commitment the manifesto makes: mathematical objects, probability, statistics, proofs, and
distributions are part of the default mental model that the scientific, research, finance, risk, AI,
and analysis layers build on, not an optional extension they each re-derive.

The manifest is authored (`manifest.ttl`) and is the sole source of slice identity and tier. Its
realized dependency set — gate-verified against the *computed* cross-slice reference graph, which
trims the declaration to exactly the slices whose terms `module.ttl`/`shapes.ttl` actually
reference — is `logic` and `lang`, together with the co-foundational peerage declaration
(`gmeow:sliceCoFoundationalWith` naming `logic` and `lang`, per
[`docs/GROUNDING.md`](../../../../docs/GROUNDING.md)). The broader ten-slice skeleton this
manifesto once sketched (kernel, entities, observations, evidence, provenance, citations, temporal,
versions, notation) was trimmed by that computed graph: the slice reaches those spines through
`gmeow:` core vocabulary (Observation, vantage, Activity) rather than referencing their slice-owned
terms directly, and the notation seam landed as the `lang:Rendering` graft instead of a `notation`
dependency. The `lang` dependency carries the registered rendering seam
(`math:ExpressionRendering ⊑ lang:Rendering`); the `logic` dependency carries the compilation,
laws, and preservation seams.

## Lineage and Supersession

The mathematics slice inherits the project's supersession doctrine: each external mathematical or
statistical vocabulary contributes a fragment, imposes a restraint GMEOW rejects, and becomes a
**generated, lossy projection** rather than the canonical model. GMEOW aligns to each by reference,
projects to each where a consumer needs it, and records the loss.

| External vocabulary | Contributes | Restraint we reject | How the mathematics slice exceeds it |
|---|---|---|---|
| **QUDT** | units, quantity kinds, dimensions, dimensional analysis, conversion | a unit/quantity model with no claim, provenance, or result context | quantities are held as observation results with frame, determinacy, uncertainty, and provenance; QUDT IRIs are referenced, not imported wholesale |
| **STATO / OBCS** | statistical tests, methods, variables, model terms, assumptions | an OBO-family schema whose expressivity ceiling would cap GMEOW's | statistical methods, tests, and assumptions are structured objects with expression, probability, and dependency detail STATO cannot carry; STATO is an alignment/reference target |
| **RDF Data Cube / SDMX / DDI** | multidimensional statistical data publication | a flat cube with no model, assumption, or inference structure | the canonical source retains model/assumption/probability/provenance structure and exports a declared-loss cube on demand |
| **MathML 4 / MathML Core** | presentation and content notation for mathematics | a rendering treated as identity | canonical identity is the expression AST; MathML is a presentation/content projection, canonical only when a formula was ingested at that fidelity and marked as such |
| **OpenMath / OMDoc / MMT** | symbol/content dictionaries, theory and module references | content serialization taken as the theory itself | symbol identity and theory references align to OpenMath/OMDoc/MMT; the GMEOW expression AST and symbol references remain the local source of truth |
| **ProbOnto / Distributome** | distribution families, parameters, relationships, reparameterizations | a catalog whose parameter conventions would become implicit defaults | GMEOW holds its own local distribution-family individuals for high-value families with explicit parameterization; the catalogs are references after license and maintenance review |
| **Wikidata** | QIDs for named concepts, theorems, distributions, constants, structures | an identifier source mistaken for a definition source | Wikidata QIDs are authority links that name alignments; the GMEOW term remains the definition and the local source of truth |

The governing rule across the table: **external identifiers name alignments, not identity.** A
GMEOW mathematical concept may align to Wikidata, OpenMath, STATO, or QUDT, but the GMEOW term is
the local source of truth, and every lossy export carries a preservation record.

## Design influences — the AST discipline and factored parameterization

Two influences shape the slice beyond the external vocabularies it supersedes.

**The expression-as-AST discipline** is the same commitment `logic:` makes when it insists a rule is
a typed intermediate representation rather than a serialized string. A computable formula is an
application tree with indexed argument slots — not RDF list ordering, not a rendered string — so
that argument order is explicit, slots are unique, non-negative, zero-based, and contiguous, every
variable occurrence is either bound by a binder or explicitly marked free with type and domain
context, and every symbol reference resolves locally or through a declared external reference. A
display string exists only as a rendering of an AST or as explicitly non-computable prose. This is
the mathematical instance of the project-wide principle that the canonical form is the maximal,
explicit, checkable one and the string is a projection.

**Factored parameterization** applies the orthogonality principle to distributions. Many
distribution families have several conventional forms — a normal by mean and standard deviation, by
mean and variance, or by mean and precision; a gamma by shape and rate or shape and scale.
Collapsing them into a single labeled "normal distribution" is a classic source of silent
wrongness. The slice therefore treats parameterization as an explicit, first-class object: a
distribution names a family *and* a parameterization, a parameterization declares its required
parameter roles, an instance supplies exactly those roles, and reparameterization is a **declared
transform**, never a string rewrite or an inferred default. A distribution without a family, or a
parametric distribution without a parameterization, is ill-formed.

## The canonical layer model

The slice is one ontology unit factored into coherent internal regions. The manifesto names the
regions; the sibling charters make each precise.

- **Mathematical reference layer** — named concepts, symbols, notations, theories, and contexts,
  with authority alignments to external vocabularies. External identifiers name alignments; the
  GMEOW term is identity. Detailed in [`MATHEMATICS-EXPRESSIONS.md`](MATHEMATICS-EXPRESSIONS.md).
- **Expression AST layer** — literals, symbol references, variables, application, binding, and
  indexed argument slots; formula identity independent of rendering. Detailed in
  [`MATHEMATICS-EXPRESSIONS.md`](MATHEMATICS-EXPRESSIONS.md).
- **Object and structure layer** — numbers, sets, tuples, vectors, matrices, tensors, functions,
  relations, operations, sequences, spaces (metric, topological, measure, vector), algebraic
  structures (group, ring, field), graphs, and category-theoretic objects (category, morphism,
  functor, natural transformation), modeled at a useful ontological granularity rather than as a
  theorem prover encoded in OWL. Detailed in [`MATHEMATICS-EXPRESSIONS.md`](MATHEMATICS-EXPRESSIONS.md).
- **Statement/proof/theory layer** — axioms, theorems, lemmas, corollaries, conjectures,
  definitions, proofs, proof steps, proof methods, formal-verification results, and
  counterexamples. A theorem label is not a truth bit: a theorem claim is held from a vantage,
  under a theory and context, with a proof or external warrant, and a proof checker's success is
  itself an observation/verification claim with provenance. Detailed in
  [`MATHEMATICS-EXPRESSIONS.md`](MATHEMATICS-EXPRESSIONS.md).
- **Probability layer** — probability spaces, sample spaces, σ-algebras, events, measures,
  conditional probability, independence, random variables, distributions, families,
  parameterizations, moments, dependency models, and the Bayesian prior/likelihood/posterior chain.
  Detailed in [`MATHEMATICS-PROBABILITY.md`](MATHEMATICS-PROBABILITY.md).
- **Statistics layer** — populations, sampling frames, samples, observation units, variables,
  dataset matrices, models, formulas, assumptions, estimators, statistics, estimates, fitted
  models, inference runs, hypotheses, tests, p-values, confidence and credible intervals, effect
  sizes, predictions, residuals, diagnostics, experimental designs, randomization, and missingness
  mechanisms. Detailed in [`MATHEMATICS-STATISTICS.md`](MATHEMATICS-STATISTICS.md).
- **Projection and alignment surfaces** — the generated lossy lowerings to MathML, OpenMath-style
  content, RDF Data Cube, STATO, QUDT, and Wikidata. Detailed in
  [`MATHEMATICS-PROJECTIONS.md`](MATHEMATICS-PROJECTIONS.md).

## The doctrine — hard fails and the loss ledger

The slice inherits the project's low/no-optionality posture: malformed probability models,
unresolved symbols, ambiguous parameterizations, and lossy unmarked projections **fail early**. The
manifesto records the doctrine; the sibling charters specify the SHACL and source-lint gates that
enforce it.

- **Formulas are ASTs; renderings are projections.** A computable expression is never represented
  only by a string literal. A string is a rendering of an AST or explicitly non-computable prose.
- **Probability is framed, and never inferred from confidence.** Probability values lie in `[0, 1]`
  unless explicitly represented in another lawful frame such as odds or log-odds with that frame
  declared; they name their probability frame or model; and no projection may silently convert a
  confidence into a probability.
- **Distributions are parameterized.** A distribution names a family and a parameterization;
  required parameter roles are present exactly once; silent default parameterizations are forbidden;
  reparameterization is a declared transform.
- **Statistical results are provenance-heavy.** A statistical number without a method, data
  provenance, and interpretation frame is at most an uninterpreted scalar. An estimate references
  its estimated parameter and estimator; a p-value references its hypothesis, test, test statistic
  or procedure, and data/model context; a confidence interval and a credible interval are distinct
  result kinds and are never collapsed.
- **Frequentist and Bayesian outputs are peers.** Neither family is privileged; each has its own
  first-class result kinds and required frames.
- **Every lossy projection is recorded.** Any projection that drops assumptions, parameterization,
  dependency structure, expression binding, or provenance emits a machine-readable
  preservation/loss record in the loss ledger, in the same discipline `logic:` applies to OWL,
  Datalog, SHACL, and the correspondence lowerings.

The loss ledger is the mathematical instance of the project-wide preservation contract: "GMEOW
projects faithfully to vocabulary V" is a checkable claim with a recorded polarity, not a slogan.

## Flagship competency questions

The grounding layer's depth is defined by concrete grand acceptance scenarios, not by adjectives. A
conforming realization is one that can represent each of the following structurally — typed, framed,
sourced, and projection-audited — and each stresses a different axis of the layer. All five reduce
to the same core: *structure and structure-preserving maps, expressed across the `math:`/`logic:`
bridge, deeply enough to be self-describing.*

1. **The symmetry groups of E8.** Lie group and Lie algebra, root system (240 roots in ℝ⁸), simple
   roots, Cartan matrix, Dynkin diagram, and Weyl group — with a symmetry group modeled as the
   automorphisms preserving a structure (the morphism core). Aligns to Lean mathlib and a Wikidata
   QID; the Lie/root-system/Weyl depth is authored ([`MATHEMATICS-REFERENCES.md`](MATHEMATICS-REFERENCES.md)).
2. **How homomorphic encryption works.** A ring homomorphism whose homomorphic law
   (`Dec(E(a) ⊗ E(b)) = a ⊕ b`) is a `logic:` formula grounded in `math:` ring/lattice structure,
   with encrypt/evaluate/decrypt as activities. The purest test of the one-way bridge.
3. **Complex proofs as process.** The proof layer (`Proof`/`ProofStep`/`dependsOnAxiom`) bound to
   `logic:`'s teleology and transaction layers: a proof effort is a goal-decomposed process whose QED
   is a verification observation. Mirrors mathlib's declaration DAG and Metamath's axiom→theorem DAG.
4. **R → `math:`, for any R script.** A universal lift where a script's mathematical/statistical
   content lifts into `math:` (`y ~ x1 + x2` → a `ModelFormula` AST, `lm`/`glm` → models, `rnorm` →
   distributions) and its computation lowers into `logic:`. A live demonstration of the two-layer
   architecture; it hard-fails with a typed diagnostic on anything it cannot lift, never dropping
   silently ([`MATHEMATICS-RUNTIME.md`](MATHEMATICS-RUNTIME.md)).
5. **An AI describing its own structure.** A tensor computational graph as a `math:` object (the
   expression AST over tensors), weights as matrices in a parameter space, embeddings as latent-space
   geometry with residual meaning, training as optimization over a loss surface — with the AI's
   *reflection* on that structure carried at the `logic:` metalevel and the metacognition layer. The
   dogfooding apex: the tool grounds the structure of the system using it, lifted from any ONNX/model
   graph exactly as the R-bridge lifts any R script.

These are the layer's acceptance bar. The domain charters ([`MATHEMATICS-REFERENCES.md`](MATHEMATICS-REFERENCES.md)
records which anchor externally and which GMEOW authors) exist to make them answerable.

The acceptance bar is itself a typed, gated contract, not prose. Each of the five scenarios above is
reified as a `gmeow:FlagshipScenario` (authored in `examples/flagship-acceptance.ttl`) binding it to
the five artifacts that realize and enforce it — its worked example
(`gmeow:demonstratedByExample`), its competency question (`gmeow:demonstratedByCompetency`), the
native producer that emits it (`gmeow:demonstratedByProducer`, a `math::producers::*` entrypoint), its
guarding counter-example (`gmeow:guardedByCounterExample`), and the named failure class its gate
raises (`gmeow:enforcesFailureClass`). Three static surfaces enforce the wiring — the shared
`gmeow:FlagshipScenarioShape` (SHACL cardinality) and thin slice `math:FlagshipScenarioShape`
(failure-range), a structural assertion, and a native cross-check that resolves each competency
reference into the tests dataset and confirms it is a registered, green (`cqExpectRow`) question —
**plus execution**: the discharge harness runs each counter-example, worked example, and native
producer, asserting exactly the declared failure class fires, the example is clean, and the producer
emits its pinned output. So an unwired scenario is the typed failure `math:UnwiredFlagshipScenario`,
and the depth bar cannot silently regress (see
[`MATHEMATICS-CONFORMANCE.md`](MATHEMATICS-CONFORMANCE.md), "Flagship acceptance-manifest rules").

## Constitutional Alignment

The mathematics slice is the project's doctrine applied to mathematics and statistics. The
CONSTITUTION requires a maximal canonical model, maximal linking, explicit and gated projection, and
no compatibility format promoted above the canonical source. The statement layer realizes this for
facts; `logic:` realizes it for axioms, rules, and the foundation; the observation spine realizes it
for held quantities. The mathematics slice realizes it for the *meaning* of mathematical and
statistical artifacts — formulas as ASTs, probabilities as framed measures, distributions as
parameterized objects, statistical results as provenance-heavy observations — and takes QUDT, STATO,
RDF Data Cube, MathML, OpenMath, the distribution catalogs, and Wikidata to their correct places as
documented, reproducible, lossy projections, never second sources of truth.

## End State

The end state is not "a statistics vocabulary, but richer." It is:

- mathematical and statistical artifacts are structurally represented, typed, framed, sourced, and
  projection-audited, with the same claim and provenance rigor GMEOW applies everywhere else;
- computable formulas are canonical expression ASTs; MathML, TeX-like strings, OpenMath
  serializations, and prose are renderings and projections, never canonical substitutes;
- probability-domain objects — spaces, measures, events, random variables, distributions,
  dependency models — are named in the mathematics slice and consumed by `logic:`'s probabilistic
  reasoning semantics, with probability held strictly distinct from confidence, weight, and
  evidence;
- distributions carry mandatory, explicit parameterization, and ill-formed distributions are
  rejected rather than defaulted;
- statistical estimates, p-values, and intervals are provenance-heavy observation results linked to
  their data, model, estimator, assumptions, and inference run, with frequentist and Bayesian
  outputs as first-class peers;
- QUDT, STATO, RDF Data Cube, MathML, OpenMath/OMDoc/MMT, the distribution catalogs, and Wikidata
  are generated, lossy projections and alignment surfaces, each carrying a preservation judgment in
  the loss ledger;
- projection loss is visible, machine-readable, and tested.

This makes the mathematics slice match the rest of the project: a maximal canonical model, maximal
linking, explicit projection, and no compatibility format — not MathML, not RDF Data Cube, not
STATO — promoted above the canonical source.
