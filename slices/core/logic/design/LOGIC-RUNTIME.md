<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — Runtime and Engine Architecture

> Status: runtime specification for `logic:`. This is the **runtime** member of the
> [GMEOW Logic document set](LOGIC.md#the-document-set). It defines the native solver, the
> substrate stack, the Nemo–Prolog seam and its data contract, graph versioning, the generated
> artifacts, the prose-explanation surface, and the CLI. The semantics these realize are in
> [LOGIC-SEMANTICS.md](LOGIC-SEMANTICS.md); the rollout order is in
> [LOGIC-MIGRATION.md](LOGIC-MIGRATION.md).

## The native solver

The solver operates directly over the RDF 1.2 canonical artifact. It does not require an OWL
downcast to understand axiom identity, statement metadata, or contextual scope, and it runs **forward
and backward**: materialization and classification, *and* Prolog-style goal resolution.

Required capabilities:

- load and reason over RDF 1.2 triple terms and reifiers;
- materialize declared rule consequences (monotonic and stratified non-monotonic), under the
  declared [semantic profile](LOGIC-SEMANTICS.md#semantic-profiles);
- resolve goals by unification and backward chaining, with builtins (cut only in the procedural
  profile — see semantics);
- evaluate closed-world constraints over asserted and derived graphs;
- carry contextual/temporal/modal/probabilistic scope through inference, and construct
  hypothesized/counterfactual worlds on demand;
- treat contradiction paraconsistently and report **contradiction witnesses** rather than only
  failing;
- **explain** derived triples, refusals, and required shapes with rule and source provenance (see
  [Explanation as projection](#explanation-as-projection-logic-to-prose));
- support profile modes, from fast local checks to release-grade checks;
- emit stable artifacts suitable for the `make check-generated` drift gates.

## Substrate

The architecture reuses what is **optimal**, not merely what is present.

- **Storage, SPARQL, and RDF 1.2 triple terms:** `pyoxigraph` (oxigraph; Rust; ~12× faster than
  rdflib, already used in `src/gmeow_tools/projections.py` and the `crosscheck` lane). Oxigraph
  **dropped RDF-star in favor of RDF 1.2**, and RDF 1.2 places triple terms only in object position
  — the canonical interchange surface is normative RDF 1.2 Full (see
  [LOGIC-SEMANTICS.md](LOGIC-SEMANTICS.md#rdf-12-triple-terms-precisely)). Optimal in-repo substrate
  for SPARQL, constraint, and goal-as-query evaluation.
- **Closest RDF-native logic kin:** N3 Logic with EYE/cwm — RDF rules, quoted graphs, builtins, both
  chaining directions. `logic:` is its maximal successor.
- **Backward chaining (chosen): embed Scryer Prolog.** `logic:` embeds **Scryer Prolog** (Rust) for
  SLD resolution, unification, and builtins — the simplest route to Prolog-grade backward search and
  the committed starting point. (Trealla is a drop-in Rust alternative if Scryer ever blocks; the
  seam below is engine-agnostic.) Goal-directed materialization by magic-sets over Nemo is a later
  optimization, not a prerequisite. Cut is confined to `logic:ProceduralPrologProfile`.
- **Materialization / existential rules: Nemo.** **Nemo** (knowsys; Rust) provides the
  existential-rule / chase substrate: Datalog extensions with datatypes, aggregates, stratified
  negation, and existential variables in rule heads that create fresh objects. **Soufflé** is
  available for the monotonic fragment; `owlrl` is retained only as an OWL-RL **cross-check oracle**
  (`tests/test_reasoning_entailments.py`), never the canonical engine. GMEOW takes **no dependency on
  a closed engine**: the capabilities a commercial system like RDFox demonstrates (incremental
  maintenance, built-in derivation tracing) are targets the custom Rust layer provides natively,
  because a canonical engine cannot rest on proprietary availability (Principle 5).
- **Proof traces and provenance:** derivations are first-class, keyed by rule IRI and the source
  statements' reifier IRIs — reusing the statement layer's reifier identity.
- **Contradiction witnesses:** minimal conflict-set / unsat-core plus truth-maintenance (ATMS/JTMS);
  a witness is emitted as a GMEOW statement graph (paraconsistent), not a bare failure.

The native solver is cross-checked against the legacy reasoners on each overlapping fragment, using
the pattern already proven by `make crosscheck` (`gmeow crosscheck-queries`), which proves rdflib and
pyoxigraph answer every committed query identically with no Docker.

## Engine architecture: a Rust core behind a Python oracle

The stack follows a pattern GMEOW has already committed to for the Graph Transport Substrate
(GTS conformance design, *"Rust core + PyO3/wasm bindings, gated by a language-neutral conformance corpus"*; a
`crates/gts/` Rust crate already exists, with pyoxigraph named as the architectural model):

- **Python is the conformance oracle, not the production engine.** The reference solver — the `owlrl`
  / pyoxigraph / pySHACL orchestration — is the executable spec: slow, simple, correct. It defines the
  verdicts every other engine must reproduce.
- **The Rust core is the production engine**, in three layers: **oxigraph** (storage, SPARQL, RDF 1.2
  — a possible world is a named graph); **Nemo** (existential-rule materialization); and **custom
  Rust** (the `logic:` IR compiler and `owl:*`/`gufo:` adapters, the modal/foundation lowering,
  backward resolution via embedded **Scryer**, world-construction orchestration and closeness
  ordering, provenance/proof-trace capture, contradiction-witness extraction, the budget/profile
  certifier, and the explanation skeleton).
- **PyO3 and wasm are the bindings.** PyO3 makes the Rust core a drop-in behind the `gmeow logic`
  Python API, cross-checked against the oracle like the rdflib≡pyoxigraph gate (Principle 7). A wasm
  binding puts the reasoner in the browser — reasoning over a GTS package client-side — serving the
  AI-first and "open anywhere" goals.

The safety net is the corpus conformance gate defines (see [LOGIC-CONFORMANCE.md](LOGIC-CONFORMANCE.md)): one
language-neutral corpus that both the Python oracle and the Rust engine pass identically. **The
compiler itself is bound for Rust**: Python's role narrows to the oracle and thin orchestration, while
the IR compiler, lowering, solver, and projections consolidate in the Rust core. The corpus gate makes
each of these moves safe to evolve.

> Ownership boundary. Nemo provides the existential-rule/chase *substrate* used by `logic:` world
> construction — it is not, by itself, a world-construction engine. `logic:` defines the
> world-construction *protocol*: graph seeding, revision, scoped chase, memoization, provenance
> capture, and disposal. The architectural insight ("existential chase ≈ world construction") is a
> `logic:`-level design choice layered on Nemo, not a property Nemo ships.

## The Nemo–Prolog seam: an asymmetric pipeline

The runtime interface between the forward engine (Nemo) and the backward engine (Scryer) is an
**asymmetric pipeline**, and the two engines never call each other directly: they communicate only
through **oxigraph named graphs**, where the graph IRI *is* the world. Nemo writes; Prolog reads; the
quad store is the shared blackboard. The three runtime phases line up one-to-one with the three
[strata](LOGIC-SEMANTICS.md#three-strata-two-already-built).

**Phase 1 — materialize (Nemo owns it; Strata A and B).** The forward-stratified rules — the lowered
modal constraints, the frame-indexed worlds, the type-level no-occurrence rules — run to fixpoint and
dump their closure into oxigraph named graphs. Terminating by construction (finite world set;
type-level under the no-occurrence gate). The result is a saturated, read-only **extensional database
(EDB)**.

**Phase 2 — resolve (Scryer owns it; the query / logic-programming layer).** Backward goal resolution
runs over the materialized store as read-only data. A base predicate is resolved by a foreign
predicate `in_world(W, S, P, O)` backed by an oxigraph quad lookup; the Prolog clauses are the
**intensional database (IDB)**, the recursive, unification-driven part Nemo cannot express.
Non-recursive pattern goals route to oxigraph **SPARQL** (the fast path); recursive or
unification-heavy goals go to Scryer. Termination is the Datalog-fragment guarantee plus tabling, with
the budget as backstop.

**Phase 3 — construct (Scryer invokes a transient Nemo chase; Stratum C only).** When a backward goal
reaches a counterfactual predicate — `holds(W_cf, φ)` with `W_cf = counterfactualOf(W_base, A)` —
Scryer calls `construct_world(W_base, A, W_cf)`. That seeds a fresh, **isolated** named graph from the
base world minimally revised to admit `A` (the AGM step, made deterministic by a declared entrenchment
ordering — see [Deterministic revision](LOGIC-SEMANTICS.md#deterministic-revision-taming-the-agm-mutation-explosion)),
runs an **isolated, transient Nemo chase** scoped to that world, and only then resolves `φ` inside it.
The constructed world is per-query and discarded afterward — or memoized (see
[Graph versioning](#graph-versioning-and-staleness)). Isolation preserves paraconsistency: a
counterfactual world is a separate graph; nested counterfactuals are nested transient graphs bounded
by the depth budget. This is the *only* place generative, undecidable work happens — the only place a
chase is spawned on the fly, and the only place the governor returns `incomplete`.

**Provenance and witnesses cross the seam uniformly** — every derived quad, whether Nemo-materialized,
Scryer-resolved, or built during a Stratum-C chase, carries its rule-IRI and source-IRIs, so one proof
trace (and the prose explanation composed from it) spans both engines without a seam in the narrative.

Two honest later optimizations, neither required for correctness: **demand-driven materialization**
(magic-sets over Nemo) replaces full Phase-1 closure when the base is huge and the query narrow; and
**incremental maintenance** keeps the materialized store fresh under base edits without a full
re-chase — the harder win, since Nemo is not incremental, and the reason that capability is one the
custom Rust layer must eventually own.

### The seam data contract

The blackboard is a *typed* data contract, not an ad-hoc dump. Every materialized quad carries its
derivation metadata, and Prolog reads both data and provenance through fixed foreign predicates.

```text
Nemo output (per derived quad written to oxigraph):
  graph:          IRI            # the world the quad belongs to
  quad:           (S, P, O, G)   # the quad itself (G == graph)
  derivation_id:  IRI            # stable id for this derivation step
  rule_iri:       IRI            # the rule that fired
  source_quad_ids: [IRI]         # the antecedent quads consumed
  profile:        IRI            # the semantic/decidability profile in force
  budget_status:  enum           # ok | partial | exhausted

Scryer foreign predicates (read-only over the materialized store):
  in_world(+W, ?S, ?P, ?O)                  # base-predicate lookup, world-indexed
  derived_by(?QuadId, ?Rule, ?Sources)      # provenance leg for explanations
  contradiction_witness(+W, ?WitnessGraph)  # within-world inconsistency, as a statement graph
```

**Materialize-back policy.** Prolog-derived answers are, by default, **not** written back into
oxigraph: Phase 2 is a read-only query layer, and its derivations are *virtual* — explanations cite
them as virtual derivation steps keyed by `derivation_id`, not as stored quads. Two explicit
exceptions: a Stratum-C constructed world *is* materialized (into its own transient named graph, under
the versioning key below), and a query may request memoization of a recursive IDB predicate, which
writes a clearly-marked derived graph carrying the same derivation metadata. In all cases the rule
holds: **no Prolog answer is silently promoted to an asserted base fact**, and an explanation must be
able to cite every step, virtual or materialized.

## Graph versioning and staleness

Because the solver uses materialized stores, transient worlds, and memoized counterfactuals, stale
results are a real risk. Every materialized world graph is therefore **content-keyed**, fitting the
existing generator/drift-gate philosophy (source-hash stamps under `.stamps/generators/`):

A materialized world graph is keyed by `(source_graph_hash, rule_set_hash, profile_id,
solver_version, budget_params)`. A cached counterfactual world is valid **only** for the exact tuple

```text
(base_world_hash, antecedent_hash, rule_set_hash, entrenchment_hash, profile, solver_version)
```

Any change to a component invalidates the cache entry and forces reconstruction. This is the same
content-hash discipline the generator framework already uses for drift detection (`source_hash` in
`generator.py`), applied to the solver's materialized and transient graphs.

## Generated artifacts

The planned generated outputs live alongside the existing `generated/` tree; every output is owned by
a registered generator and included in the normal drift/orphan checks (`src/gmeow_tools/generator.py`:
the `Generator` Protocol `generator.py:49`, `@register` `:97`, the `GENERATED by gmeow` banner and
orphan heuristic `:41,665`, graph-isomorphism drift via `rdf_compare` `:723`, source-hash stamps under
`.stamps/generators/`). A new logic generator mirrors `StatementGenerator` (including
`allows_internal_tags`, `statement_compile.py:151-156`) and is invoked by `make regenerate` / checked
by `make check-generated`. No generated artifact is ever hand-edited.

```text
generated/logic/gmeow.logic.rdf12.ttl     # the canonical RDF 1.2 logic artifact
generated/logic/projection-report.ttl     # loss + preservation ledger across all targets
generated/owl/gmeow-dl.ttl                 # OWL 2 DL projection (truth-preserving fragment)
generated/owl/gmeow-el.ttl                 # OWL 2 EL projection
generated/datalog/gmeow.dl                 # Datalog projection
generated/n3/gmeow.n3                       # N3 rules projection
generated/foundation/gufo.ttl              # gUFO down-projection of UFO⁺
```

The typed-IR pattern already exists in the transpiler: the up-projection compiler builds a typed
intermediate (`LiftMap` / `UpProjection` in `src/gmeow_tools/up_projection.py`) and projects from it.
The logic compiler mirrors that — compile `logic:` (and adapter-phase `owl:*` / `gufo:`) into one
typed IR, then emit the canonical RDF 1.2 artifact and every projection from it. Passing HermiT and
ELK over the OWL artifacts proves only the projected subset, never full `logic:` consistency. The
loss/preservation contract for these artifacts is in [LOGIC-CONFORMANCE.md](LOGIC-CONFORMANCE.md).

## Explanation as projection: logic to prose

A logic this expressive is humane only if it can say *why* in words — and `logic:` can, because GMEOW
already mandates the raw material. Every term has an `rdfs:label`, a `skos:definition`,
`skos:scopeNote`/`skos:example` where the documentation doctrine applies, and the markdown guide
prose those slices ship; the structural lint *requires* a label, a definition, and a defining slice on
every term (`src/gmeow_tools/validate.py`, `structural_lint`). So at every node of a proof the solver
has vetted human text. **Prose is another projection target — the human, explanatory surface — generated
and gated like OWL or Datalog.**

The explanation engine composes annotations *along a real derivation*: *why derived* (a proof trace),
*why disallowed* (a contradiction witness), *what shape is required* (a constraint) — each clause the
annotation of a node the solver actually used.

The crucial property is **faithfulness by construction**: an explanation is a deterministic composition
of vetted annotation text along a proof the solver produced, not a language model guessing. Every term
the prose cites appears in the proof trace or witness graph, so the explanation can be *validated* (no
cited term outside the trace), not merely trusted. A language model may *polish* the prose; the skeleton
— which axioms, rules, and sources, in which order — is provable and checkable (the conformance test for
this is in [LOGIC-CONFORMANCE.md](LOGIC-CONFORMANCE.md)). This makes annotation expressivity load-bearing:
the scopeNote/example backlog (documentation doctrine/reasoner timeout gate) is **fuel for the explanation surface**, not cosmetic debt.
`gmeow describe` already renders term-level prose; this generalizes it to a whole derivation. For the
stable parts — foundation axioms and the constraint catalogue — the rendered rationale is itself a
generated, drift-gated artifact; per-query explanations are produced at reasoning time.

## CLI shape

The CLI is Typer with a mix of top-level commands and nested sub-apps (`src/gmeow_tools/cli.py`,
`src/gmeow_tools/cli_dev.py`). The public `gmeow` surface now carries a thin `gts` shim that shells
out to the external `gts` binary installed with `gmeow-gts`; bundle builds are developer-facing
top-level `gmeow-dev compile-gts` / `gmeow-dev compile-gts-full` commands; evaluation commands
remain grouped under `evals_app`. The logic surface follows the same pattern:

```bash
gmeow logic compile            # compile logic: → IR → canonical artifact + projections
gmeow logic compile --check    # drift-check the generated logic/projection artifacts
gmeow logic explain            # proof traces, derivations, contradiction witnesses (prose)
gmeow logic query              # backward goal resolution (Scryer)
```

Reasoning extends the existing `reason` command (`cli.py:407`, already taking `--reasoner ELK|hermit`)
with a `--mode` selecting the engine and fragment:

```bash
gmeow reason --mode native     # the native logic: solver (authority)
gmeow reason --mode owl-dl     # HermiT over the OWL DL projection
gmeow reason --mode owl-el     # ELK over the OWL EL projection
gmeow reason --mode datalog    # the Datalog projection
```

OWL projection is consistent with the existing `gmeow project` surface (`cli.py:897`). The Makefile
aliases keep their meaning, with the native lane promoted to authority: `make reason` (fast ELK
projected-subset, `Makefile:45`), `make verify` (native EL/DL reasoned-graph negatives, Java/Docker-free), `make crosscheck`
(rdflib/pyoxigraph agreement, no Docker, `:42`), `make check` (fast gate, native EL/DL reason + verify),
`make classic-cross-check` (ROBOT/ELK/HermiT/Jena oracle, non-required). The fast gate already excludes Docker reasoners and HermiT
is already out of the default gate; the native solver becomes `make check`'s reasoning authority, and
the legacy reasoners remain secondary validators of their projected fragments.

---

*Engines and tools named here — oxigraph, Nemo, Scryer, Soufflé, ELK/HermiT, ProbLog, EYE/cwm,
PyO3/wasm — are listed in [LOGIC-REFERENCES.md](LOGIC-REFERENCES.md).*
