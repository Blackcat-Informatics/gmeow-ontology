<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — Runtime and Engine Architecture

> Status: target architecture for `logic:`. This is the **runtime** member of the
> [GMEOW Logic document set](LOGIC.md#the-document-set). It defines the native solver, the
> compiler/runtime split, the forward materialization ↔ backward resolution seam and its data
> contract, graph versioning, the generated artifacts and their preservation judgments, the
> prose-explanation surface, and the reasoning command surface. The semantics these realize are in
> [LOGIC-SEMANTICS.md](LOGIC-SEMANTICS.md); the transaction-path execution semantics are in
> [LOGIC-TRANSACTION.md](LOGIC-TRANSACTION.md); the typed intermediate representation is in
> [LOGIC-IR.md](LOGIC-IR.md).
>
> **Reading the status labels.** Claims that touch implementation are tagged with one of three
> labels: **Normative semantics** (what the formal model requires of any conforming engine),
> **Currently implemented subset** (what the runtime evaluates today), and **Required but not yet
> implemented** (a normative obligation no engine yet discharges). An untagged statement is
> normative semantics by default.

## The compiler/runtime split

The `logic:` implementation is divided along a clean architectural boundary: a **compiler core**
and a **reasoning runtime**.

The **compiler core** is a pure, self-contained pipeline: it parses `logic:` source, constructs
the typed IR described in [LOGIC-IR.md](LOGIC-IR.md), performs the projection lowerings, and emits
the canonical RDF 1.2 artifact and every target projection. It carries no reasoning machinery — no
solver, no chase engine, no unification — and therefore has no runtime dependencies on those
systems. This portability is the point: the compiler core can be embedded in constrained or
resource-limited environments (offline tooling, embedded targets, browser contexts) where the full
reasoning runtime is unavailable or unwanted. The compiler's only output is the IR, the generated
artifacts, and the preservation judgments (see [LOGIC-IR.md § Lowering and the preservation
judgment](LOGIC-IR.md)).

The **reasoning runtime** is the engine layer: it receives the IR and the generated canonical
artifact, operates the materializer and the goal resolver, constructs and maintains typed contexts
realized as named graphs, and produces typed `logic:ReasoningResult` values (see
[LOGIC-SEMANTICS.md § The reasoning result](LOGIC-SEMANTICS.md)). The runtime is not portable in
the same sense — it depends on capable solver substrates and is intended for server-side and
developer-side use. The split ensures that a consumer who needs only compilation (cross-compilation,
static projection, documentation generation) never takes a dependency on the full solver stack.

Both halves share the same typed IR as their interface. The compiler produces it; the runtime
consumes it.

## The native solver

The reasoning runtime is built around a **single canonical native solver**. It operates directly
over the RDF 1.2 canonical artifact — no OWL downcast is needed to understand axiom identity,
statement metadata, or contextual scope. The solver runs **forward and backward**: materialization
and classification, *and* goal resolution by backward chaining and unification.

Required solver capabilities:

- load and reason over RDF 1.2 triple terms and reifiers;
- materialize declared rule consequences (monotonic and stratified non-monotonic), under the
  reasoning contract declared by the request (see [LOGIC-SEMANTICS.md § The reasoning
  contract](LOGIC-SEMANTICS.md#the-reasoning-contract));
- resolve goals by unification and backward chaining, with builtins (cut only in the procedural
  preset — see [LOGIC-SEMANTICS.md § Cut is procedural](LOGIC-SEMANTICS.md#cut-is-procedural-not-canonical));
- evaluate closed-world constraints over asserted and derived graphs;
- carry contextual/temporal/modal/probabilistic scope through inference, and construct
  hypothesized/counterfactual typed contexts on demand (see
  [LOGIC-SEMANTICS.md § Worlds, Modality, and Counterfactuals](LOGIC-SEMANTICS.md#worlds-modality-and-counterfactuals--a-typed-context-algebra));
- treat contradiction paraconsistently and report **contradiction witnesses** rather than only
  failing;
- **explain** derived triples, refusals, and required shapes with rule and source provenance (see
  [Explanation as projection](#explanation-as-projection-logic-to-prose));
- support tiered validation, from fast structural checks to deep-reasoning checks (see
  [Validator tiers](#validator-tiers));
- emit stable artifacts suitable for drift detection.

## Substrate roles

The runtime is composed from a small set of distinct, well-scoped substrate roles. Each role is
filled by a component optimized for that purpose; the architecture does not conflate them.

- **Storage, SPARQL, and RDF 1.2 triple terms.** A high-performance, RDF-1.2-native quad store
  fills this role. It serves SPARQL evaluation, constraint evaluation, and goal-as-query
  resolution over materialized named graphs. RDF 1.2 places triple terms only in object position;
  the canonical interchange surface is normative RDF 1.2 Full (see
  [LOGIC-SEMANTICS.md § RDF 1.2 triple terms, precisely](LOGIC-SEMANTICS.md#rdf-12-triple-terms-precisely)).
  The storage component doubles as the shared blackboard between the materializer and the goal
  resolver — their only shared state.
- **Forward materialization / existential rules.** A datalog-plus-existential-rules chase engine
  fills this role, providing the existential-rule substrate: Datalog extensions with datatypes,
  aggregates, stratified negation, and existential variables in rule heads that create fresh
  objects. This substrate is also the typed-context construction engine: `logic:` defines the
  typed-context construction *protocol* (graph seeding, revision, scoped chase, memoization,
  provenance capture, and disposal) layered on top of it. The architectural insight that
  existential chase approximates typed-context construction is a `logic:`-level design choice,
  not a property the substrate itself ships.
- **Backward goal resolution.** A Prolog-grade SLD resolution engine fills this role — unification,
  backward chaining, builtins, and tabling. Cut is confined to the procedural preset. The backward
  engine reads the materialized store as read-only data; it never writes back to it except under
  the explicit materialize-back policy described below.
- **Proof traces and provenance.** Derivations are first-class, keyed by rule IRI and the source
  statements' reifier IRIs — reusing the statement layer's reifier identity.
- **Contradiction witnesses.** Minimal conflict-set detection plus truth-maintenance; a witness is
  emitted as a GMEOW statement graph (paraconsistent), not a bare failure.

### Substrate lineage

The materializer role is filled by a Rust-native existential-rule engine (**Nemo** — knowsys);
the backward-resolution role by an embedded Rust Prolog (**Scryer**); storage and SPARQL by a
Rust-native RDF 1.2 quad store (**oxigraph**). These are conceptual substrate choices — named here
because the design decisions (asymmetric pipeline, blackboard handoff, Kripke algebra mapped to
typed named-graph contexts) are grounded in what those engines commit to. They are not build-time
feature flags or replaceable components the architecture is neutral about; the seam design is
written *for* these substrates. External OWL reasoners (e.g. ELK, HermiT) remain available for
checking the OWL projections of the IR, but they are secondary validators of their projected
fragments — not authorities over the canonical `logic:` semantics.

> Ownership boundary. The existential-rule substrate provides the chase *mechanism* used by
> `logic:` typed-context construction — it is not, by itself, a context-construction engine.
> `logic:` defines the typed-context construction *protocol*: graph seeding, revision, scoped
> chase, memoization, provenance capture, and disposal. The architectural insight ("existential
> chase ≈ context construction") is a `logic:`-level design choice layered on that substrate.

## The forward materialization ↔ backward resolution seam

The runtime interface between the **materializer** (forward) and the **goal resolver** (backward)
is an **asymmetric pipeline**: the two components never call each other directly. They communicate
only through named graphs in the shared quad store, where the graph IRI identifies a typed context.
The materializer writes; the goal resolver reads; the quad store is the shared blackboard. The
three runtime phases map one-to-one to the three
[strata](LOGIC-SEMANTICS.md#three-strata-of-context-reasoning).

**Phase 1 — materialize (forward engine owns it; Strata A and B).** The forward-stratified rules
— the lowered modal constraints, the frame-indexed contexts, the type-level no-occurrence rules —
run to fixpoint and write their closure into named graphs. **Normative semantics:** termination
within this phase is guaranteed only when the contract's `Resource` facet certifies a terminating
fragment (weakly-acyclic, jointly-acyclic, or other certified-sufficient condition). Stratified
Datalog rules over a finite domain terminate; existential rules outside a certified acyclicity
condition do not. The result, when the phase completes, is a saturated, read-only **extensional
database (EDB)**.

**Currently implemented subset:** Phase 1 runs under stratified negation with the no-occurrence
gate enforced, which provides the termination guarantee for the type-level fragment currently
evaluated.

**Required but not yet implemented:** Full existential-rule certification and the runtime
enforcement of the contract's certified-fragment annotation on Phase 1 termination guarantees.

**Phase 2 — resolve (backward engine owns it; the query / logic-programming layer).** Backward
goal resolution runs over the materialized store as read-only data. A base predicate is resolved
by a foreign predicate `in_world(W, S, P, O)` backed by a quad lookup; the Prolog clauses are the
**intensional database (IDB)**, the recursive, unification-driven part the forward engine cannot
express. Non-recursive pattern goals route to SPARQL (the fast path); recursive or
unification-heavy goals go to the backward engine. **Normative semantics:** termination in Phase 2
is not automatic — unrestricted recursion, stable-model evaluation, and procedural goal resolution
can be non-terminating unless the contract certifies a terminating fragment (e.g. Datalog
restriction, tabling with finite tables, bounded search). The resource budget acts as a backstop,
returning `evaluation = budget-exhausted` rather than diverging; it does not make the phase
"terminating by construction."

**Phase 3 — construct (backward engine invokes a transient chase; Stratum C only).** When a
backward goal reaches a counterfactual predicate — `holds(W_cf, φ)` with
`W_cf = counterfactualOf(W_base, A)` — the backward engine calls `construct_context(W_base, A,
W_cf)`. That seeds a fresh, **isolated** named graph from the base context minimally revised to
admit `A` (the AGM step, made deterministic by a declared entrenchment ordering — see
[LOGIC-SEMANTICS.md § Deterministic revision](LOGIC-SEMANTICS.md#deterministic-revision-taming-the-agm-mutation-explosion)),
runs an **isolated, transient chase** scoped to that context, and only then resolves `φ` inside
it. The constructed context is per-query and discarded afterward — or memoized (see
[Graph versioning](#graph-versioning-and-staleness)). Isolation preserves paraconsistency: a
counterfactual context is a separate graph; nested counterfactuals are nested transient graphs
bounded by the depth budget. This is the *primary* place generative, undecidable work happens —
the primary place a chase is spawned on the fly, and where the governor returns `incomplete`. The
resource budget governs termination here; the phase is non-terminating in general outside
certified fragments.

**Provenance and witnesses cross the seam uniformly** — every derived quad, whether
materializer-produced, resolver-produced, or built during a Stratum-C chase, carries its rule-IRI
and source-IRIs, so one proof trace (and the prose explanation composed from it) spans both
components without a seam in the narrative.

Two honest later optimizations, neither required for correctness: **demand-driven materialization**
(magic-sets over the forward engine) replaces full Phase-1 closure when the base is large and the
query narrow; and **incremental maintenance** keeps the materialized store fresh under base edits
without a full re-chase — the harder win, since the forward engine is not incremental, and the
reason that capability is one the custom solver layer must eventually own.

### The seam data contract

The blackboard is a *typed* data contract, not an ad-hoc dump. Every materialized quad carries its
derivation metadata, and the backward engine reads both data and provenance through fixed foreign
predicates.

```text
Materializer output (per derived quad written to the quad store):
  graph:           IRI            # the typed context the quad belongs to
  quad:            (S, P, O, G)   # the quad itself (G == graph)
  derivation_id:   IRI            # stable id for this derivation step
  rule_iri:        IRI            # the rule that fired
  source_quad_ids: [IRI]          # the antecedent quads consumed
  contract_hash:   IRI            # content-addressed hash of the reasoning contract in force
  budget_status:   enum           # ok | partial | exhausted

Goal resolver foreign predicates (read-only over the materialized store):
  in_world(+W, ?S, ?P, ?O)                  # base-predicate lookup, context-indexed
  derived_by(?QuadId, ?Rule, ?Sources)      # provenance leg for explanations
  contradiction_witness(+W, ?WitnessGraph)  # within-context inconsistency, as a statement graph
```

The `contract_hash` field is the content-addressed identity of the `logic:ReasoningContract` in
force when the quad was derived. Cache validity, provenance attribution, and drift detection all
key on this hash — not on a profile identifier or opaque mode name. Any change to the contract
(facet values, resource budget, certified fragment declaration, or solver version) produces a
distinct hash and invalidates downstream cached results.

**Materialize-back policy.** Resolver-produced answers are, by default, **not** written back into
the quad store: Phase 2 is a read-only query layer, and its derivations are *virtual* —
explanations cite them as virtual derivation steps keyed by `derivation_id`, not as stored quads.
Two explicit exceptions: a Stratum-C constructed context *is* materialized (into its own transient
named graph, under the versioning key below), and a query may request memoization of a recursive
IDB predicate, which writes a clearly-marked derived graph carrying the same derivation metadata.
In all cases the rule holds: **no resolver answer is silently promoted to an asserted base fact**,
and an explanation must be able to cite every step, virtual or materialized.

## Graph versioning and staleness

Because the solver uses materialized stores, transient typed contexts, and memoized
counterfactuals, stale results are a real risk. Every materialized context graph is therefore
**content-keyed**, following the same content-hash discipline the generator framework uses for
drift detection:

A materialized context graph is keyed by `(source_graph_hash, rule_set_hash, contract_hash,
solver_version, budget_params)`. A cached counterfactual context is valid **only** for the exact
tuple:

```text
(base_context_hash, antecedent_hash, rule_set_hash, entrenchment_hash, contract_hash, solver_version)
```

Any change to a component invalidates the cache entry and forces reconstruction. The `contract_hash`
is the content-addressed identity of the full `logic:ReasoningContract` — including all facet
selections, resource budget, and the certified-fragment declaration — so any change to reasoning
configuration produces a new hash and invalidates the entry. This is the content-hash discipline
the generator framework already uses for drift detection, applied to the solver's materialized and
transient graphs.

## Validator tiers

The runtime exposes two validation tiers, serving different latency and completeness requirements.
They are not separate modes with separate invocations; they are levels within a single validation
surface selected by the reasoning contract's `Resource` facet.

**Structural tier (fast, always available).** This tier checks every condition that can be decided
without reasoning: annotation completeness, label/definition/slice-anchoring invariants, shape
conformance over the asserted graph, IR well-formedness, and stereotype cardinality. It runs
without engaging the materializer or goal resolver and produces findings immediately. Every
generated artifact passes this tier before it is considered a candidate for release.

**Deep-reasoning tier (opt-in, bounded by contract budget).** This tier engages the full
solver — Phase-1 materialization, Phase-2 goal resolution, and, where permitted by the contract,
Phase-3 counterfactual construction. It checks cross-context rigidity, modal consistency,
foundation discipline violations that require derivation, and the semantic integrity of
probabilistic models. It produces `logic:ReasoningResult` values with the full **compositional
status** — the five orthogonal fields (input, evaluation, completeness, preservation, information)
described in
[LOGIC-SEMANTICS.md § The reasoning result](LOGIC-SEMANTICS.md#the-reasoning-result). Budget
exhaustion is a normal outcome; the result discloses it explicitly rather than returning a false
answer.

The structural tier is a prerequisite: the deep-reasoning tier is invoked only on inputs that
already pass the structural tier. A finding from either tier is a `logic:violation` in the shared
result format; there is no finding type exclusive to one tier.

## Transaction-path execution

When the reasoning contract selects `Evolution = transaction-path` (see
[LOGIC-TRANSACTION.md](LOGIC-TRANSACTION.md)), the solver evaluates queries over **paths** rather
than single states. The runtime realizes this by mapping path semantics onto the typed-context
layer: each state in the path is a named graph in the quad store; elementary transitions (`ins`,
`del`) advance the path by producing successor named graphs; serial conjunction (⊗) partitions
the path into prefix and suffix subpaths resolved independently.

The materialize-back policy is extended: a committed transaction path writes its successor state
as a named graph with the same derivation metadata as any other materialized graph. A
*hypothetical* transaction (the sandbox operator of [LOGIC-TRANSACTION.md](LOGIC-TRANSACTION.md))
uses the same transient named-graph mechanism as a Stratum-C counterfactual context — isolated,
budget-bounded, discarded unless explicitly promoted.

Serializability checking for concurrent transaction paths produces **history-level findings**,
not contradiction witnesses. A non-serializable interleaving is a `SerializationAnomaly`: a
dependency cycle or conflicting-operation pattern in the transaction history that admits no
equivalent serial execution under the declared isolation policy. The final state after such a
schedule may be perfectly logically consistent; `SerializationAnomaly` is therefore distinct from
a contradiction witness (which asserts ⊥ within a context) and is never modelled as one. Lost
updates, write skew, and read/write anomalies are findings of this kind, described by their
dependency cycle or violated isolation level — never silently linearized, and never conflated with
within-context inconsistency.

## Generated artifacts and the compiler's projection role

The compiler core (see [The compiler/runtime split](#the-compilerruntime-split)) emits a set of
generated outputs alongside the canonical RDF 1.2 logic artifact. Every output is owned by a
registered generator and subject to drift detection; no generated artifact is hand-edited.

```text
generated/logic/gmeow.logic.rdf12.ttl     # the canonical RDF 1.2 logic artifact
generated/logic/projection-report.ttl     # loss + preservation ledger across all targets
generated/owl/gmeow-dl.ttl                 # OWL 2 DL projection (truth-preserving fragment)
generated/owl/gmeow-el.ttl                 # OWL 2 EL projection
generated/datalog/gmeow.dl                 # Datalog projection
generated/n3/gmeow.n3                       # N3 rules projection
generated/foundation/gufo.ttl              # gUFO down-projection of UFO⁺
```

The compiler follows the same typed-IR projection pattern the rest of the system uses: compile
`logic:` (and adapter-phase `owl:*` / `gufo:`) into the one typed IR described in
[LOGIC-IR.md](LOGIC-IR.md), then emit the canonical artifact and every projection from it. Each
projection carries the preservation judgment produced by the lowering — exact, sound-but-incomplete,
complete-but-possibly-unsound, validation-only, or unsupported — and the aggregate of those
judgments is the loss/preservation ledger in `projection-report.ttl`. External OWL reasoners
running over the OWL projection artifacts prove only the projected subset, never full `logic:`
consistency.

## Explanation as projection: logic to prose

A logic this expressive is humane only if it can say *why* in words — and `logic:` can, because
GMEOW already mandates the raw material. Every term has an `rdfs:label`, a `skos:definition`, and
`skos:scopeNote`/`skos:example` where the documentation doctrine applies; the annotation contract
requires a label, a definition, and a defining slice on every term. So at every node of a proof
the solver has vetted human text. **Prose is another projection target — the human, explanatory
surface — generated and gated like OWL or Datalog.**

The explanation component composes annotations *along a real derivation*: *why derived* (a proof
trace), *why disallowed* (a contradiction witness), *what shape is required* (a constraint) — each
clause the annotation of a node the solver actually used.

The crucial property is **faithfulness by construction**: an explanation is a deterministic
composition of vetted annotation text along a proof the solver produced, not a language model
guessing. Every term the prose cites appears in the proof trace or witness graph, so the
explanation can be *validated* (no cited term outside the trace), not merely trusted. A language
model may *polish* the prose; the skeleton — which axioms, rules, and sources, in which order — is
provable and checkable (the conformance contract for this is in
[LOGIC-CONFORMANCE.md](LOGIC-CONFORMANCE.md)). This makes annotation expressivity load-bearing:
the `skos:scopeNote`/`skos:example` annotation backlog is **fuel for the explanation surface**,
not cosmetic debt. The `gmeow describe` surface already renders term-level prose; explanation
generalizes it to a whole derivation. For the stable parts — foundation axioms and the constraint
catalogue — the rendered rationale is itself a generated, drift-gated artifact; per-query
explanations are produced at reasoning time.

## Reasoning command surface

The solver is exposed through two command-surface roles:

The **compile role** drives the compiler core: parse `logic:` source → construct the typed IR →
emit the canonical artifact and projections → check those artifacts for drift. It is the compiler
half of the compiler/runtime split and may run without the reasoning runtime.

The **reason role** drives the full reasoning runtime: it selects the engine and the fragment via
the reasoning contract. The native solver (the full runtime described here) is the authority for
canonical `logic:` semantics. OWL-projection reasoning (DL or EL fragment) runs the respective
OWL reasoner over the corresponding projected artifact — a secondary validator of that fragment,
not an authority over `logic:` itself. A Datalog-projection mode similarly reasons over the
Datalog projection.

The **explain and query roles** expose derivation traces and backward goal resolution,
respectively, as first-class operations on the running solver.

---

*Engines and tools named here — oxigraph, Nemo, Scryer, ELK/HermiT, ProbLog, EYE/cwm — are
listed in [LOGIC-REFERENCES.md](LOGIC-REFERENCES.md).*
