<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Mathematics — The Mathematical Core

> The **mathematical-core charter** of the GMEOW Mathematics design set: the mathematical reference
> layer, the typed expression AST, the object-and-structure layer, and the statement/proof/theory
> layer. It makes precise the claims the manifesto ([`MATHEMATICS.md`](MATHEMATICS.md)) states once —
> that a mathematical concept's identity is not its rendering or its external ID, and that a
> computable formula is a structured tree, not a string. The probability and statistics layers that
> build on this core are in [`MATHEMATICS-PROBABILITY.md`](MATHEMATICS-PROBABILITY.md) and
> [`MATHEMATICS-STATISTICS.md`](MATHEMATICS-STATISTICS.md); the lossy lowerings of everything here are
> in [`MATHEMATICS-PROJECTIONS.md`](MATHEMATICS-PROJECTIONS.md).
>
> **Reading this charter.** The declarative present tense is normative: "X is" means a conforming
> realization implements X, established by the slice's `shapes.ttl`, competency queries, and the
> projection loss ledger — not a claim that any implementation already realizes X except as those
> gates demonstrate.

## Purpose

The mathematical core names *what mathematics is about* before any probability or statistics is
layered on: the concepts, symbols, and theories a formula references; the structured expressions
that are the formulas themselves; the objects and structures those expressions denote; and the
statements, proofs, and verification results that carry mathematical knowledge. Four regions
factor the core, each with a single load-bearing commitment.

- **Reference layer** — *identity is the GMEOW term, not its rendering or external ID.*
- **Expression AST layer** — *a computable formula is a tree, not a string.*
- **Object/structure layer** — *name structure at a useful granularity; do not encode a prover in OWL.*
- **Statement/proof/theory layer** — *a theorem label is not a truth bit.*

## The mathematical reference layer

The reference layer lets GMEOW name a mathematical concept — the Riemann zeta function, the normal
distribution family, the axiom of choice, the symbol `∫` — without collapsing its identity into a
MathML rendering, a Wikidata QID, or a paragraph of prose.

Core classes: `math:MathematicalObject`, `math:MathematicalConcept`, `math:MathematicalSymbol`,
`math:MathematicalNotation`, `math:MathematicalTheory`, `math:MathematicalContext`,
`math:MathematicalDefinition`, and `math:MathematicalStatement`.

Core properties: `math:hasMathematicalSymbol`, `math:hasNotation`, `math:definedInTheory`,
`math:usesDefinition`, `math:formalizesExpression`, and `math:informalGloss`.

The governing rule:

> **External identifiers name alignments, not identity.** A GMEOW mathematical concept may align to
> Wikidata, OpenMath, STATO, or QUDT, but the GMEOW term remains the local source of truth.

A concept is a first-class individual, not a subclass proliferation. The slice does **not** mint a
new OWL class for every named constant, structure, distribution, or symbol; named mathematical
objects are individuals, and a class-level term is introduced only where a genuine class of
individuals is needed (e.g. `math:Distribution` is a class; `π` is an individual `math:Number`).
This keeps the TBox small and the ABox expressive, matching the project's frame-relative modeling.

**Alignment is a mapping record, not a bespoke predicate.** External authority links follow the
established repository pattern: `gmeow:TermEquivalence` reification records in the slice's
`mappings/equivalences.ttl` (with `gmeow:alignSubject`/`gmeow:alignPredicate`/`gmeow:alignObject`,
a `semapv:` justification, and a confidence), lowered as a `logic:Correspondence` — the ninth
`logic:` IR node kind (`slices/grounding/logic/design/LOGIC-CORRESPONDENCE.md`). The mathematics slice
introduces **no** free-standing `authorityLink` predicate; a Wikidata QID for a named concept is a
`skos:exactMatch`/`skos:closeMatch` alignment carrying its preservation judgment in the loss ledger,
exactly as every other slice records its external links.

Theory and context individuals (`math:MathematicalTheory`, `math:MathematicalContext`) scope
symbol meaning and axiom dependence: the same glyph `e` denotes Euler's number in one theory and
the identity element in another, and a symbol reference is meaningful only relative to the theory
that defines it. Time-scoped and versioned theories reuse the `temporal` and `versions` slices —
a definition that evolves is a versioned `math:MathematicalDefinition`, not an overwrite.

## The expression AST layer

A formula that is meant to be checked, normalized, evaluated, or projected is an **abstract syntax
tree**, never an opaque string. This is the mathematical instance of the project-wide principle
that the canonical form is the maximal, explicit, checkable one and the surface string is a
projection of it.

Core classes: `math:MathematicalExpression` (the abstract root), `math:LiteralExpression`,
`math:SymbolReference`, `math:VariableExpression`, `math:ApplicationExpression`,
`math:BindingExpression`, `math:ArgumentSlot`, `math:ExpressionType`, and
`math:ExpressionRendering`.

Core properties: `math:operator`, `math:argumentSlot`, `math:slotIndex`, `math:slotExpression`,
`math:boundVariable`, `math:freeVariable`, `math:expressionType`, `math:rendersAs`,
`math:parseSource`, and `math:normalForm`.

### The hard rules of the AST

1. **A computable formula is an AST.** A `math:MathematicalExpression` that participates in
   checking, normalization, evaluation, or content projection is structured; it is not represented
   only by a string literal.
2. **A display string is a rendering.** A string may exist only as a `math:ExpressionRendering`
   (`math:rendersAs`) of an AST, or as explicitly non-computable prose marked as such.
3. **Argument order is indexed, not list-ordered.** Application operand order is carried by
   `math:ArgumentSlot` individuals with an integer `math:slotIndex`, not by RDF list ordering —
   unless a generated projection explicitly needs a list, in which case the list is derived from the
   slots, never the source of truth.
4. **Slots are unique and, in strict mode, zero-based contiguous.** Within one
   `math:ApplicationExpression` the slot indexes are unique with no duplicates; strict canonical
   mode additionally requires them **zero-based and contiguous** with no gaps. The convention is
   fixed at zero-based — there is no "zero or one" optionality, because an optional base index would
   violate the slice's low-optionality posture and make two encodings of the same application
   non-identical.
5. **Every variable occurrence is bound or explicitly free.** Each `math:VariableOccurrence` is
   either bound by a `math:BindingExpression` (`math:bindsOccurrence`) or explicitly marked free
   (`math:freeVariableDeclaration`) with a declared type and domain context. There is no implicit
   free variable. The declaration/occurrence split (below) is what makes this checkable under
   nesting and shadowing.
6. **Every symbol reference resolves.** Each `math:SymbolReference` resolves to a local
   `math:MathematicalSymbol` or a declared external symbol reference; a dangling symbol is
   ill-formed.

### A worked example — matrix multiplication

`A · B` is an application of a `matmul` operator to two ordered operands, each a symbol reference,
with a rendering carried separately:

```ttl
ex:matMulAB
    a math:ApplicationExpression ;
    math:operator ex:matmulSymbol ;
    math:argumentSlot ex:matMulAB_s0 , ex:matMulAB_s1 ;
    math:expressionType ex:matrixExpressionType ;
    math:rendersAs ex:matMulAB_mathml .

ex:matMulAB_s0
    a math:ArgumentSlot ;
    math:slotIndex 0 ;
    math:slotExpression ex:refA .

ex:matMulAB_s1
    a math:ArgumentSlot ;
    math:slotIndex 1 ;
    math:slotExpression ex:refB .

ex:refA a math:SymbolReference ; math:hasMathematicalSymbol ex:symbolA .
ex:refB a math:SymbolReference ; math:hasMathematicalSymbol ex:symbolB .

ex:matMulAB_mathml
    a math:ExpressionRendering ;
    math:parseSource "<mrow><mi>A</mi><mo>&#x22C5;</mo><mi>B</mi></mrow>" .
```

Operand order (`A` then `B`) is explicit and non-commutative-safe: swapping the slot expressions is
a different expression, and the shape gate forbids the two slots from sharing index `0`. The MathML
is a rendering hung off `math:rendersAs`, not the identity of the product.

### Normalization and declared equivalence

Two expressions are equal by *declared normalization*, never by string coincidence. A
`math:normalForm` links an expression to its canonical form under a named normalization procedure,
and an equivalence between two expressions is a claim held by a vantage — *who or what* declared the
normalization is recorded, because "these formulas are the same" is an inferential act, not a
lexical fact. Structural (α-)identity — same operator, same slot-indexed operands, same binding
structure up to bound-variable renaming — is the finest equality; coarser equalities (commutative,
algebraic, semantic) are declared normalizations layered on top and attributed.

### Variable declaration and occurrence

Binding is modeled with an explicit **declaration/occurrence split**, because nested binders,
shadowing, α-equivalence, and capture-avoiding substitution are unmanageable otherwise. A
`math:VariableDeclaration` (introduced by a `math:BindingExpression` through
`math:bindsVariable`, or standing alone as a `math:freeVariableDeclaration` with type/domain) is
the *identity* of a variable within a scope; a `math:VariableOccurrence` (`math:declaredVariable`
pointing at its declaration, `math:occursInScope` naming its scope) is a *use site*. A binder binds
occurrences (`math:bindsOccurrence`), not glyphs, so `∑ᵢ (xᵢ + ∑ᵢ yᵢ)` — where the inner `i`
shadows the outer — has two distinct declarations and each occurrence resolves to exactly one,
making α-equivalence a graph isomorphism over declarations rather than a string comparison.

### Number literals, operator signatures, and closed-form functions

The AST leaves and operators carry enough structure to state a *parameterized closed-form
function* — a curve such as `T(x) = A·x⁻ᵖ − B·(1−x)⁻q + C` — natively and exactly, as a general
facility supporting **any** curve rather than a hardwired one.

A **numeric-literal leaf** is a `math:NumberLiteral`: the expression node that denotes a specific
`math:Number` — the `1` in `(1 − x)`, the constant `2` in `2·x`, an exponent. It carries its value
through `math:literalValue`, which points at a well-formed `math:Number` situated in a number system
(the grounding is [`MATHEMATICS-NUMBERS-AND-SETS.md`](MATHEMATICS-NUMBERS-AND-SETS.md)). A
`math:NumberLiteral` is *not* a bare RDF literal — because it is a `math:MathematicalExpression`, it
can fill a `math:slotExpression`, which a raw literal cannot — and it is *not* a
`math:MathematicalConstant` such as π: a constant is a named exact number object, while a
`NumberLiteral` is a syntactic leaf of a formula that *names* the number it denotes.

The arithmetic operators gain a **signature over number systems**. `math:Negation` is minted as the
unary additive-inverse operator (`−x`), so `x⁻ᵖ` is `Exponentiation(x, Negation(p))` — a unary
minus, distinct from the binary `math:Subtraction`. Every `math:ArithmeticOperation` now names the
number system its operands are drawn from through `math:operatorDomain` and the system its result
lies in through `math:operatorCodomain`, each a `math:NumberSystem`. An operator is not a bare glyph:
addition and its field siblings are framed `ℝ → ℝ`, an even root or a real logarithm lands in `ℂ`
(`ℝ → ℂ`, stated honestly rather than pretending the result stays real), and `math:Negation` ranges
over the signed extended real line `ℝ̄ → ℝ̄` so `−(+∞) = −∞` is data, not an undefined edge. The
signature is **required on every operator** — an operator missing either half is a
`math:UnframedOperator` — so the operator filling a `math:operator` slot always says what system it
computes over.

A **closed-form function** is a `math:ClosedFormFunction`, a subclass of `math:Function` (so it
inherits the obligation to declare its `math:domain` and `math:codomain`, an unframed one being
`math:UnframedFunction`). It gives a curve by an explicit `math:definingExpression` — the body AST —
in one formal argument, and it separates that argument from its fitted parameters: the abstraction
variable `x` is named through `math:formalArgument` and each tunable parameter (`A`, `B`, `C`, `p`,
`q`) through `math:functionParameter`. So in `T(x)` the variable the function is *of* and the
parameters it is *tuned by* are distinct declarations, never conflated. A closed form missing its
body or its formal argument is `math:UnboundClosedForm`; its `math:functionParameter`s are `0..n`
and unconstrained. Both the argument and the parameters enter the body as `math:VariableExpression`
leaves resolving, through `math:variableOccurrence`/`math:declaredVariable`, to the function's
`math:formalArgument` or one of its `math:functionParameter`s — the leaf-to-declaration linkage that
keeps the body honest. Because that linkage is a transitive traversal over arbitrary AST depth, it
exceeds the guarded fragment of the declarative gate and is checked structurally over the authored
worked example rather than by a general runtime constraint.

This is the general algebra: `math:Addition`, `math:Multiplication`, `math:Negation`,
`math:Exponentiation` and the rest compose over `math:NumberLiteral` leaves and
`math:VariableExpression` leaves into a `math:ClosedFormFunction`'s `math:definingExpression`,
stating any parameterized closed form exactly. A concrete curve such as `T(x)` is authored as an
instance of this facility, not as a bespoke class.

### Denotation and lowering into `logic:`

The expression AST is **not** a second logic. `logic:` already owns a typed, full first-order IR —
terms, formulas, types, predicates, and proof objects
(`slices/grounding/logic/design/LOGIC-IR.md`). The mathematical AST must therefore declare, for any
expression that crosses into reasoning, *what it denotes* and *how it lowers*, so the two layers
compose instead of duplicating.

Every `math:MathematicalExpression` carries a `math:denotationKind` — one of:

```text
MathematicalExpression
  ├─ denotes a mathematical object / value      →  logic: term
  ├─ denotes a proposition / formula            →  logic: formula
  ├─ denotes a type / class / sort              →  logic: type
  ├─ denotes a function / relation / operator   →  logic: predicate or function symbol
  └─ denotes a proof / proof-term               →  logic: proof object
```

The lowering is explicit and preservation-judged: `math:compilesToLogicFormula`,
`math:compilesToLogicTerm`, and `math:compilesToLogicType` link an expression to the `logic:` IR
node it lowers to, and `math:logicLoweringPreservation` records the polarity in the shared
`logic:preservationKind` vocabulary ([`MATHEMATICS-CONFORMANCE.md`](MATHEMATICS-CONFORMANCE.md)).

> **Hard rule.** A truth-valued mathematical expression does **not** become a `logic:` formula
> unless its `math:denotationKind` and its lowering preservation are declared. Silent promotion of
> a mathematical expression to a logical assertion is forbidden — an equation is a denotable object
> until something declares it a proposition and lowers it, with recorded preservation, into the
> reasoning layer.

This is the seam that keeps the mathematics slice from re-implementing the IR: the AST is the
*mathematical* surface, `logic:` is the *reasoning* surface, and the lowering is a declared,
preservation-judged map between them — the same discipline every other `logic:` projection follows.

## The object and structure layer

The object layer names mathematical objects and structures at a granularity useful for description,
alignment, and constraint — deliberately *not* at the granularity of a theorem prover encoded in
OWL. Formal consequences are the business of `logic:` and future solver profiles; the ontology
source names structure and states constraints.

Core classes span four families:

- **Elements and collections** — `math:Number`, `math:Set`, `math:Tuple`, `math:Vector`,
  `math:Matrix`, `math:Tensor`, `math:Sequence`.
- **Maps and operations** — `math:Function`, `math:Relation`, `math:Operation`.
- **Spaces and structures** — `math:Space`, `math:MetricSpace`, `math:TopologicalSpace`,
  `math:MeasureSpace`, `math:VectorSpace`, `math:AlgebraicStructure`, `math:Group`,
  `math:Ring`, `math:Field`, `math:GraphMathematicalObject`.
- **Category-theoretic objects** — `math:Category`, `math:Morphism`, `math:Functor`,
  `math:NaturalTransformation`.

Core properties: `math:hasElement`, `math:hasMember`, `math:hasDimension`, `math:hasShape`,
`math:domain`, `math:codomain`, `math:arity`, `math:operationOn`, `math:preservesStructure`,
`math:sourceObject`, `math:targetObject`, and `math:composesWith`.

Design posture, stated as three rules:

- **No per-object subclass explosion.** Do not mint a subclass for every named concept; prefer
  first-class individuals for named constants, structures, distributions, and symbols, introducing
  class-level terms only where a genuine class is needed.
- **Structure-preserving maps are first-class.** A morphism carries `math:sourceObject`/
  `math:targetObject` and, where relevant, `math:preservesStructure`; a functor and a natural
  transformation are objects, not annotations, so category-theoretic alignment (used elsewhere in
  the project under `docs/APPLIED_CATEGORY_THEORY/`) has real referents to point at.
- **Shape and dimension are data.** A matrix's shape and a tensor's dimensions are carried by
  `math:hasShape`/`math:hasDimension`, so that expression-level dimension-compatibility
  constraints (matrix multiplication's inner-dimension match) are checkable against the operands.

## The statement/proof/theory layer

The statement layer describes mathematical knowledge artifacts and their epistemic status without
confusing an *asserted* statement with a *validated* theorem.

Core classes: `math:Axiom`, `math:Theorem`, `math:Lemma`, `math:Corollary`, `math:Conjecture`,
`math:DefinitionStatement`, `math:Proof`, `math:ProofStep`, `math:ProofMethod`,
`math:FormalVerificationResult`, and `math:Counterexample`.

Core properties: `math:hasPremise`, `math:hasConclusion`, `math:provesStatement`,
`math:dependsOnAxiom`, `math:usesProofMethod`, `math:verifiedByEngine`,
`math:verificationResult`, and `math:hasCounterexample`.

The load-bearing rule:

> **A theorem label is not a truth bit.** A theorem claim is held from a vantage, under a theory and
> context, with a proof or an external warrant. A proof checker's success is itself an
> observation/verification claim with provenance.

So a theorem is not "true" by virtue of the `math:Theorem` type. It states a conclusion under
premises (`math:hasPremise`/`math:hasConclusion`), depends on axioms (`math:dependsOnAxiom`)
under a versioned theory, and is *proved* by a `math:Proof` that uses a `math:ProofMethod`. A
`math:Counterexample` is likewise a first-class object attached by `math:hasCounterexample`, so a
refuted conjecture carries its refutation structurally.

### Status is a role under a theory, not a global class

Because status is theory- and standpoint-relative, the **canonical** carrier of "theorem-hood" is a
role, not a bare class assertion. A `math:MathematicalStatement` carries a `math:statementRole`
(theorem, lemma, corollary, conjecture, axiom, definition) that holds `math:roleInTheory` a named,
versioned theory — so "this statement is a theorem in Euclidean geometry v2" is a claim scoped to a
theory, not an unconditional global fact. The named classes `math:Theorem`, `math:Lemma`, and the
rest are retained as **convenience projections** (shaped subclasses generated from the role for
consumers that want a flat type), never as the canonical bearer of truth. This prevents the failure
mode where a consumer reads `a math:Theorem` as "proved, everywhere, unconditionally".

### Process, result, and claim are three objects

Verification splits cleanly into the process that ran, the result it produced, and the claim held
from a vantage — the same separation the statistics layer enforces
([`MATHEMATICS-STATISTICS.md`](MATHEMATICS-STATISTICS.md)) and the conformance charter gates
([`MATHEMATICS-CONFORMANCE.md`](MATHEMATICS-CONFORMANCE.md)). A `math:ProofCheckActivity` (a
`gmeow:Activity`) is the occurrent run; a `math:FormalVerificationResult` is the structured result
object; and a `gmeow:Observation` is the held verdict, with the checker as `gmeow:vantage`. None is
typed as another.

### A worked example — a theorem/proof claim

```ttl
ex:pythagoreanStatement
    a math:MathematicalStatement ;
    math:statementRole math:roleTheorem ;
    math:roleInTheory ex:euclideanGeometryV2 ;
    math:hasConclusion ex:pythagoreanConclusionExpr ;
    math:dependsOnAxiom ex:euclidPostulate5 .

ex:pythagoreanProof
    a math:Proof ;
    math:provesStatement ex:pythagoreanStatement ;
    math:usesProofMethod ex:proofMethodDirect .

# process
ex:coqCheckRun
    a math:ProofCheckActivity , gmeow:Activity ;
    math:usedProof ex:pythagoreanProof ;
    gmeow:usedSoftwareCommit ex:coqEngineV8 .

# result object
ex:coqCheckResult
    a math:FormalVerificationResult ;
    math:verificationResult math:verificationPassed .

# held claim
ex:coqCheckObservation
    a gmeow:Observation ;
    gmeow:vantage ex:coqEngineV8 ;
    gmeow:observedFeature ex:pythagoreanProof ;
    gmeow:observationResult ex:coqCheckResult ;
    gmeow:wasGeneratedBy ex:coqCheckRun ;
    gmeow:observationType gmeow:observationTypeDerived .
```

The statement's theorem-hood is a role scoped to `ex:euclideanGeometryV2`; the check that ran
(`ex:coqCheckRun`) is an activity; the verdict (`ex:coqCheckResult`) is a result object; and the
held claim (`ex:coqCheckObservation`) is an observation with the engine as vantage. "Verified"
always answers *by whom, over what, through which run, with what result* — and none of the four
nodes is typed as another.

This statement-role layer is **materialized** in the slice: `math:MathematicalStatement`,
`math:statementRole`, the closed `math:StatementRole` value class and its six role individuals,
`math:roleInTheory`, the convenience projections (`math:Theorem`, `math:Lemma`, `math:Corollary`,
`math:Conjecture`, `math:DefinitionStatement`), `math:Counterexample`/`math:hasCounterexample`,
`math:ProofMethod`/`math:usesProofMethod`, the three-object verification split
(`math:ProofCheckActivity` process, `math:FormalVerificationResult` result object, and the
`gmeow:Observation` held claim) with `math:verifiedByEngine`/`math:verificationResult` and the
closed `math:VerificationOutcome` value class, and the strictly one-way math→logic bridge
`math:conjectureUnderTest` all live in `module.ttl`. The theorem-is-not-a-truth-bit gate is
`math:TheoremGroundingShape` (raising `math:UngroundedTheoremClaim`) and the result-grounding gate
is `math:FormalVerificationResultShape` (raising `math:UngroundedVerificationResult`), both with
negative fixtures in `tests/example-conformance.ttl`; the worked example above is
`examples/theorem-proof-claim.ttl`, and competency question 5 is pinned by
`queries/competency/theorem-proof-theory-engine.rq`.

## Shape and lint gates

The core ships with strict `shapes.ttl` and source-lint gates. The expression gates, verbatim to
the manifesto's doctrine:

- An `ApplicationExpression` has exactly one operator.
- Each `ArgumentSlot` has exactly one index and exactly one expression.
- Slot indexes are unique per application; strict canonical mode requires them contiguous.
- A `VariableExpression` is bound or explicitly declared free; a free variable declares type/domain
  context.
- A `SymbolReference` resolves locally or through a declared external symbol reference.
- A computable expression is not represented only by a string literal.

The statement gates enforce the theorem-is-not-a-truth-bit rule: a `math:Theorem` claim carries a
theory context and either a `math:Proof` or a declared external warrant; a
`math:FormalVerificationResult` is grounded as an observation with a vantage, never asserted as a
free-floating truth value.

## Competency questions

The core is accepted only when it can answer these structurally
([`MATHEMATICS.md`](MATHEMATICS.md) records the full set; the core owns the expression/theory half):

1. What symbols does this formula reference, and which theory or context defines each symbol?
2. Which variables are bound, and which are free — and what type/domain does each free variable
   carry?
3. What is the operator and the argument order of this expression?
4. Which formulas are equivalent by declared normalization, and who or what declared that
   equivalence?
5. Which theorem does this proof claim to prove, under which axioms and which theory version — and
   which engine, if any, verified it, with what result?

## Projection note

Every artifact named here has lossy lowerings — MathML and OpenMath for expressions and symbols,
Wikidata for concept identity, OMDoc/MMT for theories. Each lowering declares its unsupported
constructs and its preservation polarity in the loss ledger; the projection contract is in
[`MATHEMATICS-PROJECTIONS.md`](MATHEMATICS-PROJECTIONS.md). Canonical computable content is always
the GMEOW expression AST; a MathML tree is canonical identity only when a formula was ingested at
that fidelity and marked as such.
