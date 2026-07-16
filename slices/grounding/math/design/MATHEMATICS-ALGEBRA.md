<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Mathematics — Algebra: Structures, Symmetry, and Homomorphisms

> The **algebra charter** of the GMEOW Mathematics design set: the algebraic-structure hierarchy
> (groups → rings → fields → modules → algebras), structure-preserving maps, Lie theory and root
> systems, and the homomorphism as a first-class law. It carries two of the layer's flagships — the
> **symmetry groups of E8** and **how homomorphic encryption works** — because both are, at root,
> structure and the maps that preserve it. It deepens the object layer of
> [`MATHEMATICS-EXPRESSIONS.md`](MATHEMATICS-EXPRESSIONS.md), aligns to Lean mathlib and Wikidata
> ([`MATHEMATICS-REFERENCES.md`](MATHEMATICS-REFERENCES.md)), and gates through
> [`MATHEMATICS-CONFORMANCE.md`](MATHEMATICS-CONFORMANCE.md).
>
> **Reading this charter.** The declarative present tense is normative: "X is" means a conforming
> realization implements X, established by the slice's canonical `module.ttl` axioms and `logic:Constraint` records, competency queries, and the
> projection loss ledger.

## Purpose

Algebra is where the object layer's `math:Morphism` / `math:preservesStructure` primitives earn their
keep. The charter's discipline is that **a structure declares its operations and laws, and a map
between structures declares what it preserves.** A group is not a labelled set; it is a set with an
operation satisfying stated axioms. A homomorphism is not an annotation; it is a map with a preserved
law that lowers to a `logic:` formula. This is the through-line that makes E8's symmetry and
homomorphic encryption the *same kind of object*.

## The algebraic-structure hierarchy

Core classes: `math:AlgebraicStructure`, `math:Magma`, `math:Semigroup`, `math:Monoid`,
`math:Group`, `math:AbelianGroup`, `math:Ring`, `math:CommutativeRing`, `math:Field`,
`math:Module`, `math:VectorSpace`, `math:Algebra`, `math:PolynomialRing`, and `math:Ideal`.

Core properties: `math:underlyingSet`, `math:structureOperation`, `math:identityElement`,
`math:inverseOperation`, `math:satisfiesAxiom`, and `math:substructureOf`.

A `math:AlgebraicStructure` names its underlying set, its operation(s), identity/inverse where they
exist, and the axioms it satisfies (`math:satisfiesAxiom`, each an `math:Axiom` from the statement
layer, lowered to a `logic:` formula). The hierarchy is subsumption: an `math:AbelianGroup` is a
`math:Group` with a commutativity axiom; a `math:Field` is a `math:CommutativeRing` whose non-zero
elements form a group under multiplication. Structure is declared and checkable — a `math:Group`
asserted without an operation and identity is ill-formed.

## Structure-preserving maps

Core classes: `math:Homomorphism` (a specialization of `math:Morphism`), with
`math:GroupHomomorphism`, `math:RingHomomorphism`, `math:Isomorphism`, `math:Automorphism`, and
`math:AutomorphismGroup`.

Core properties: `math:preservedOperation`, `math:kernel`, `math:image`, and `math:preservationLaw`.

A `math:Homomorphism` names the operation it preserves and its **preservation law** as a `logic:`
formula: a group homomorphism φ carries `math:preservationLaw` ⟦∀a,b: φ(a·b) = φ(a)·φ(b)⟧, a ring
homomorphism carries both the additive and multiplicative laws. `math:kernel` and `math:image` are
first-class. An `math:Automorphism` is an isomorphism of a structure with itself, and the
`math:AutomorphismGroup` — the symmetries of a structure — is itself a group. **A symmetry group is
an automorphism group**; that identity is what makes E8 expressible.

## Lie theory and root systems — the E8 flagship

Core classes: `math:LieGroup`, `math:LieAlgebra`, `math:RootSystem`, `math:SimpleRoot`,
`math:CartanMatrix`, `math:DynkinDiagram`, `math:WeylGroup`, `math:Lattice`, and
`math:GroupRepresentation`.

Core properties: `math:hasRootSystem`, `math:hasSimpleRoot`, `math:cartanMatrix`,
`math:dynkinDiagram`, `math:weylGroup`, `math:rootSystemRank`, and `math:representationOf`.

A `math:LieGroup` and its `math:LieAlgebra` carry a `math:RootSystem` — for E8, 240 roots in an
8-dimensional space with a rank-8 `math:CartanMatrix`, a `math:DynkinDiagram`, and a `math:WeylGroup`
(order 696,729,600) that is the symmetry group of the root system. The E8 lattice is a
`math:Lattice`; representations are `math:GroupRepresentation`s. The depth is **authored** (no
external ontology holds it), **aligned** to Lean mathlib's root-system and Lie-algebra formalizations
and to a Wikidata QID for concept identity, and **cited** to the ATLAS/GAP for group data
([`MATHEMATICS-REFERENCES.md`](MATHEMATICS-REFERENCES.md)).

> **Flagship — E8.** "The symmetry groups of E8" is answerable when the layer can name E8's root
> system, simple roots, Cartan matrix, Dynkin diagram, and Weyl group, and model each symmetry group
> as an automorphism group preserving that structure — with concept identity anchored to Wikidata and
> structure aligned to mathlib.

## Homomorphic encryption — the HE flagship

Homomorphic encryption is a `math:RingHomomorphism` (partial or full) whose preservation law is
exactly the property that makes it useful. It is expressed with the machinery above plus the lattice
grounding:

Core classes: `math:EncryptionScheme`, `math:HomomorphicEncryptionScheme`,
`math:LatticeHardnessAssumption` (LWE/RLWE), with `math:PolynomialRing`/`math:Ideal` from the
hierarchy; and the scheme's operations (`math:encryptOperation`, `math:evaluateOperation`,
`math:decryptOperation`) as `gmeow:Activity`-typed processes.

Core properties: `math:homomorphicOver`, `math:noiseModel`, `math:securityAssumption`, and
`math:preservationLaw`.

The homomorphic property `Dec(E(a) ⊗ E(b)) = a ⊕ b` is a `math:preservationLaw` — a `logic:` formula
grounded in the ring/lattice structure the scheme operates over. The scheme names the operation it is
homomorphic over (`math:homomorphicOver`), its hardness assumption
(`math:securityAssumption` → `math:LatticeHardnessAssumption`), and its noise model; encryption,
evaluation, and decryption are processes (the process/result/claim split,
[`MATHEMATICS-CONFORMANCE.md`](MATHEMATICS-CONFORMANCE.md)). No external ontology exists — GMEOW
authors it, and it dogfoods GMEOW's own privacy reasoning.

> **Flagship — homomorphic encryption.** "How homomorphic encryption works" is answerable when the
> scheme is a ring homomorphism whose homomorphic law is a `logic:` formula over declared ring/lattice
> structure, whose hardness assumption and noise model are named, and whose encrypt/evaluate/decrypt
> steps are activities — the purest exercise of the one-way `math:` → `logic:` bridge.

## A worked example — a group homomorphism with its preserved law

```ttl
ex:detHom
    a math:GroupHomomorphism ;
    math:domain ex:GL2R ;                      # (ℝ², ×) general linear group
    math:codomain ex:RstarMultiplicative ;     # (ℝ*, ×)
    math:preservedOperation ex:matrixMultiplication ;
    math:preservationLaw ex:detProductLaw ;    # ⟦∀A,B: det(A·B) = det(A)·det(B)⟧ as a logic: formula
    math:kernel ex:SL2R .                       # the special linear group
```

`det` is a homomorphism because it preserves multiplication; its kernel (matrices of determinant 1)
is itself a group. The same shape — a map, a preserved operation, a law — is what E8's Weyl action
and a homomorphic-encryption scheme instantiate.

## Shape and lint gates

Catalogued in [`MATHEMATICS-CONFORMANCE.md`](MATHEMATICS-CONFORMANCE.md): a structure declares its
operation, identity, and axioms; a homomorphism declares its preserved operation and preservation
law (a `logic:` formula, not a string); a Lie group/algebra declares its root system, Cartan matrix,
and Weyl group; and a homomorphic-encryption scheme declares its homomorphic operation, hardness
assumption, and noise model, with its steps as activities.

## Competency questions

1. What operation and axioms define this structure, and where does it sit in the hierarchy?
2. What operation does this homomorphism preserve, what is its preservation law, and what is its
   kernel?
3. What are E8's root system, simple roots, Cartan matrix, Dynkin diagram, and Weyl group — and which
   symmetry group is which automorphism group?
4. Which ring operation is this encryption scheme homomorphic over, under what hardness assumption and
   noise model?
5. Which structures are isomorphic, and by which declared isomorphism?
