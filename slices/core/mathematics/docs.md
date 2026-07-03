<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Mathematics — the `math:` grounding layer

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/mathematics` · **tier: core**
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
