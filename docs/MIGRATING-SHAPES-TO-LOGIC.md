<!-- SPDX-License-Identifier: AGPL-3.0-only -->

# Migrating hand-authored SHACL shapes to `logic:` projections

**Doctrine (Principle 17, [`slices/grounding/logic/design/LOGIC-VALIDATION.md`](../slices/grounding/logic/design/LOGIC-VALIDATION.md)).**
The only authored validation form is `logic:`. SHACL (and ShEx) exist **solely** as
ephemeral pipeline derivations of authored `logic:` nodes — a hand-authored
`slices/**/shapes.ttl` or root `shapes/*.ttl` is a *second source of truth* that is
unreasoned, ungoverned by the loss ledger, and free to drift. Every such shape must
become a projection of an authored `logic:` node.

The `slice-quality` **projection axis** flags each slice still shipping a hand-authored
`shapes.ttl` with `slice-quality.projection.hand-authored-shapes` (advisory). This
document is how you discharge that debt for one slice.

> **Equivalence before deletion.** A shape is deleted **only** after its check is
> provably reproduced by the projected union. Migrate → prove parity → *then* retire the
> hand-authored file. Never delete a shape whose constraint is not yet projected — that
> drops live enforcement (it is exactly what broke 324 conformance tests in the premature
> big-bang attempt).

## Decide the fragment

A hand-authored shape is one of two fragments:

| Fragment | Hand-authored form | Migrate to | Projects into |
|---|---|---|---|
| **Declarative** | `sh:property` with `sh:minCount`/`sh:maxCount`/`sh:class`/`sh:datatype`/`sh:nodeKind`/`sh:in` | an OWL/RDFS axiom in `module.ttl` + a closure opt-in | `generated/shapes/validation-shapes.ttl` |
| **Procedural** | `sh:sparql [ a sh:SPARQLConstraint ; sh:select … ]` | a `logic:Constraint` in `module.ttl` | `generated/shapes/procedural-constraints.ttl` |

## Declarative migration

Author the backing axiom in the **owning slice's `module.ttl`**; `derive_validation_shapes`
(`crates/logic-compile/src/frontend.rs`) reproduces it into `validation-shapes.ttl`.

- Range / class obligation → `rdfs:range` / `owl:someValuesFrom` (+ `ClosedWorldClosure`
  opt-in where a `sh:class` range shape must derive; `OpenWorldClosure` to opt a term out).
- Datatype / nodekind / value-set → `rdfs:range` / `owl:DatatypeProperty` / `owl:oneOf`.

**Reasoner-safety (mandatory).** Cardinality restrictions must stay inside the EL fragment
or `make reason-verify` **hard-fails**:

- NEVER `owl:cardinality` / `owl:minCardinality` / `owl:maxCardinality` (out of EL).
- Existence (`sh:minCount 1`) → `owl:someValuesFrom` (skolem-free, no `minCount` needed).
- Bounded count → `owl:maxQualifiedCardinality` + `owl:onClass`/`owl:onDataRange`
  (skolem-free); restore a lost `sh:minCount` with a sibling `owl:minQualifiedCardinality`.
- Avoid exact/single-node `owl:qualifiedCardinality` (skolem explosion).

## Procedural migration

Author a `logic:Constraint` whose `logic:integrity` is a realized `logic:Formula` tree
(`∀/∃/∧/∨/¬/→/↔`, `logic:relation` + `logic:argument`). The projector
(`crates/logic-compile/src/projections/shapes.rs`) lowers it to a
`sh:SPARQLConstraint` NodeShape named `{Name}ProceduralConstraintShape`, minted in the
constraint's own namespace, carrying `logic:formalizes`. Worked examples live in
[`slices/core/ai/module.ttl`](../slices/core/ai/module.ttl).

Skeleton (a guarded existence — "an LLM-extracted claim must carry a grounding span"):

```turtle
gmeow:ClaimNeedsEvidenceConstraint
    a logic:Constraint ;
    logic:severity "Warning" ;                       # omit ⇒ Violation
    logic:formalizes gmeow:ClaimNeedsEvidenceShape ; # provenance; may name the retired shape
    logic:message "…advisory, droppable message…" ;
    logic:integrity logic:cneForall .
logic:cneForall a logic:Formula ;
    logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable "this" ] ;
    logic:forall logic:cneImpl .
logic:cneImpl a logic:Formula ; logic:antecedent logic:cneGuard ; logic:consequent logic:cneExists .
logic:cneGuard a logic:Formula ; logic:and logic:cneType , logic:cneMethod .
logic:cneType a logic:Formula ; logic:relation <…#type> ;
    logic:argument [ logic:termIndex 0 ; logic:termVariable "this" ] , [ logic:termIndex 1 ; logic:termIri gmeow:StandpointClaim ] .
logic:cneMethod a logic:Formula ; logic:relation gmeow:observationMethod ;
    logic:argument [ logic:termIndex 0 ; logic:termVariable "this" ] , [ logic:termIndex 1 ; logic:termIri gmeow:methodLlmExtraction ] .
logic:cneExists a logic:Formula ;
    logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable "span" ] ;
    logic:exists logic:cneBody .
logic:cneBody a logic:Formula ; logic:relation gmeow:groundedIn ;
    logic:argument [ logic:termIndex 0 ; logic:termVariable "this" ] , [ logic:termIndex 1 ; logic:termVariable "span" ] .
```

Idiom cheatsheet:

- **Target** is *derived from the guard*: a guard atom with `this` as subject →
  `sh:targetSubjectsOf` / `sh:targetClass`; with `this` as object (e.g.
  `groundedIn(?claim, this)`) → `sh:targetObjectsOf`. No `sh:target` is hand-written.
- **Class guard** → `logic:relation <…22-rdf-syntax-ns#type>` with `logic:termIri <Class>`.
- **Existence obligation** → nested `logic:exists` in the consequent (lowers to
  `FILTER NOT EXISTS`); disjunctive obligations lower to a `UNION` of `NOT EXISTS`.
- **Comparison** → `logic:termGreater` / `logic:termLessEqual` (numeric/temporal).
- **Value set** → `logic:termIn`; **nodekind** → `logic:termIsIri`.
- A constraint that exceeds the range-restricted guarded fragment (arity-1 atoms, full
  first-order nesting) is **carried as flagged unsupported residue** in the loss ledger —
  never silently dropped — and the hand-authored shape must stay until the check is
  relocated (a STOP-and-ask, not a self-granted exception).

## Prove parity, then retire

1. `make regenerate` **twice** (SHACL/cardinality edits leave
   `gmeow.{openapi,schema}.json` stale after one pass), then `make check-generated`.
2. Confirm the slice's `tests/example-conformance.ttl` + `tests/counter-examples/*.ttl`
   still pass/fail identically against the projected union (each migrated constraint needs
   a ≥1-pass / ≥1-fail witness pair).
3. Only then delete the migrated shape (or its block); the shape-purity intent is that the
   authored tree holds `logic:` only.
4. Re-run `make check` — the `slice-quality.projection.hand-authored-shapes` advisory for
   the slice clears once its `shapes.ttl` is gone.
