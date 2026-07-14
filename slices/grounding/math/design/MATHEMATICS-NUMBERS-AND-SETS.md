<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Mathematics — Numbers, Sets, and Functions

> The **bedrock charter** of the GMEOW Mathematics design set: number systems and exactness,
> arithmetic, sets and their construction, and relations and functions. A grounding layer must be
> complete at the bottom before it reaches E8 or PCA, and this charter is that bottom — the objects
> every other charter quantifies, indexes, maps, and measures over. It builds on the object layer of
> [`MATHEMATICS-EXPRESSIONS.md`](MATHEMATICS-EXPRESSIONS.md); its gates are in
> [`MATHEMATICS-CONFORMANCE.md`](MATHEMATICS-CONFORMANCE.md) and its external anchors in
> [`MATHEMATICS-REFERENCES.md`](MATHEMATICS-REFERENCES.md).
>
> **Reading this charter.** The declarative present tense is normative: "X is" means a conforming
> realization implements X, established by the slice's `shapes.ttl`, competency queries, and the
> projection loss ledger.

## Purpose

Numbers, sets, and functions are the primitives the rest of the grounding layer is built from. The
charter's single discipline is **exactness is explicit**: a number knows whether it is an exact
element of a number system or a floating-point approximation of one; a set knows whether it is given
extensionally or by a defining condition; a function knows its domain and codomain. Nothing is left
to a bare RDF literal that silently loses which number system, which construction, or which map it
belongs to.

## Number systems and exactness

Core classes: `math:Number`, `math:NaturalNumber`, `math:Integer`, `math:RationalNumber`,
`math:RealNumber`, `math:ComplexNumber`, `math:AlgebraicNumber`, `math:TranscendentalNumber`,
`math:MathematicalConstant`, and `math:ApproximateValue`.

Core system individuals: the tower `math:naturalNumbers` ⊂ `math:integers` ⊂ `math:rationalNumbers`
⊂ `math:realNumbers` ⊂ `math:complexNumbers`, and — the reals with two signed poles adjoined —
`math:ExtendedRealLine`.

Core properties: `math:inNumberSystem`, `math:isExact`, `math:approximates`,
`math:approximationError`, `math:numericDatatype`, and `math:extendedRealValue`.

A `math:Number` declares its number system (`math:inNumberSystem`: ℕ ⊂ ℤ ⊂ ℚ ⊂ ℝ ⊂ ℂ, with the
algebraic/transcendental distinction inside ℝ/ℂ) and whether it is **exact** or an
**approximation**. An exact rational is the pair it denotes; π is a `math:MathematicalConstant` (an
exact `math:TranscendentalNumber`), *not* `3.14159`; and `3.14159` is a `math:ApproximateValue`
that `math:approximates` π with a stated `math:approximationError`. Floating-point values ground in
the canonical RDF datatypes `xsd:double`/`xsd:float` (IEEE-754), carried by `math:numericDatatype`
([`MATHEMATICS-REFERENCES.md`](MATHEMATICS-REFERENCES.md) — the `xsd` FP datatypes are subsumed, not
re-minted).

**Signed extended reals.** Some quantities run off the finite line: a limit diverging downward, an
infimum of a set with no lower bound, a σ-finite measure's infinite mass. The **signed extended real
line** `math:ExtendedRealLine` names ℝ̄ = ℝ ∪ {−∞, +∞} and adjoins two poles — `math:PositiveInfinity`
(already the codomain of `math:totalMass`) and its dual `math:NegativeInfinity` (the glyph "−∞") — to
the reals. It is **grounded through the same number-system machinery**, not a parallel one:
`math:realNumbers math:subsystemOf math:ExtendedRealLine`, and each pole is a member through
`math:inNumberSystem math:ExtendedRealLine`. A pole is a *definite point* on ℝ̄, never a finite number,
an error, or an undefined/NaN result (that ±∞ ∓ ±∞ or 0·∞ would give). A **signed-extended-real slot**
carries `math:extendedRealValue` — an `rdf:Property` (like `math:totalMass`) whose range honestly spans
both a finite numeric literal *and* a pole individual — and its value is a finite real of either sign,
`math:PositiveInfinity`, or `math:NegativeInfinity`; anything else is a `math:MalformedExtendedReal`.

> **Hard rules.**
>
> - A `math:Number` declares its number system; an unsituated number is ill-formed.
> - An exact number and an approximation of it are distinct objects; a `math:ApproximateValue` names
>   what it approximates and its error, and is never conflated with the exact value.
> - A named constant (π, e, γ) is an exact `math:MathematicalConstant` individual with a Wikidata
>   QID and, where applicable, an OEIS/DLMF link — never a decimal literal.
> - A signed-extended-real slot (`math:extendedRealValue`) holds a finite number of either sign,
>   `math:PositiveInfinity`, or `math:NegativeInfinity`; a pole written as text or any other node is
>   a `math:MalformedExtendedReal`.

## Arithmetic

Core classes: `math:ArithmeticOperation` (a specialization of `math:Operation`), with
`math:Addition`, `math:Multiplication`, `math:Exponentiation`, and their inverses as operator
individuals.

Arithmetic expressions are `math:ApplicationExpression`s over these operators
([`MATHEMATICS-EXPRESSIONS.md`](MATHEMATICS-EXPRESSIONS.md)) — `2 + 2` is an application of the
addition operator to two `math:Integer` literal expressions, not the string `"2+2"` and not the
evaluated `4` (evaluation is a solver handoff, [`MATHEMATICS-RUNTIME.md`](MATHEMATICS-RUNTIME.md)).
Operators carry their algebraic laws by reference to the algebra charter (commutativity,
associativity, identity, inverse), so `+` over ℤ knows it is the operation of an abelian group
([`MATHEMATICS-ALGEBRA.md`](MATHEMATICS-ALGEBRA.md)). The external anchor is OpenMath `arith1`/`nums1`.

## Sets and their construction

Core classes: `math:Set` (from the object layer), `math:FiniteSet`, `math:SetBuilderExpression`,
`math:Membership`, `math:SetOperation`, `math:PowerSet`, `math:CartesianProduct`, and
`math:Cardinality`.

Core properties: `math:hasElement`/`math:hasMember`, `math:memberCondition`, `math:subsetOf`,
`math:setOperationOn`, and `math:hasCardinality`.

A set is either **extensional** (a `math:FiniteSet` enumerating its elements via `math:hasElement`)
or **intensional** (a `math:SetBuilderExpression` whose `math:memberCondition` is a `logic:`
formula the AST denotes into — `{ x ∈ ℝ | x² < 2 }`). The two are distinct and a set is never both
without a declared equality. Set operations (union, intersection, complement, difference) are
`math:SetOperation` applications; `math:PowerSet` and `math:CartesianProduct` are first-class
constructors; and `math:Cardinality` (finite, ℵ₀, 𝔠, …) is carried, not assumed. The external
anchor is OpenMath `set1`.

> **Hard rule.** An intensional set names its member condition as a `logic:` formula (the
> denotation seam), never as an opaque string. A "complement" names its ambient set — an unqualified
> complement is ill-formed (the same discipline the geometry charter applies to subspace
> complements, [`MATHEMATICS-ANALYSIS-AND-GEOMETRY.md`](MATHEMATICS-ANALYSIS-AND-GEOMETRY.md)).

## Relations and functions

Core classes: `math:Relation`, `math:Function` (from the object layer), `math:FunctionSpace`, with
`math:InjectiveFunction`, `math:SurjectiveFunction`, `math:BijectiveFunction`, and
`math:PartialFunction` as declared properties/subclasses.

Core properties: `math:domain`, `math:codomain`, `math:image`, `math:isInjective`,
`math:isSurjective`, `math:functionComposition` (with its order-bearing refinements
`math:compositionOuter`/`math:compositionInner`), and `math:inverseFunction`.

A `math:Function` declares its `math:domain` and `math:codomain` — a function without them is
ill-formed. Injectivity, surjectivity, and bijectivity are declared properties (checkable, not
assumed); composition is a first-class `math:functionComposition` (associative, matching the
category-theoretic morphism composition of the object layer). Because composition is
non-commutative (`g ∘ f ≠ f ∘ g`), the composite→component relation is order-bearing: the
refinements `math:compositionOuter` (the function applied second) and `math:compositionInner`
(the function applied first) name which component is which, so the order is never lost, while the
order-agnostic super-property `math:functionComposition` still holds of either. A
`math:FunctionSpace` is the set
of functions between two objects, so higher-order constructions (an operator taking a function to a
function — a derivative, [`MATHEMATICS-ANALYSIS-AND-GEOMETRY.md`](MATHEMATICS-ANALYSIS-AND-GEOMETRY.md))
have a referent. A relation is a subset of a Cartesian product; a function is a relation with the
functional property declared. The external anchor is OpenMath `fns1`/`relation1`.

## A worked example — an exact rational and its approximation

```ttl
ex:oneThird
    a math:RationalNumber ;
    math:inNumberSystem math:rationalNumbers ;
    math:isExact true .

ex:oneThirdDecimal
    a math:ApproximateValue ;
    math:approximates ex:oneThird ;
    math:numericDatatype xsd:double ;
    math:quantityValue "0.3333333333333333"^^xsd:double ;   # note: gmeow:Quantity would frame this in a measurement
    math:approximationError ex:doubleRoundingError .
```

The exact rational and its `double` approximation are two objects; a consumer that needs the value
reads the approximation and *knows* it is one, with its error — the floating-point result never
masquerades as the exact number.

## Shape and lint gates

Catalogued in [`MATHEMATICS-CONFORMANCE.md`](MATHEMATICS-CONFORMANCE.md) (extending the gate matrix):
a number declares its system; an approximation names what it approximates and its error; a set is
extensional or intensional (not silently both); an intensional set's condition is a `logic:` formula,
not a string; a function declares domain and codomain; a complement names its ambient set.

## Competency questions

1. What number system does this number belong to, and is it exact or an approximation of what?
2. Is this set given extensionally or by a defining condition, and what is that condition as a
   `logic:` formula?
3. What are the domain, codomain, and image of this function, and is it injective/surjective/
   bijective?
4. What is the cardinality of this set?
5. Which named constants (π, e, …) appear, and what are their Wikidata/OEIS/DLMF anchors?
