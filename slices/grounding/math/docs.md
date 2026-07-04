<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Mathematics — the `math:` grounding layer

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/math` · **tier: core**
> The grounding vocabulary of the primitive mathematical objects — numbers, sets, and functions —
> that every other mathematical charter quantifies, indexes, maps, and measures over. A peer of the
> `logic:` reasoning layer.

This slice is the home of **GMEOW Mathematics (`math:`)**. Its single discipline is that
**exactness is explicit**: a number declares the system it lives in, an exact value and an
approximation of it are distinct objects, and a named constant is an exact individual anchored to an
external authority rather than a decimal literal. A set is given extensionally by its elements or
intensionally by a condition that denotes a `logic:` formula — never both silently — and a function
declares its domain and codomain.

The bedrock realized here covers four regions:

- **Number systems and exactness** — the containment tower ℕ ⊂ ℤ ⊂ ℚ ⊂ ℝ ⊂ ℂ
  (`math:NaturalNumber` … `math:ComplexNumber`), the algebraic/transcendental cross-cut, named
  constants (`math:MathematicalConstant`, with `math:pi`, `math:eulerNumber`,
  `math:eulerMascheroni`), and the exact/approximate seam (`math:ApproximateValue`,
  `math:approximates`, `math:approximationError`, `math:numericDatatype` subsuming IEEE-754
  `xsd:double`/`xsd:float`).
- **Arithmetic** — `math:ArithmeticOperation` and the operator individuals `math:Addition`,
  `math:Multiplication`, `math:Exponentiation` and their inverses, applied inside a
  `math:ApplicationExpression`.
- **Sets** — `math:Set` and its extensional (`math:FiniteSet`, `math:hasElement`) and intensional
  (`math:SetBuilderExpression`, `math:memberCondition`) forms, set operations
  (`math:SetOperation`, `math:Complement`, `math:PowerSet`, `math:CartesianProduct`), and cardinality
  (`math:Cardinality`, `math:cardinalityFinite`, `math:alephNull`, `math:continuum`).
- **Relations and functions** — `math:Relation`, `math:Function` with `math:domain`/`math:codomain`/
  `math:image`, the declared boolean bearers `math:isInjective`/`math:isSurjective`/`math:isBijective`
  (of which `math:InjectiveFunction`/`math:SurjectiveFunction`/`math:BijectiveFunction` are convenience
  projections), and `math:PartialFunction`, `math:FunctionSpace`, `math:functionComposition`,
  `math:inverseFunction`.

The object-layer parents (`math:Set`, `math:Function`, `math:Operation`, `math:Relation`,
`math:MathematicalExpression`) are minted here as the ground the bedrock builds on; the full
expression grammar and proof/theory layer belong to the mathematical-core surface.

Every well-formedness violation is a typed, queryable object: `math:MathConformanceFailure` and its
subclasses, each raised by a SHACL shape that names it through `math:enforcesFailureClass`. The
preservation-polarity vocabulary is reused verbatim from the `logic:` loss ledger, and each named
constant and number system is anchored to Wikidata (with OEIS locators for the constants) as a
`gmeow:TermEquivalence` alignment.

## The design set

The normative design is a set of charters under [`design/`](./design/):

| Document | Genre | Contents |
| --- | --- | --- |
| [`design/MATHEMATICS.md`](./design/MATHEMATICS.md) | manifesto | vision, doctrine, the grounding-layer posture |
| [`design/MATHEMATICS-NUMBERS-AND-SETS.md`](./design/MATHEMATICS-NUMBERS-AND-SETS.md) | charter | the bedrock: number systems and exactness, arithmetic, sets, relations and functions |
| [`design/MATHEMATICS-EXPRESSIONS.md`](./design/MATHEMATICS-EXPRESSIONS.md) | charter | the mathematical core: the reference, expression-AST, object, and statement/proof layers |
| [`design/MATHEMATICS-CONFORMANCE.md`](./design/MATHEMATICS-CONFORMANCE.md) | enforcement | the gate matrix — each hard rule, its gate kind, and the `math:` failure class it raises |
| [`design/MATHEMATICS-REFERENCES.md`](./design/MATHEMATICS-REFERENCES.md) | references | the external-authority landscape (OpenMath, Wikidata, OEIS, DLMF, QUDT, xsd) and the anchoring posture |

## The gates

Each bedrock hard rule is enforced by a SHACL shape in [`shapes.ttl`](./shapes.ttl) and demonstrated
by a positive fixture (it validates) and a negative counter-example (it raises exactly the named
failure) under the native slicetest harness:

| Rule | Shape | Failure class |
| --- | --- | --- |
| A number declares its number system | `math:NumberSituatedShape` | `math:UnsituatedNumber` |
| An approximation names what it approximates and its error | `math:ApproximateValueShape` | `math:ExactApproximateConflation` |
| A named constant is an exact individual, not a decimal | `math:MathematicalConstantShape` | `math:ConstantAsDecimalLiteral` |
| An intensional condition denotes a `logic:` formula | `math:IntensionalSetShape` | `math:StringOnlyMemberCondition` |
| A complement names its ambient set | `math:ComplementShape` | `math:UnqualifiedComplement` |
| A set is extensional or intensional, not both | `math:SetExtentShape` | `math:AmbiguousSetExtent` |
| A function declares its domain and codomain | `math:FunctionFramingShape` | `math:UnframedFunction` |

The five competency questions in [`queries/competency/`](./queries/competency/) demonstrate that the
term surface answers, over the fixtures, what number system a number is in and whether it is exact,
whether a set is extensional or intensional and its condition, a function's domain/codomain/image and
its injectivity/surjectivity/bijectivity, a set's cardinality, and which named constants appear with
their external anchors.

## Algebra — structures, symmetry, and homomorphisms

The algebra charter ([`design/MATHEMATICS-ALGEBRA.md`](./design/MATHEMATICS-ALGEBRA.md)) deepens the
object layer with four regions, on one discipline: **a structure declares its operations and laws, and
a map between structures declares what it preserves.** `math:` expresses; `logic:` owns reasoning —
every axiom and preservation law is a real `logic:Formula` first-order AST the reasoner consumes, not
an opaque string.

- **The algebraic-structure hierarchy** — `math:AlgebraicStructure` ⊐ `math:Magma` ⊐ `math:Semigroup`
  ⊐ `math:Monoid` ⊐ `math:Group` ⊐ `math:AbelianGroup`; `math:Ring` ⊐ `math:CommutativeRing` ⊐
  `math:Field`; `math:Module` ⊐ `math:VectorSpace`; `math:PolynomialRing`, `math:Ideal` — each naming
  its `math:underlyingSet`, `math:structureOperation`, and the axioms it `math:satisfiesAxiom`. The
  reusable axiom library ([`examples/algebra-axioms.ttl`](./examples/algebra-axioms.ttl)) authors
  associativity, commutativity, identity, inverse, and distributivity as `logic:Formula` ASTs.
- **Structure-preserving maps** — `math:Homomorphism` (⊑ `math:Morphism`) with `math:GroupHomomorphism`,
  `math:RingHomomorphism`, `math:Isomorphism`, `math:Automorphism`, and `math:AutomorphismGroup` (⊑
  `math:Group`), carrying `math:preservedOperation`, `math:kernel`, and a `math:preservationLaw`; the
  first-isomorphism triple (`math:Quotient`, `math:normalSubgroupOf`). The determinant example authors
  `det(A·B) = det(A)·det(B)` as a `logic:Formula`, with `GL₂/SL₂ ≅ ℝ*`.
- **Lie theory and the E8 flagship** — `math:LieGroup`, `math:LieAlgebra`, `math:RootSystem`,
  `math:CartanMatrix`, `math:DynkinDiagram`, `math:WeylGroup` (⊑ `math:AutomorphismGroup`), `math:Lattice`,
  `math:GroupRepresentation`, and `math:GroupAction`/`math:actsOn`. E8 is authored with 240 roots, rank
  8, and a Weyl group of order 696,729,600 modelled as the automorphism group acting on the roots — the
  numbers pinned by an exact-match competency question so wrong data fails the gate.
- **Homomorphic encryption (flagship 2) and secret sharing** — `math:HomomorphicEncryptionScheme` (a
  `math:RingHomomorphism`) with `math:homomorphicOver`, `math:securityAssumption` (LWE/RLWE), and
  `math:noiseModel`; encrypt/evaluate/decrypt as `gmeow:Activity` processes. `Dec(E(a) ⊗ E(b)) = a ⊕ b`
  is a `logic:Formula` over the declared ring structure — the purest exercise of the one-way
  `math:` → `logic:` bridge. Shamir secret sharing reuses the field / polynomial-ring machinery.

Algebra also **dogfoods** GMEOW: `math:Lattice math:formalizes gmeow:tuningSystemJustIntonation` (a
prime-limit just-intonation tuning is a free abelian lattice), the one-way `math:formalizes` bridge
(Principle 19) making algebra grounding for the ontology. The algebra classes align to Wikidata
(`skos:exactMatch`, QIDs curl-validated) and to Lean **mathlib** by reference (a new
`gmeow-mathlib.sssom.tsv` lane, `skos:relatedMatch`, URLs curl-validated).

The algebra gates:

| Rule | Shape | Failure class |
| --- | --- | --- |
| A structure declares its carrier, operation, and axioms | `math:AlgebraicStructureShape` | `math:IncompleteAlgebraicStructure` |
| A homomorphism declares its preserved operation and law | `math:HomomorphismShape` | `math:UnderspecifiedHomomorphism` |
| A preservation law denotes a `logic:` formula, not a string | `math:PreservationLawShape` | `math:StringOnlyPreservationLaw` |
| A Lie group declares its root system | `math:LieStructureShape` | `math:IncompleteLieStructure` |
| A root system declares its Cartan matrix, Weyl group, and rank | `math:RootSystemShape` | `math:IncompleteLieStructure` |
| An HE scheme declares its homomorphic operation, hardness, and noise | `math:HomomorphicEncryptionSchemeShape` | `math:UnderspecifiedEncryptionScheme` |
