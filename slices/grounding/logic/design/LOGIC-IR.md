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
- **Native term identity** — relation dictionaries and bound probes compare the full
  PurRDF `TermValue`, including datatype, language, direction, blank scope and quoted
  components. Persistent term, plan and witness keys use PurRDF's injective canonical
  encoding. Display text is an output projection and never a term identity key.
  Datatype value equality remains a separate logical operation; interning does not
  identify distinct lexical terms merely because their denotations agree.
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
  § axes). The evaluable path is `Formula → NNF → existential normalization → Horn dependency
  extraction → logic:RelationalCore → typed native chase`: the Horn fragment lowers exactly and runs
  alongside the program's own rules. Fixed-arity **n-ary predication** (`op(x,y,z)`, or a unary atom)
  is *also* evaluable: at the lowering boundary each fixed-arity atom is **reified** into a conjunction
  of ordinary binary atoms over a single content-addressed reifier node —
  `logic:instanceOf(R, Rel) ∧ logic:naryArg0(R, a₀) ∧ … ∧ logic:naryArgN(R, aₙ)` (the standard
  n-ary-relations encoding under the HiLog reflection). A body atom binds a fresh reifier variable; a
  head atom *derives* a new tuple whose reifier node the restricted chase mints by tuple identity
  (`mint_nary_reifier`, content-addressed on the relation + ordered arguments), so a fixed-arity n-ary
  program lowers `exact`. A positive existential head `B(x) → ∃z. H(x,z)` retains its shared
  witnesses across the whole head conjunction as one dependency. This applies to binary heads as
  well as reified n-ary heads; termination and completion require separate native admission evidence.
  Restricted-head satisfaction is an existential probe over the frozen native
  round: one complete extension is sufficient to block invention, while absence
  requires completing the entire probe. A traversal budget exhausted without a
  witness withholds the decision. Body joins and head probes stream one current
  path through indexed native cursors; they do not materialize tables of partial
  assignments. Bound native-term inequality guards prune the head path as soon as
  both operands are available. These guards retain the selected TGD semantics;
  they do not add a unique-name assumption to DL cardinality reasoning. Body
  provenance retains the actual source facts and spellings on the chosen path.
  Head rows enter the caller's deterministic candidate buffer directly, borrowing
  their firing's premises until publication. Standalone chase retains the first
  source for each fact; joint execution retains its total provenance ordering.
  Duplicate removal changes neither the selected raw-candidate ceiling nor the
  shared derivation budget. No second pending-row materialization is required.
  A finite corpus never certifies an unrestricted rewrite.
  Disjunctive or negative heads, further quantifier alternation, unsupported compound function terms,
  unbounded sequence markers, and universal head variables not bound by the body remain flagged
  residue. They are never silently evaluated as one disjunct or as existential witnesses. The residue
  is disclosed by a **closed shape-tag set** (`logic:FormulaShape`: `Disjunctive`, `Nested`,
  `Quantified`, `StrongNegation`, `Variadic` — the last now denoting *only* an unbounded sequence
  marker, not a fixed-arity arity mismatch), so the loss ledger names *which* construct exceeded the
  fragment rather than emitting one opaque note, and the resulting answer carries a `sound-under`
  preservation polarity rather than a false `exact`.

  Native key agreement is a witnessed conjunction: every key property must have at
  least one shared value for the two subjects in the same asserting world. It does
  not assume unique resource names; a key clash requires explicit distinctness.
  A list-carried key requires a complete, nonempty, unambiguous property list.
  A canonical `logic:KeyAssertion` has exactly one resource-valued `logic:keyClass`
  and a nonempty set of IRI-valued `logic:keyProperty` components. Adding a component
  strengthens the conjunction without making that record malformed, so every
  producer of `keyClass` and `keyProperty` must complete before agreement is tested.
  A dependency cycle through this completion boundary is refused. Property values
  remain positive native joins and can be derived in later rounds; the proof retains
  actual definition, membership, distinctness and shared-value statements. Unknown
  datatype equality cannot certify a failed comparison, and equal literal values
  do not erase their distinct lexical provenance. These guards do not themselves
  supply general resource-equality substitution or a complete key entailment calculus.

  Deterministic resource equalities from functionality, inverse functionality and
  object maximum/exact-one restrictions are positive producers in the shared native
  fixed point. Canonical characteristic records and direct property markers retain
  their actual schema and value premises. Inverse functionality compares native
  literal values without discarding lexical evidence; unknown comparisons refuse
  execution. Qualified object bounds require both fillers' membership, except that
  the universal object class needs no invented type assertion. Count fields use the
  same admitted syntax and bounded cache as the cardinality operators. A larger
  maximum requires a disjunction of possible equalities and never selects an
  arbitrary pair. Resource names remain intact, with no unique-name assumption or
  destructive representative substitution. These equality facts feed authored
  consumers, symmetry, transitivity and explicit-inequality contradictions in the
  same world; they do not alone certify complete equality congruence or whole-model
  completeness.

  Local self restrictions also participate in that fixed point in both directions:
  membership in a true `hasSelf` restriction entails its self edge, and the self
  edge entails membership. The flag uses native XSD boolean value interpretation;
  false flags and string literals do not enable local reflexivity. Every consequence
  retains its restriction, property and edge or membership premises.

  Class complement is authored as `logic:complementOf` between class expressions
  in their owning world. Its OWL predicate is a generated target view admitted by
  the shared grounding table, never a private reasoner alias. Complement is explicit
  logical opposition, not negation as failure or the absence of a membership row.
  A supported membership clash retains both memberships and the actual complement
  definition. It can support only its implicated subjects in that world; it cannot
  produce unrelated memberships or silently combine contexts. Case-split proof
  evidence and capability boundaries remain separate from local consequence.

  Maximum-cardinality witnesses are positive native producers. Each selected bound
  constrains its own explicit property and qualifier; a missing `onClass` or
  `onDataRange` never makes a qualified bound unqualified. Resource fillers require
  pairwise explicit distinctness, while literal fillers use datatype value equality
  without erasing their lexical premises. A zero maximum needs one qualifying
  filler. Independent bound assertions remain conjunctive. Qualified class counts
  consume actual membership statements from the same world; the universal object
  class needs no separately asserted membership. Named datatype counts use the
  shared exact value interpretation and PurRDF's supported XSD membership decisions;
  unavailable datatype mappings remain explicit refusals. Finite datatype expressions
  compile directly from the same world's completed native definitions into a shared
  expression DAG. Enumerations, complement, intersection, union and ordered/length
  restrictions retain every conjunctive facet and actual source premise. Constructor,
  list and facet producers must complete first; feedback through that boundary is
  refused. Plans are bounded and world-local, with no repeated lowering for retained
  definitions. Complement negates value-space membership, never missing RDF facts.
  Cyclic, incomplete, ambiguous or unsupported definitions fail admission. The datatype
  refutation reader shares these definitions and native value interpretations. Its
  range and universal constraints apply to existing values; each existential requests
  a separate witness in its filler, never a universal constraint on unrelated values.
  Qualified lower bounds require their explicit data range. Capacity bounds are
  derived lazily once per retained plan, using PurRDF's primitive XSD decisions and
  exact native rational/enumeration evidence. Unknown intersection capacity withholds
  a decision. Clash evidence retains actual class-inheritance paths and definition
  statements, rather than inventing a directly asserted restriction membership.
  Every asserted datatype-property value participates, including bare named ranges
  and the literal-only property contract. Unmodeled class and property obligations
  prevent a whole-source consistency certificate. Proof bounds retain the source
  integer independently of host pointer width; no source count sizes an allocation.
  Definition admission also covers currently unreachable constructors. A bounded
  world-local inventory shares inherited restrictions across individuals; proof
  paths retain their actual type and subclass statements.
  Object minimum and exact bounds produce compact native witness families. Qualified
  requests retain their explicit `onClass`; unqualified and `someValuesFrom` requests
  require an explicit object-property declaration. The universal object class admits
  existing resources without inventing membership evidence. Each family shares the
  native complete distinct-subset search with maximum checks; exhausting the selected
  search budget withholds completion and cannot authorize invention. Existing resource
  names alone never prove distinctness. No count expands the executable rule template:
  checked arithmetic bounds the entire conjunctive firing before allocating a linear
  witness inventory, then streams required property, type and pairwise inequality
  rows into the shared candidate buffer. World identity and all body-bound arguments,
  including count and definition carriers, determine witness identity. Each row retains
  its actual source premises. Ancestor-blocked model edges never enter this operation.

  Direct native rule execution and decidability certification enforce the same
  source-context admission as cached execution. Preparing a reusable structural
  template may retain a scoped source without executing it; that retained
  obligation must be admitted before a world runs. The positive-head rule
  template rejects negative heads and non-object-level statement
  kinds before lowering. A constraint or meta-level formula cannot be relabeled
  as an ordinary derivation, and an aggregate cannot become an ordinary join.
  Their complete canonical source remains available for a separately admitted
  execution fragment; a cache hit cannot grant a missing capability.
  Ordinary and annotated materialization enforce this admission for every selected
  profile, including stable-model and well-founded execution, and before reading a
  demand source. Hydrated operator preparations retain the same obligation. Native
  session classification preserves a source admission error rather than relabeling it
  as floundering after a second lowering attempt. Means-end sessions consume the
  producer-owned program together with its preparation, never a detached rule slice.

  **Native builtin values.** Forward and demand-driven joins pass borrowed RDF
  values directly to the shared exact-numeric evaluator. Numeric admission reads
  the literal's datatype, language and direction; an IRI or textual literal does
  not become numeric by its spelling. Math-cell probes borrow native IRIs. The
  reference evaluator supplies its existing surface bindings to the same value
  algebra and binding-mode decisions. Runtime bindings are never rendered and
  reparsed for successful evaluation; human-readable terms are created only when
  recording failure evidence. Post-join generators and filters retain the joined
  solution allocation and its supporting facts. A gap on an ordinary rule rejects
  the entire candidate batch, including candidates computed before that gap.
  Ordinary and joint fixed points stop before committing the failed round; forward
  materialization returns an explicit diagnostic and demand execution retains its
  typed unsupported outcome. Neither can certify a partially evaluated stratum.
  Constraint-tagged violation rules retain their distinct defined-false contract.
  This representation change grants no new numeric profile or rewrite authority.

  **Complete-group reduction.** A canonical rule carrying `aggregateFunction`,
  `aggregateVariable`, `aggregateResult` and `groupKey` lowers to a typed native
  reduce operator under `StratifiedNAFProfile`. The monotonic `PositiveHornProfile`
  does not admit this operator. `COUNT`, `SUM`, `AVG`, `MIN` and `MAX` use PurRDF's value-level
  accumulators directly; no query text, RDF serialization or temporary dataset
  is needed. The input and every group key must be positively body-bound, the
  result must be fresh outside the entire body and occupy the head object, and
  a variable head subject must be a group key. Inequality guards run before
  grouping and may only reference positively body-bound variables. Negated
  variables must also be positively body-bound.

  Each group is a set of complete body substitutions within one world. Equal
  aggregate values from distinct substitutions remain separate members;
  alternate proofs of the same substitution do not multiply it. Variable and
  member order is canonical. For an alternate proof the lexically least concrete
  source witness is retained, and the group's provenance is the ordered union of
  its members' positive support facts. Annotated execution names this contract
  `CompleteGroupSupport` and class `StratifiedAggregate`; it does not claim to
  enumerate all alternative aggregate proofs. Absence and completion are guards,
  not scored premises. A joint schedule with existential heads is explicitly
  classified `StratifiedAggregateChase` and carries selected physical lineage.

  Every reduce read requires all intersecting writers to finish in an earlier
  stratum. This includes ordinary, existential and data-selected schema writers;
  a strict dependency cycle is refused even without negation. A budget cut in an
  input stratum cannot publish a partial count or mark its result complete.
  Reductions produce finite seeds for later strata; their completed groups cannot
  participate in recursive value invention. Source-sensitive value-flow analysis
  conservatively retains their effects, including a keyless empty group.

  With group keys, an empty body extension has no groups. Without keys there is
  one global group, including an empty group. Explicitly selected worlds survive
  a source probe returning no relevant rows; no dummy RDF fact is asserted. The value-level numeric promotion,
  term ordering and empty `COUNT`/`SUM`/`AVG` results follow PurRDF; an undefined
  result (including empty `MIN`/`MAX` or a poisoned numeric fold) fails the whole
  selected operation instead of omitting a conclusion. Non-monotone reducts and
  the ordinary signed incremental circuit do not represent complete-group
  maintenance and reject that operator explicitly. Ordinary stratified native
  execution, including annotations and joint producer schedules, executes it.

  A fixed two-ordinal position abstraction covers the value flow of every finite
  witness family, including all frontier dependencies and inequality positions.
  This representation admits only the position-based weak-acyclicity proof; two
  representative witnesses cannot certify arbitrary nonlinear joins through the
  stronger critical-instance analyses. When static definitions fix every predicate,
  analysis retains the native binary relations and class positions directly.
  Source-independent termination admission remains preferred. A source-bound proof
  may additionally specialize analysis through relations that the complete reachable
  value-flow analysis proves immutable. Native indexed joins enumerate every binding
  of those definitions; mutable body atoms and all remaining frontier dependencies
  remain in the proof. The union across worlds can add matches but cannot omit a
  concrete firing. Native execution retains its original shared layouts and exact
  spellings. This is a sufficient termination proof for the admitted input, not
  bounded-run evidence or a certificate for unrestricted rewrites.

  If mutable selector relations prevent that proof, the same abstract interpreter
  can distinguish at most 512 exact native value cells from the selected source
  metadata. Its complete reachable domains include every later producer. Only
  variables whose domain excludes `Other` are specialized, enumerating every
  remaining constant combination; possible future witness values stay variable.
  This admits finite derived selector chains without running a second concrete
  closure or re-lowering executable rules. The same position-proof restriction and
  complete-enumeration limits apply.

  Specialization retains at most 4,096 source rows and one MiB of source metadata,
  and at most 4,096 specialized rules and one MiB of rule metadata. Incomplete
  enumeration supplies no new certificate: the original refusal or explicit runtime
  budget still applies. At most eight bounded abstract value-flow analyses are shared
  per template, each limited to one MiB of formatted metadata; no facts or worlds
  enter that cache. Executable admission also binds the complete exact immutable
  source facts, including their asserting worlds, into the input contract without
  retaining the source dataset in a cache. The wider identity is required because
  enriched cells can distinguish formerly unknown values in dynamic-predicate rows
  too. Changing those
  definitions cannot reuse a finite-program certificate through the abstract `Other`
  value cell. Published joint certificates retain this input contract explicitly.
  Minimum witness generation, datatype model construction and whole-source consistency
  remain distinct claims; no maximum witness or finite chase alone certifies a complete
  consistent model.

  Datatype contradictions are also native positive producers. Declared datatype
  properties require literal objects; their ranges and each `allValuesFrom` constrain
  actual values. Each existential filler and each selected lower/exact count can
  contradict a proved datatype capacity. An independently insufficient universal
  range is enough to prove a clash, while distinct existential fillers are never
  intersected merely because they share a property. Qualified lower bounds require
  the explicit `onDataRange` premise; no range is substituted for a missing qualifier.
  The prepared operator reuses the world's native definition and value caches,
  preserves every actual body and definition premise, and publishes the contradiction
  into the same authored-rule fixed point. Constructor/list/facet reads are completed
  dependencies, so their feedback cycles are refused; actual values and restrictions
  remain positive inputs. Undefined membership or capacity is an explicit execution
  refusal, never a negative decision. The typed operation choice admits one list,
  cardinality or datatype operator per layout. These contradictions do not supply
  general intersected value-space models or construct missing existential witnesses.
  Primitive finite capacities come directly from PurRDF's datatype authority,
  including its exact and lower-bound results. Authenticated observations check
  every authored math capacity against that same native answer; no parallel Rust
  inventory restricts which primitive spaces the engine can decide.

  Compact axiom and rule objects share the relational core's atomic term algebra: variable,
  IRI, blank node, or complete RDF 1.2 literal. Literal datatype, language and direction
  participate in content identity and remain native across lowering and projection. Plain
  `"?x"` is variable syntax only at the authored rule-field ingress; an explicit
  `logic:termLiteral` carrier denotes a question-mark literal there. A domain literal never
  becomes a variable by lexical inspection. A target unable to express a typed literal must
  disclose and omit its whole affected axiom or rule; collapsing it to an untyped value does
  not establish sound under-approximation. Binary assertions using an external predicate
  or a logical variable retain an explicit Formula declaration in canonical RDF; arbitrary
  domain triples do not become logical assertions by mere presence. Scoped reification
  admits the selected proposition independently of its predicate namespace, validates its
  RDF term roles, and binds its full contextual envelope into reifier identity. Its scope
  fields belong to that envelope, not to the domain axiom set. Canonical projection never
  adds an unscoped assertion for a scoped claim; a separately authored global claim is
  retained as an independent assertion.

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

Canonical formula literals carry the complete native RDF 1.2 value: lexical form,
datatype, language tag and base direction. `logic:termLiteral` projects that value
directly. An explicit `logic:termLiteralDatatype` declares the datatype of a plain
lexical carrier and must agree with an already typed or language-tagged value;
it never overrides native identity. A contradictory declaration is malformed.
The same native value is retained by relational-core terms and validation values,
including cache restoration. Binary facts with a resource or variable subject use
the shared compact atomic representation even when their object is a rich literal.
Literal subjects and other non-triple forms retain the full Formula carrier.
OWL cardinality emission validates the source count with the native datatype engine
and writes the target-required `xsd:nonNegativeInteger`; canonical output retains
the authored datatype and lexical value. A non-integer count refuses that projection.

Relational program identity canonicalizes one native structure containing facts,
rules and their shared blank constants. It includes polarity, complete literal
values, source presence and loss residue. Canonicalization refusal is an error;
it cannot authorize a fallback identity. Atom and rule component keys use length
framing, so separators embedded in values cannot change their field boundaries.

The relational-core transport is a native RDF 1.2 dataset shared by artifact output
and the carrier. A `logic:RelationalCoreProgram` record is distinct from the
`logic:RelationalCore` dialect individual. Each atom records its sign, including
facts and additional head conjuncts. A typed `logic:RelationalCoreTerm` record with exactly one `logic:rcIri`,
`logic:rcLiteral` or `logic:rcVariable` value distinguishes native IRIs,
literals and exact variable names in either position; blank constants retain their
shared native identity. These metadata records assert no object-level facts. Production GTS carries the
records directly; compact quoted envelopes are confined to temporary canonical
identity graphs.
These position-independent atomic values are distinct from `logic:TermCarrier`,
whose ordered formula slots require indexes and sigil-free source variable names.
There is no datatype-based variable inference or `rcObjectLiteral` side marker.

The projection reader uses the native default-graph indexes. Scalar fields are
single-valued and datatype-checked, record links are typed, and both positional
head and body lists must have contiguous unique indexes. The declared preservation
judgment must agree with explicit loss evidence. Missing or ambiguous fields refuse;
unrelated named graphs cannot supply a missing field. Semantic program identity
identifies reordered body conjunctions, while the stricter projection identity also
binds body order and repeated occurrences. Both preserve blank sharing modulo
relabeling. Cache publication checks this complete transport identity against the
backing graph; hydration restores the authenticated full typed payload.

The flat Horn lowering does not admit contextual scope on an axiom, a rule, its
head or any body atom. It records source-identified residue rather than emitting
an unscoped statement, including when the only context is provenance, confidence,
time or theory membership. A negative rule head is likewise residue, never a
positive consequence. The original typed logic program remains the canonical
owner of these constructs. This explicit boundary does not complete the separate
contextual source/model calculus or authorize optimizations across its scope.

### Interpreted finite RDF numeric relations

Canonical Formula antecedents may select `logic:rdfNumericAdd`,
`rdfNumericSubtract`, `rdfNumericMultiply` or `rdfNumericDivide` with operands
`(left, right, result)`, or `rdfNumericEqual`, `rdfNumericNotEqual`, `rdfNumericLess`,
`rdfNumericLessOrEqual`, `rdfNumericGreater` or `rdfNumericGreaterOrEqual` with
`(left, right)`. These identities explicitly select finite RDF numeric values
(integer and its derived datatypes, decimal, float and double). They do not
reinterpret arbitrary mathematical function symbols or the exact rational and
dimensional query builtin domain. `logic:invokesBuiltin` remains dependency
metadata for its declared procedural builtin profile, never an operand AST.

The one Formula clausifier retains numeric calls as typed relational instructions,
not reified data tuples. A canonical dependency schedule binds both inputs before
executing a call. A fresh result variable receives the computed value; an already
bound result requires numeric value equality. Comparisons filter bound inputs.
There is no implicit inverse solving. Unbound or cyclic input dependencies,
malformed arity and assertions of interpreted predicates as heads remain explicit
lowering residue. Every operand and its literal metadata, operator, result and
schedule position participates in program and transport identity and round-trips
through the native relational projection reader. A numeric output variable is
bound by the body; it is distinct from an explicitly existential head witness.

Ordinary and conjunctive existential rules share an immutable numeric plan.
Constants are decoded once per prepared plan. Each solution decodes a referenced
variable at most once, and later calls reuse computed values directly. PurRDF's
public scalar kernels own numeric promotion, arithmetic and comparisons; no query
text, temporary dataset or copied generic arithmetic implementation is involved.
The adapter retains all source-fact support. Standpoints remain separate native
worlds. Numeric type, precision and domain errors, nonfinite operands or results,
and undefined operations refuse the entire selected operation before the failed
round publishes. They never become a false result, skipped check or completion
certificate. A valid, defined comparison returning false is an ordinary filter.

Unbudgeted value-creating numeric producers require the joint position-based
weak-acyclicity certificate. The conservative abstraction treats their outputs as
fresh possible values dependent on **every positive body variable**, including
inputs absent from the head. The ordinary witness frontier (only copied head
variables) is insufficient for arithmetic. The abstraction retains all ordinary, existential
and schema feedback edges. Arithmetic may identify distinct symbolic values, so
critical-instance tuple proofs cannot certify this abstraction. A source-bound
specialization retains this position-only restriction. Bounded execution remains
bounded evidence. Numeric rules use the certified joint producer route even when
they have no existential head. Current reduct and signed incremental paths refuse
these instructions explicitly; they cannot erase arithmetic on a cache hit or an
empty input. This execution contract grants no correspondence independence,
probability model, law discharge or unrestricted rewrite certificate.
