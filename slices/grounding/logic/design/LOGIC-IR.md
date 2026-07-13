<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — The Typed Intermediate Representation

> The compiler's intermediate representation. The IR is
> the single typed structure every `logic:` source compiles into and every projection compiles
> out of. Member of the GMEOW Logic design set ([`LOGIC.md`](LOGIC.md)); the surface profiles that
> select how the IR is evaluated are defined in [`LOGIC-CONTRACT.md`](LOGIC-CONTRACT.md), and the
> model-theoretic meaning of its constructs in [`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md).

## What the IR is

The IR is **a typed unified logic IR with a full-FOL formula core**, not a Horn or Datalog
fragment with extensions. The IR is unified because it holds more than object-level formulas:
transactions, action schemas, validation shapes, and meta-formulas all live in the same typed
structure. A `logic:` program is parsed once into this typed IR; every output — OWL, Datalog, N3,
the Common Logic dialects, the canonical RDF 1.2 serialization — is a projection *of* the IR; and
every external dialect ingested is parsed *into* the same IR. There is exactly one IR; the surface
a request targets is a facet of the reasoning contract, not a different internal form.

**Predicates and types are reified as ordinary objects** (a HiLog-style reflection). When the
foundation needs to quantify over a predicate or a type, it quantifies over the *object* that
reifies that predicate or type, not over a genuine predicate variable. The object level therefore
stays first-order — quantifiers range over individuals in the domain of discourse, some of which
happen to be reified predicates and types — while still expressing what reads as quantification
over predicates and types. This is a deliberate design choice: a first-order object level with
reflected types, rather than admitting genuine predicate variables (which would push the IR beyond
first-order). This is why the formula core is full-FOL: the core quantifiers are first-order, and
higher-order-looking statements are recovered through reification rather than by extending the
logic.

"Datalog plus negation-as-failure" is one evaluable subset of the IR, reached by lowering, not the
ceiling of what the IR can hold.

### The realized formula surface

The full-FOL core is a reified `logic:Formula` tree. A formula node is one of:

- a **quantifier** — `logic:forall` / `logic:exists` over a body formula, with its bound variables
  carried by ordered `logic:quantifiedVariable` term-carriers (multi-variable block order is
  significant);
- a **connective** — `logic:and` / `logic:or` (variadic, commutative), `logic:not` (strong negation,
  kept distinct from negation-as-failure), the ordered `logic:antecedent` / `logic:consequent` pair of
  a material implication, or the commutative `logic:iff`;
- an **atomic predication** — `logic:relation` (a reified relation `logic:Type`, the HiLog reflection —
  no predicate-variable term) over ordered `logic:argument` term-carriers.

A **sequence marker** (`logic:SequenceMarker`, carried by `logic:termSequenceMarker`) is a variadic
argument that binds a *sequence* of terms, generalizing the fixed arity-three atom to predications of
any arity. Each term-carrier fixes its position with `logic:termIndex` and holds exactly one of
`logic:termIri` / `logic:termVariable` / `logic:termLiteral` (+ `logic:termLiteralDatatype`) /
`logic:termSequenceMarker`, so the variadic, order-significant lists round-trip independent of RDF
statement order.

Horn+NAF derivation rules remain a recognized **sub-fragment** carried by `logic:Rule`: a program's
trivially-Horn facts and rules stay in the rule/axiom collections, and only what genuinely exceeds
that fragment is carried as a `logic:Formula`. A trivially-Horn binary predication is therefore never
admitted as a top-level formula, so a single fact never receives two distinct canonical identities.

## Node kinds

The IR is a typed sum, not an untyped triple bag. Every node declares its kind, and the kind
governs what may be done with it:

- **object-level formula** — an ordinary first-order formula over the domain of discourse;
- **meta-level formula** — a formula *about* formulas (a statement that quotes or ranges over
  other propositions, distinct from the propositions it mentions);
- **constraint** — an integrity condition whose violation is a finding, not a derivation;
- **derivation rule** — a head entailed from a body, the productive subset;
- **query** — a goal to be resolved, with its answer shape;
- **transaction program** — a state-changing composite over the path semantics of
  [`LOGIC-TRANSACTION.md`](LOGIC-TRANSACTION.md);
- **action schema** — a named precondition/effect/invariant template a transaction program may
  invoke;
- **validation shape** — a closed-world data-shape condition (the SHACL-shaped subset), the
  single kind the SHACL Core and ShEx shape surfaces are generated lowerings of, distinct from
  the general `constraint` kind whose full-FOL residue only these surfaces approximate (fully
  specified in [`LOGIC-VALIDATION.md`](LOGIC-VALIDATION.md));
- **correspondence** — a law-bearing, possibly-lossy, possibly-bidirectional alignment between a
  source pattern and a target pattern (the **ninth** kind), wrapping a `logic:Lens` (its executable
  `get`/`put` core) and carrying its morphism class on the seven-rung ordered law-spine, its claimed
  laws with discharge verdict (reusing the foundation's `logic:DischargeVerdict` /
  `logic:DischargeCondition` vocabulary), the separated quantitative axes, FOL/SOL caveats, and
  standpoint indexing. Its `get`/`put` legs are transaction programs; its caveat/relation envelope is
  meta-level and stays meta. It is the single kind cross-ontology alignment compiles into, and from
  which SSSOM/EDOAL/FnO/SPARQL/up-lift are generated lowerings. Fully specified in
  [`LOGIC-CORRESPONDENCE.md`](LOGIC-CORRESPONDENCE.md).

Keeping these kinds distinct is what prevents a constraint from being mistaken for a rule, a query
operator from being read as a program operator, or a meta-level quotation from collapsing into the
object-level assertion it quotes.

## What the IR makes explicit

A typed unified logic IR must pin down the decisions that informal rule languages leave implicit.
Each is a declared property of the IR, never an unstated convention:

- **Equality and congruence** — whether equality is asserted, derived, or absent, and the
  congruence it licenses.
- **No unique-name assumption by default** — distinctness is asserted, not presumed from distinct
  names; a unique-name policy is an opt-in (the `Equality` facet of the contract).
- **Datatype semantics** — the value spaces and comparisons for typed literals.
- **Existential witnesses and Skolem terms** — how existentials are witnessed, with Skolem identity
  scoped so a witness is the same across re-evaluations *of the same existential in the same
  setting*. A witness's identity is determined by (at least) the source formula it witnesses, the
  binding of that formula's free variables, the context or world in which it is introduced, and the
  governing reasoning contract. Scoping identity this way is what keeps witnesses for distinct
  formulas, distinct bindings, or different worlds from collapsing into one term by accident.
- **Variable hygiene and alpha-equivalence** — bound-variable renaming is meaning-preserving; the
  canonical form is alpha-normalized so equal-up-to-renaming formulas share one identity.
- **Domain-closure assumptions** — whether the domain is closed (only named individuals exist) is
  declared, never assumed.
- **Explicit versus default negation** — strong negation and negation-as-failure are different
  nodes, never conflated (and selected per contract).
- **Formula-versus-program operator typing** — connectives over formulas and combinators over
  transaction programs are typed apart; serial composition is not conjunction.
- **Quantification over reified predicates and types** — where the foundation's instantiation
  reasoning requires ranging over a predicate or a type, it ranges over the object that reifies it
  (HiLog-style), keeping the object level first-order; the reification layer is tracked so reflected
  chains stay coherent.
- **Stratification of any truth or `holds` predicate** — a predicate that reflects truth of other
  statements is stratified so the IR cannot encode a self-referential paradox by accident.
- **Module (theory) membership, orthogonal to world/standpoint** — which named theory a statement
  belongs to is an explicit contextual-scope dimension (`logic:Module`, carried per statement by
  `logic:inModule`, composed by `logic:imports`), distinct from the epistemic world/standpoint
  dimension. A module says *where* a sentence lives; a world/standpoint says *under whose
  perspective* it holds. Conflating the two is a category error, so they are separate axes of the
  same multi-dimensional context facet (`logic:ModuleContextAxis` alongside the world, standpoint,
  time, and path axes), never one slot. This is the construct the ingested and emitted Common Logic
  dialects (CLIF, CGIF, XCL) map their `(module …)` / `(cl-imports …)` forms onto; because it rides
  the ordinary reifier-scope carrier, it round-trips through the canonical RDF 1.2 serialization
  (and thus every faithful dialect) at `exact` preservation.

## Lowering and the preservation judgment

A projection lowers the IR toward a target whose expressivity is usually narrower. Lowering is
never assumed faithful. **Every lowering returns a preservation judgment** describing exactly what
the target preserves:

- **exact** — the target answers the same questions as the canonical form for the declared query
  class;
- **sound but incomplete (under-approximation)** — everything the target entails is canonically
  valid; it may miss answers;
- **complete but possibly unsound (over-approximation)** — the target does not miss answers; it may
  add some;
- **validation-only** — the target detects some invalidity but is not an entailment relation;
- **unsupported** — the construct cannot be expressed in the target at all.

A formula that lowers to `unsupported` is **carried and flagged, never dropped**, and every result
downstream of a lowering discloses which formulas the target did not evaluate (see
[`LOGIC-SEMANTICS.md` § The reasoning result](LOGIC-SEMANTICS.md)). The aggregate of these
judgments is the loss ledger that accompanies the generated artifacts (see
[`LOGIC-CONFORMANCE.md`](LOGIC-CONFORMANCE.md)).

The soundness of the full-FOL core — that a decided verdict is the *right* verdict, not merely a
self-consistent one — is established independently of any self-authored golden by the external
FOL soundness oracle: TPTP problems parsed into this IR, negation-reduced, decided over the
EL/DL-expressible fragment, and compared against their community-decided SZS ground truth, with the
first-order-beyond-DL remainder disclosed as capability-gap ledger rows (see
[`LOGIC-CONFORMANCE.md` § the external FOL soundness oracle](LOGIC-CONFORMANCE.md)).

### Class coverings and partitions

A **class covering** — "every `Whole` is one of `S₁ … Sₙ`" — is not a new node kind or a bespoke
axiom vocabulary. It is an ordinary object-level `logic:Formula`: the disjunction
`∀x. Whole(x) → (S₁(x) ∨ … ∨ Sₙ(x))`. Because a disjunction genuinely exceeds the Horn+NAF fragment
it is carried as a `logic:Formula` (never promoted from, nor duplicated as, a binary axiom), so a
covering has exactly one canonical identity. Disjointness among the members stays the separate,
trivially-binary `owl:disjointWith` axiom, kept out of the formula.

Lowering follows the preservation judgments above:

- **canonical RDF 1.2** carries the covering formula *exact*;
- **OWL 2 DL** recognizes the covering shape and lowers it faithfully — `owl:disjointUnionOf` when
  every member pair is asserted disjoint (a partition), otherwise `rdfs:subClassOf` an `owl:unionOf`
  class (an exhaustive cover that leaves a deliberate overlap intact). The union list and union
  class are minted, content-derived IRIs (never blank nodes), so the serialization is byte-stable
  across regeneration;
- **OWL 2 EL, the gUFO bridge, Datalog, and N3** cannot express a disjunction, so the covering is
  carried-and-flagged as `unsupported` residue (tagged `Disjunctive`), never silently dropped.

A covering states only exhaustiveness; it does not re-encode any membership discipline the
foundation already enforces (for example the sort partition's mutual exclusivity, which the
OntoUML stereotype-cardinality discipline owns).

## IR commitments — legalization, load-bearing annotations, the relational core

Three commitments shape the IR so that lowering and execution have a sound target. They are cheap to
honour from the start and expensive to retrofit; the correspondence calculus
([`LOGIC-CORRESPONDENCE.md`](LOGIC-CORRESPONDENCE.md)) and the native engine
([`LOGIC-RUNTIME.md`](LOGIC-RUNTIME.md)) both depend on them. The architecture is MLIR's — dialects,
per-node verifiers, progressive lowering; **not** LLVM IR's substrate (an imperative SSA IR cannot
represent open-world entailment, paraconsistency, or modal scope). Patterns and tooling cross over;
the substrate does not.

- **Lowering is legalization (`logic:ConversionTarget`).** A lowering to a target is a legalization
  against a declared legal IR or dialect — statically, or *dynamically legal* iff a construct falls
  in the target's certified fragment. A conversion target is distinct from `logic:ProjectionTarget`,
  the reasoning-contract facet that merely requests one or more answer renderings. **Partial
  conversion** leaves an illegal construct in place, flagged: this *is* the "unsupported carried and
  flagged, never dropped" rule above. Every lowering is therefore a total function into `⟨ legal
  output ⊕ flagged residue ⟩`, and the loss ledger is the residue set.
- **Every annotation is typed load-bearing or droppable (`logic:loadBearing`).** A display hint /
  `scopeNote` is **droppable** — correctness must never depend on it, and dropping it only pessimizes.
  An in-band complement or a quantitative axis is **load-bearing** — the inverse leg needs it for
  `put∘get = id`. A lowering may drop a droppable annotation silently but must either preserve a
  load-bearing one or record its loss. Without this bit a section/retraction (perfect-subsumption)
  claim cannot be verified, which is why it is in the node type from the start.
- **The relational-core dialect (`logic:RelationalCore`).** A first-class Datalog±-with-stratified-
  negation sub-language is the lowering waist between the full-FOL IR and the physical execution engine.
  Every execution strategy, the incremental layer, and the semiring annotation target it. The other
  prerequisites already hold: the canonical IR is content-addressed (below) and the quantitative axes
  are semiring-annotatable first-class structure ([`LOGIC-CORRESPONDENCE.md`](LOGIC-CORRESPONDENCE.md)
  § axes). The realized evaluable path is `Formula → NNF → Skolemize → Horn-clause extraction →
  logic:RelationalCore → typed native chase`: the Horn-expressible fragment lowers exactly and runs
  alongside the program's own rules. Fixed-arity **n-ary predication** (`op(x,y,z)`, or a unary atom)
  is *also* evaluable: at the lowering boundary each fixed-arity atom is **reified** into a conjunction
  of ordinary binary atoms over a single content-addressed reifier node —
  `logic:instanceOf(R, Rel) ∧ logic:naryArg0(R, a₀) ∧ … ∧ logic:naryArgN(R, aₙ)` (the standard
  n-ary-relations encoding under the HiLog reflection). A body atom binds a fresh reifier variable; a
  head atom *derives* a new tuple whose reifier node the restricted chase mints by tuple identity
  (`mint_nary_reifier`, content-addressed on the relation + ordered arguments), so a fixed-arity n-ary
  program lowers `exact` — no longer residue. Only what genuinely exceeds the fragment (a disjunctive
  head, an existential needing a Skolem *function*, a **genuinely unbounded** sequence-marker atom, or
  an n-ary head argument the body does not bind — a non-range-restricted existential) is
  partial-converted — carried as flagged residue, never silently evaluated as one disjunct. The residue
  is disclosed by a **closed shape-tag set** (`logic:FormulaShape`: `Disjunctive`, `Nested`,
  `Quantified`, `StrongNegation`, `Variadic` — the last now denoting *only* an unbounded sequence
  marker, not a fixed-arity arity mismatch), so the loss ledger names *which* construct exceeded the
  fragment rather than emitting one opaque note, and the resulting answer carries a `sound-under`
  preservation polarity rather than a false `exact`.

**Validation, not trust.** Transforms are validated, not trusted: a round-trip/witness-preservation
check (the analogue of compiler `debugify`) and a refinement check against the declared preservation
polarity (the analogue of translation validation) are decidable graph-isomorphism checks over the
content-addressed canonical form, not semantic-equivalence search.

## Canonical identity

The IR has a canonical form — sorted, alpha-normalized, with content-addressed identity for
formulas, witnesses, and derivations. Two programs that normalize to the same typed structure under
alpha-renaming, RDF graph isomorphism, ordering normalization of commutative collections, and the
explicitly declared rewrite system share one canonical IR identity. This is **structural** identity,
not a claim to decide general semantic equivalence: two programs that happen to mean the same thing
but normalize to different structures have different canonical IRs, because deciding semantic
equivalence for the full IR is not reducible to content addressing. Round-tripping a program through
any faithful projection and back yields the same canonical IR.

For a formula, the canonical key is **alpha-normalized**: a single binding-environment walk renames
bound variables to de-Bruijn-style canonical tokens so alpha-equivalent formulas (`∀x.p(x)` and
`∀y.p(y)`) share a key, while **free** variables are preserved (they carry meaning). It is also
**order-normalized**: `logic:and` / `logic:or` are flattened and their operands sorted by
already-normalized child key, `logic:iff` is pair-sorted, and the ordered `logic:antecedent` /
`logic:consequent` of an implication keep their order. This alpha-normalized order is also the order
the Skolem-witness IRIs are minted in, so two equal-but-differently-constructed formulas produce
identical witnesses. This canonical identity is the anchor
the conformance contract checks against and the basis for the content-addressed provenance that
proofs and explanations cite.

## Constitutional alignment

One canonical form; every surface a generated projection of it, each carrying an honest
preservation judgment. The IR is where the doctrine "describe once, generate the rest" is enforced
for the reasoning layer, and where "never silently degrade" becomes a typed, machine-checked
property rather than a promise.
