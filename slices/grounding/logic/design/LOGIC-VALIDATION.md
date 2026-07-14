<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — Validation as a Projection: the SHACL / ShEx shape surface

> The **shape-surface** chapter of the GMEOW Logic design set: how closed-world
> data-shape validation — the constraints others hand-author *in* SHACL or ShEx — is
> authored once as canonical `logic:` validation shapes and **projected** to a SHACL
> Core surface and a ShEx surface, never authored on the surface directly. It is the
> constraint peer of the derivation projection in
> [`LOGIC-SHACL-AF.md`](LOGIC-SHACL-AF.md): that document projects the productive subset
> (`derivation rule` → `sh:SPARQLRule`, *these derive*); this document projects the
> integrity subset (`validation shape` → `sh:NodeShape` / ShEx shape, *these validate*).
> Both obey the doctrine stated once in [`LOGIC.md`](LOGIC.md) and generalized in
> [`LOGIC-META-SEMANTICS.md`](LOGIC-META-SEMANTICS.md): every prior formalism — OWL,
> Datalog, N3, SPARQL, SHACL, **and ShEx** — is a generated lossy projection of the
> canon, so power is added to the canon and emitted to the surface.
>
> **Reading this document.** The declarative present tense is normative: "X is" means a
> conforming realization implements X, established by the loss ledger and the
> conformance corpus ([`LOGIC-CONFORMANCE.md`](LOGIC-CONFORMANCE.md)). It is not a claim
> that any implementation realizes more than the corpus demonstrates.

## The proposal this inverts

The default practice — and the practice a naive reading of the RDF stack invites — is
to **hand-author shapes on a validation surface**: write `sh:NodeShape` / `sh:PropertyShape`
in a `shapes.ttl`, or write ShEx shape expressions, and treat that surface file as the
place where the data contract lives. That is what most SHACL/ShEx tooling assumes, and
it is what a portion of GMEOW's own tree still does (the slice-resident `shapes.ttl`
files and the vocabulary-derived `gmeow.shex` export).

GMEOW's position is the exact inverse, and — as with the computation surface — it is not
a matter of taste. It follows from Constitution **Principle 17** (the logic is canonical;
OWL/Datalog/SHACL/SPARQL are projections of it), **Principle 4** (one canonical source;
everything else a generated lossy projection), and **Principle 12** (the expressivity
ceiling is set by the canon, not by a downstream surface). Hand-authoring shapes would:

- make a **validation surface** the home of the data contract, so the contract is capped
  at that surface's expressivity *at authoring time* — a shape that is really a full-FOL
  integrity condition can never be stated, and there is no honest account of what the
  surface could not say;
- create a **second source of truth** — a constraint no reasoner backs, that cannot be
  reasoned over (shape subsumption, redundancy, and contradiction between two shapes stay
  invisible), and that no loss ledger governs;
- leave two surfaces (SHACL and ShEx) free to **drift** from each other, because each is
  authored apart rather than lowered from one object — the same decoupled-asymmetry defect
  the correspondence calculus ([`LOGIC-CORRESPONDENCE.md`](LOGIC-CORRESPONDENCE.md)) removes
  from alignment.

So GMEOW does not author shapes on the SHACL or ShEx surface. It authors data-shape
conditions in `logic:` — where they are typed, reasoned over, standpoint-indexable, and
governed by the loss ledger — and **projects** a SHACL Core surface and a ShEx surface for
any consumer that wants the contract expressed in a standard shape language. Each
projection is generated, drift-gated, and carries a preservation judgment at the boundary.

## The canonical object: the `validation shape` node kind

[`LOGIC-IR.md`](LOGIC-IR.md) enumerates **`validation shape`** as a first-class IR node
kind — "a closed-world data-shape condition (the SHACL-shaped subset)" — held distinct
from the general **`constraint`** kind ("an integrity condition whose violation is a
finding, not a derivation") and from the **`derivation rule`** kind (the productive subset
the SHACL-AF surface projects). Keeping the three apart is exactly what stops a data-shape
check from being mistaken for a derivation, and a shape-expressible check from being
confused with the full-FOL integrity condition it may only *approximate*.

- A `logic:` **validation shape** targets a class (or a value-keyed selection) and states,
  per constrained path, the closed-world cardinality, kind, datatype, value-set, pattern,
  language, and node-shape conditions its focus nodes must satisfy — the closed-world
  reading that [`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md) declares co-resident with
  open-world classification. Its violation is a **finding**, not a derived triple.
- A general **`constraint`** node that exceeds the closed-world shape subset — a full-FOL
  integrity condition, a standpoint- or world-indexed constraint, a paraconsistent
  contradiction witness — is **not** a validation shape. Only its shape-expressible
  under-approximation lowers to a shape; the residue is carried in the canon and flagged
  (below). This is the exact analogue of the SHACL-AF rule surface, where a full-FOL rule
  body is carried and only its stratified-Horn fragment projects.

The frontend stays ergonomic and slice-local — an author does not hand-write the canonical
form. The reasoned, law-bearing validation shape is what the *compiler produces* from that
frontend, the same language/IR separation the correspondence calculus uses
([`LOGIC-CORRESPONDENCE.md`](LOGIC-CORRESPONDENCE.md) §"Frontend syntax versus canonical
form"). What this buys, that a hand-authored shape cannot: the shape is **reasoned over**
(subsumption, redundancy, and contradiction between shapes become inferences, not review);
it is **content-addressed** (so it dedups and rides compactly in `gmeow.gts`); it is
**standpoint-indexable** (a shape may hold under one standpoint and not another, rather than
silently universal); and it is **governed by the loss ledger** (so every shape surface
carries an honest preservation judgment and the purity gate can red the build).

## The two shape projections

A `logic:` validation shape projects to two target dialects. There is **one** shape
lowering, reused, so the two surfaces cannot drift from each other — exactly as OWL-DL and
OWL-EL cannot drift from one `LogicProgram`, and as SSSOM and EDOAL cannot drift from one
correspondence leg.

### SHACL Core

The validation shape's target becomes the shape's `sh:targetClass` (or an
`sh:SPARQLTarget` when the trigger is value-keyed); each constrained path becomes an
`sh:PropertyShape` carrying the cardinality / `sh:class` / `sh:datatype` / `sh:nodeKind` /
`sh:in` / `sh:pattern` / `sh:minLength` / `sh:languageIn` / logical-combinator conditions
the shape declares. GMEOW's RDF-1.2 statement-layer extension (`sh:reifierShape` /
`sh:reificationRequired`, already implemented in the native engine) is one more component
the shape node can carry — so the projected surface validates reified statements a
standard-SHACL surface cannot even name.

### ShEx

The **same** validation shape projects to a ShEx shape expression: the target becomes the
shape's node selector, each path becomes a triple constraint with its cardinality and value
set (`NodeKind`, datatype, value-set, and XSD facets). ShEx is a **strictly narrower**
surface than SHACL Core — it has no SPARQL-based constraint form, no cross-node comparison,
and a thinner facet vocabulary — so its preservation claim is weaker and its residue set
larger, and both are declared, never inferred by the reader. The existing `gmeow.shex`
export, today derived structurally from the vocabulary fold, is subsumed by this projection:
its shapes become lowerings of the authored validation shapes, not an independent
structural downcast of property domains and ranges.

## Where the loss is

Each shape surface is a **validation dialect of a fixed fragment**, so each projection
declares its preservation in the loss ledger
([`LOGIC-CONFORMANCE.md`](LOGIC-CONFORMANCE.md), `target_meta`), never silently — reusing
the preservation-polarity vocabulary verbatim rather than inventing a parallel loss system:

- Within the closed-world shape fragment each surface can express, the projection is
  faithful for the validation query class: the surface flags exactly the focus nodes the
  canonical shape flags. The applicable polarity is `logic:ValidationOnly` for that class
  (a shape surface validates; it does not entail), sharpening to
  `logic:SoundUnderApproximation` for any conjunct the surface can only partially enforce.
- **Full-FOL integrity conditions** (genuine quantifier/connective trees), **standpoint- /
  world- / time-indexed** constraints, **paraconsistent** contradiction witnesses, and any
  cross-node condition **ShEx** in particular cannot state, have no faithful shape form.
  They are **carried and flagged** in the canonical layer (lossless under the canonical
  RDF-1.2 projection, reachable for checking through the relational-core lowering of
  [`LOGIC-RUNTIME.md`](LOGIC-RUNTIME.md)), and recorded as ledgered drops on the affected
  target — never dropped in silence.
- The two targets carry **different** residue sets against the same canonical shape: a
  condition that survives to SHACL Core may be a `logic:unsupported` drop for ShEx. The
  ledger records the polarities and the unsupported-construct set per target, so "this
  contract holds in SHACL but degrades in ShEx" is a machine-readable fact, not a footnote.

The asymmetry is stated, not hidden: the canon authors the shape; both shape surfaces are
**emit-only** — there is no parse-back from `sh:NodeShape` or a ShEx expression into a
`logic:` validation shape, because the canon, not the surface, is the authoring ground
(Principle 4). A reader must not infer a round-trip the doctrine deliberately does not
provide.

## Deriving the fragment from slice axioms

The validation shapes are not hand-authored: they are **derived** from the constraint-bearing
axioms slices already carry — `rdfs:domain`/`rdfs:range`, cardinality, `owl:Functional`/
`InverseFunctionalProperty`, `owl:someValuesFrom`/`allValuesFrom`/`hasValue`, qualified
cardinality, `owl:disjointWith`/`oneOf`/`AllDisjointClasses`, and the value-set and facet
restrictions. Two validates-but-does-not-entail conditions have NO slice-authorable OWL
antecedent, because their OWL renderings are outside the native reasoner's decidable
fragment, and are authored as constraint-sugar records the derive lowers onto the target
class shape instead: a literal-pinned **forbidden value** ("class C must never carry
P = v" — the value-complement pattern) is a `logic:ForbiddenPatternConstraint` with a
literal `logic:forbiddenValue`, lowered to `sh:not [ sh:hasValue v ]` (the IRI-pinned form
is the class-negation idiom, carried node-level as `sh:not [ sh:class … ]` by the
disjointness family, and keeps its procedural projection only); and a bounded **numeric
range** (a faceted-datatype filler is undecidable natively the moment a literal is asserted
on the constrained path) is a `logic:ValueRangeConstraint` with inclusive literal bounds,
lowered to `sh:minInclusive`/`sh:maxInclusive`. There is no parallel hand-authored shape
vocabulary; the only other authored signal is the closure/reading annotation described
below.

**Dataset-derive, not typed-IR lift.** The derivation reads the **merged authored dataset**
directly (the root ontology plus every slice module, closed under the same fold the pipeline
builds) and lowers each constraint axiom to a `ValidationShapeIr`. The alternative — lifting the
already-typed logic IR — was rejected: the restriction and cardinality axioms live in the
authored OWL/RDFS surface, not in a typed logic node, so a dataset-derive reads them where they
actually are and stays a thin, single-pass projection of the authored ground rather than a
second typed layer that could itself drift from the slices. This keeps the derivation consistent
with the rest of the pipeline (every generated artifact is a projection of the same fold) and
means an author adds a constraint by writing ordinary slice OWL, never a shape.

**Derive-all by default, with a closure opt-out.** Every eligible axiom derives a shape by
default (MAXIMAL UTILITY). A property or class may decline the closed-world reading with a single
authored signal — a `logic:ClosureEntry` binding its `logic:closureKey` to
`logic:closureValue logic:OpenWorldClosure` — reusing the existing closure-map vocabulary
verbatim (no new shape DSL). An absent annotation means "derive".

**The domain/range exception: open-world by default.** `rdfs:domain` and `rdfs:range` are the
one construct that inverts the default. They are open-world **inference** axioms: they *entail*
the subject's / object's type, they do not *require* that type to be asserted. Read closed-world
as an `sh:targetSubjectsOf`/`sh:targetObjectsOf` + `sh:class` obligation, they over-claim on any
graph that legitimately relies on the entailment rather than restating the type standalone — so a
naive derive-all makes the shape surface reject the ontology's own illustrative instance data. A
domain/range validation shape is therefore derived **only** when the property is explicitly opted
**in** with a `logic:closureValue logic:ClosedWorldClosure` closure entry — the exact inverse
polarity of the opt-out, using the same closure vocabulary. Genuinely closed-world constraints
(cardinality, value restrictions, functional / inverse-functional, disjointness) are real
obligations, not inference axioms, and stay derive-all: a node carrying two disjoint types or a
second value on a functional property is a genuine violation, not an un-asserted entailment.

**Required-path projection without OWL existentials.** An `owl:someValuesFrom` restriction
`K ⊑ ∃P.C` remains an open-world existential in the logical core. Its validation projection is
therefore only the conservative value-typing under-approximation: a bare `sh:class` obligation,
vacuously satisfied when the path is absent. It never projects `sh:minCount` by itself.

A genuinely closed-world required path is authored as an `owl:allValuesFrom` value constraint plus
an explicit `logic:ClosureEntry` carrying `logic:onClass K`, `logic:closureKey P`, and
`logic:closureValue logic:ClosedWorldClosure`. That class/property pair is the canonical authority
for adding `sh:minCount 1`. Keeping requiredness out of `owl:someValuesFrom` prevents the native
reasoner from minting existential witnesses into the shipped closure while preserving the exact
closed-world validation obligation. A closure entry without `logic:onClass` may still opt a
property's `rdfs:domain`/`rdfs:range` projection in globally, but it cannot make a path required for
every class that uses the property.

**The statement-layer reifier obligation (`sh:reifierShape` / `sh:reificationRequired`).** One
closed-world condition has **no ordinary-OWL antecedent**: "every `K`→`P`→value assertion must be
reified, and the reifier of that statement must conform to shape `C`." OWL cannot say it, and
minting a hand-authored shape predicate for it would reintroduce the parallel shape vocabulary this
layer forbids. It is therefore derived from the **classic RDF reification form the frontend already
reads** — no new authoring term: a slice authors a reifier `?r` (`a rdf:Statement`) typed with a
GMEOW class `C` that reifies the schema-level triple via `rdf:subject K` / `rdf:predicate P` /
`rdf:object O`. The classic form is used rather than the native RDF-1.2 `rdf:reifies <<( K P O )>>`
term on purpose: a quoted-triple object cannot ride the base-quad fold to the `gmeow.gts` terminal
(the statement layer travels the reifier/annotation tables), so the schema-level obligation is
authored as plain base triples. The derivation lowers this to a property shape on path `P` (targeting
`K`) carrying `sh:reifierShape {C}-shape` + `sh:reificationRequired true`. Two rules make it
well-formed: (1) `K`, `P`, and `C` must all be GMEOW-owned (the same dogfooding guard every family
applies); and (2) `C` must be a **constrained** GMEOW class so FAMILY 1 independently derives the
`{C}-shape` node the `sh:reifierShape` reference resolves to — an untyped or unconstrained reifier
would dangle the reference, so it is not an obligation. Unlike the other families the reifier
component is a **property-shape** condition (it is
keyed to the path's statement): the native SHACL 1.2 engine reads `sh:reifierShape` /
`sh:reificationRequired` only from a single forward-predicate property shape, so they emit inside the
`sh:property [ … ]` block, never on the node shape. ShEx Core has no statement layer, so the
condition is carried in the loss ledger as ShEx residue (the SHACL surface emits it faithfully).

## Verified by construction, and the placement rule

The emitted shape surfaces are generated artifacts, regenerated by the single-pass pipeline
and proven drift-free by `check-generated` (Principle 7): the committed bytes equal the
bytes the projector produces, or the build is red. They ride the in-memory carrier into
`gmeow.gts` through the typed shape lookaside (the `Shacl` / `Shex` sidecar kinds the RDF
core already distinguishes), so a repo-free consumer reads the validation surface without
re-running the compiler (maximal information flow).

The placement rule is the **Hybrid convention** the foundation and the computation surface
already use, now extended from derivation to constraint: a validation-shape construct
(`sh:NodeShape` / `sh:PropertyShape`, or a ShEx shape expression) that appears anywhere in
the authored sources (`slices/`, `shapes/`, `dsl/`) rather than under the generated tree is
either a hand-authored second source of truth — which is **forbidden** — or it carries a
`logic:formalizes` back-reference naming the `logic:` validation shape it is the projection
of. The static seal that enforces this is the **shape half** of the projection-purity gate
in [`governance/constitution.ttl`](../../../../governance/constitution.ttl) (`meta:gate-projection-shape-purity`,
`check_projection_shape_purity`): the computation gate seals `sh:rule` / `sh:SPARQLRule`; the
shape gate seals `sh:NodeShape` / `sh:PropertyShape` and the ShEx surface, closing the exemption
that currently lets a hand-authored constraint shape pass.

The shape gate is realized **incrementally**, exactly as the frame/result shape stages that
already emit SHACL under the generated tree are "the projector's first realized fragment." The
first realized fragment of the *constraint* half is the **`sh:sparql` procedural-constraint
signature** of the open-world FOL axioms (irreflexivity, acyclicity, relatum-distinctness): each
is authored once in `logic:` (a `logic:PropertyCharacteristicAssertion` /
`logic:RelatumDistinctnessAssertion` / named `owl:AllDisjointClasses`), reasoned over by the
native coherence gate, and projected to `sh:SPARQLConstraint` node shapes in
`generated/shapes/constraint-shapes.ttl`; the seal red-fails on an authored `sh:select`
re-encoding one of those axiom signatures without a `logic:formalizes` back-reference. The
remaining fragment — the declarative `sh:PropertyShape` closed-world checks (cardinality,
datatype, node-kind, value-set) and the ShEx surface — migrates next, under
equivalence-before-deletion ([`LOGIC-CONFORMANCE.md`](LOGIC-CONFORMANCE.md)): the current
committed shapes are the golden oracle the projector must reproduce before the hand-authored
versions are deleted. (SPARQL/cross-node constraints have no ShEx form — ShEx has no
SPARQL-based constraint — so the FOL-axiom fragment is SHACL-only, recorded as a `logic:unsupported`
ShEx drop in the loss ledger.)

## A named consumer (P15)

A projection earns its existence by a consumer (Principle 15). The **SHACL Core** shape
surface serves a consumer running a SHACL validator — a data-ingestion or conformance
toolchain that wants GMEOW's data contracts as standard shapes it can enforce in place. The
**ShEx** surface serves the ShEx-validator ecosystem (notably the Wikidata / EntitySchema
lineage), which will not adopt SHACL and wants the same contracts as shape expressions. For
each consumer the surface is the point of contact; for GMEOW both remain projections, peer
to the OWL/Datalog/gUFO and SHACL-AF projections, never a second source of truth.

## Relationship to the correspondence calculus

The shape surface and the correspondence calculus meet at the **validation law**. A
`logic:Correspondence` that claims to project GMEOW into an external target
([`LOGIC-CORRESPONDENCE.md`](LOGIC-CORRESPONDENCE.md), the OpenEHR "replacement" laws)
asserts that the down-projected view *validates standalone* against the target's model —
`π_target(d(g)) ⊨ shapes`. That validation is discharged by a validation shape projected to
the target's own shape surface. So the shape projection is not only a standalone consumer
surface; it is the executable form of a correspondence's validation law — one more place the
loss ledger, not a slogan, decides whether "GMEOW subsumes V" holds.

## Where this sits

| Concern | Document |
|---|---|
| The projection doctrine these surfaces obey | [`LOGIC.md`](LOGIC.md), [`LOGIC-FOUNDATION.md`](LOGIC-FOUNDATION.md), [`LOGIC-META-SEMANTICS.md`](LOGIC-META-SEMANTICS.md) |
| The typed IR and the `validation shape` / `constraint` node kinds this projects | [`LOGIC-IR.md`](LOGIC-IR.md) |
| The closed-world / open-world co-residence the shape reading assumes | [`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md) |
| The derivation peer of this constraint surface (the *other* half of SHACL) | [`LOGIC-SHACL-AF.md`](LOGIC-SHACL-AF.md) |
| The loss ledger, preservation kinds, and equivalence-before-deletion migration | [`LOGIC-CONFORMANCE.md`](LOGIC-CONFORMANCE.md) |
| The relational-core lowering that checks constraints beyond the shape fragment | [`LOGIC-RUNTIME.md`](LOGIC-RUNTIME.md) |
| The correspondence whose validation law this surface discharges | [`LOGIC-CORRESPONDENCE.md`](LOGIC-CORRESPONDENCE.md) |

The vocabulary the shapes are authored in is in [`../module.ttl`](../module.ttl); the
generated SHACL and ShEx surfaces are projections of those validation shapes, written by the
pipeline and proven drift-free by the generated-artifact gate.
