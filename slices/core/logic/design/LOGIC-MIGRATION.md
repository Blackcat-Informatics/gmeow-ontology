<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# GMEOW Logic — Migration, MVP Ladder, and Risk Register

> Status: rollout specification for `logic:`. This is the **migration** member of the
> [GMEOW Logic document set](LOGIC.md#the-document-set). The architecture is maximal by design; this
> document is the strict ladder that keeps the project from trying to build a universal reasoner all
> at once, plus the adapter phases, the gates, the deprecation order, and the design-level risk
> register. Semantics are in [LOGIC-SEMANTICS.md](LOGIC-SEMANTICS.md); the engine is in
> [LOGIC-RUNTIME.md](LOGIC-RUNTIME.md).

## The MVP ladder

Each phase ships a usable artifact and is gated by the [conformance corpus](LOGIC-CONFORMANCE.md)
before the next begins. The "excludes on purpose" column is load-bearing: it is what keeps each rung
small and prevents scope creep into the maximal end-state prematurely.

| Phase | Deliverable | Excludes on purpose |
|---|---|---|
| **v0: IR + projection ledger** | Parse `logic:` into a typed IR; emit OWL/SHACL/Datalog/N3/gUFO projections + the projection report | Native solving |
| **v1: monotonic core** | RDF 1.2 terms, named graphs/worlds, Horn/Datalog materialization, loss ledger, proof traces | Negation-as-failure, counterfactual construction |
| **v2: profile certifier** | EL/DL/Datalog/TGD profile-membership checks; the `unknown`/`incomplete` budget marker | Full Prolog-grade search |
| **v3: modal/foundation lowering** | Rigidity and identity-supply lints lowered into executable rules | General nested modal logic |
| **v4: backward goals** | Embedded Scryer goal resolution over the materialized EDB | Cut in canonical semantics |
| **v5: Stratum-C counterfactuals** | Deterministic revision + transient world construction under budget | Multi-world Lewis ties by default |
| **v6: probabilistic/weighted layer** | Explicit `logic:probability`/`logic:weight` semantics with tests | Treating `logic:confidence` as probability |

Two invariants hold across every rung: the Python oracle implements the rung first as the executable
spec, and the Rust core must pass the identical corpus (Principle 7). No rung is "done" until both
agree.

## Adapter phases

`logic:` does not require rewriting the existing model on day one. Two adapter phases run in parallel,
each letting legacy source coexist with canonical `logic:` source while the source-of-truth boundary
moves:

- **The `owl:*` adapter.** Existing `owl:*` axioms are normalized into the same typed IR as `logic:`
  axioms, so the OWL projection of `logic:` and the hand-authored OWL agree by round-trip isomorphism
  (the gate already proven for the statement layer, `statement_compile.py:123-130,194-210`).
- **The `gufo:` adapter.** Existing `gufo:` stereotype authoring is accepted exactly parallel to the
  `owl:*` adapter: a slice may declare a `logic:` foundational role, and the gUFO projection generator
  emits the `gufo:` stereotypes that `reasoning_lint.py` validates. Nothing in the current model
  breaks.

The adapters are temporary by design — they exist so the migration is incremental, not so the legacy
forms remain canonical. A construct authored in both `logic:` and a legacy form must normalize
identically or the build fails.

## Foundation projection and discipline

UFO⁺ is authored canonically in `logic:`; the upper ontologies are generated, and they are **not all
the same kind of projection**:

- **gUFO** is the **primary generated down-projection** of UFO⁺ — the OWL realization of the same UFO
  lineage, truth-preserving for the fragment OWL can express, validated by running
  `src/gmeow_tools/reasoning_lint.py` (and the `imports/gufo.ttl`-shaped checks) as conformance tests
  over the downcast: it must satisfy every OntoUML anti-pattern check (`exactly_one_stereotype`,
  `identity_overlap`/MixIden, `anti_rigidity_discipline`/MixRig·FreeRole, `relator_mediation`/RelComp).
- **BFO, DOLCE, and SUMO** are generated **alignment/bridge views, not truth-preserving projections**,
  unless a specific subfragment is certified as such in the loss ledger. They carry different
  ontological commitments; the maximal-source doctrine respects that rather than claiming a shared
  foundation. A bridge view is labelled in the [preservation ledger](LOGIC-CONFORMANCE.md) so no
  consumer mistakes it for a sound projection.

The discipline that today guards meta-grounding is preserved across the lint-to-axiom move: the lowered
modal (rigidity) and second-order (identity-supply) axioms must reproduce, over the downcast, exactly
the verdicts the lints produce today — the lints become the regression test and the specification of
the lowering (see [operational semantics](LOGIC-SEMANTICS.md#operational-semantics-modality-and-identity-supply)).

## Gates

The migration succeeds only if every compatibility surface is tested against the same source of truth,
reusing the patterns the project already runs. The required tests, mapped to existing patterns:

- **IR normalization** for existing OWL and gUFO source (adapter phase);
- **equivalence** for matching `logic:`, `owl:*`, and `gufo:` source forms — the round-trip
  isomorphism gate (`statement_compile.py:123-130,194-210`);
- **two-engine agreement** per projected fragment — the `crosscheck` pattern (native vs
  ELK/HermiT/Datalog must agree where the projection preserves meaning);
- **oracle/engine agreement** — the Python oracle and the Rust core must pass one shared,
  language-neutral conformance corpus identically (the conformance gate; Principle 7);
- **DL/EL projection** tests for preserved and intentionally narrowed axioms;
- **loss-ledger** tests for every omitted or weakened construct, including the **preservation
  polarity** of each projection (see [LOGIC-CONFORMANCE.md](LOGIC-CONFORMANCE.md));
- **native rule** tests for derived triples (monotonic and non-monotonic, per declared profile);
- **native constraint** tests for closed-world failures;
- **logic-programming** tests for backward goal resolution and unification;
- **explanation faithfulness** tests: every IRI cited in a generated explanation appears in the proof
  trace or witness graph (grounded, never hallucinated);
- **fragment certification** tests: a profile declared decidable is statically certified, a violation
  is flagged, and budget exhaustion returns `unknown`/`incomplete`, never a false answer;
- **modal/counterfactual conformance** against the slice fixtures (deception held/projected, the Crimea
  contest, the trust-collapse cascade under the no-occurrence gate, a narrative canon, and a constructed
  counterfactual that does not leak into its base world);
- **foundation conformance**: the gUFO downcast passes `reasoning_lint.py` unchanged and every existing
  `gufo:`-dependent slice stays valid.

Existing transpiler and statement suites are the model (`tests/test_statements.py`,
`tests/test_up_projection.py`). The full corpus schema is in [LOGIC-CONFORMANCE.md](LOGIC-CONFORMANCE.md).

## Deprecations

Nothing is removed before its replacement is gated. The deprecation order:

1. **HermiT/ELK move from authority to secondary validators.** They already sit outside the fast gate
   (`make check` excludes Docker reasoners; HermiT runs in its own job, `make check-docker`). Once the
   native solver passes the corpus, `make reason --mode native` becomes the authority and the OWL
   reasoners validate only their projected fragments.
2. **The Python solver becomes the oracle, not the engine.** It is retained permanently as the
   executable spec, but the Rust core carries production workloads.
3. **`owl:*` and `gufo:` adapter source is retired per slice**, only after that slice's `logic:` form
   passes the equivalence gate. Retirement is per-slice and reversible until the gate is green.

No deprecation is silent: each is recorded, and any capability dropped from a projection appears in the
loss ledger with its preservation polarity.

## Design risk register

This architecture is ambitious enough to warrant a standing, design-level risk register. It already
argues that undecidability is *accepted and managed*; the same honesty applies to the other semantic
hazards. Each row names the hazard, where it bites, and the mitigation that is already in the design.

| Risk | Where it appears | Mitigation |
|---|---|---|
| Cut changes declarative answers | Prolog profile | cut is procedural-only, confined to `ProceduralPrologProfile`; loss recorded on projection ([semantics](LOGIC-SEMANTICS.md#cut-is-procedural-not-canonical)) |
| Confidence mistaken for probability | weighted/ProbLog-style inference | four separate predicates; probability only in `ProbabilisticProfile` ([semantics](LOGIC-SEMANTICS.md#confidence-probability-weight-and-evidence)) |
| Named graph treated as modal semantics | worlds | world-indexed entailment relation; no implicit dataset-union ([semantics](LOGIC-SEMANTICS.md#inconsistency-across-worlds-and-world-indexed-entailment)) |
| Anti-rigidity needs a missing witness world | foundation | witness-obligation policy by default; Stratum C may construct witnesses ([semantics](LOGIC-SEMANTICS.md#anti-rigidity-needs-a-witness-policy)) |
| Triple term treated as both quote and assertion | metalogic | a triple term names a proposition; assertion is via explicit predicates ([semantics](LOGIC-SEMANTICS.md#triple-terms-reifiers-and-assertion)) |
| Projection overclaims preservation | loss ledger | preservation polarity per projection ([conformance](LOGIC-CONFORMANCE.md)) |
| Non-stratified negation loops / multiple models | rules | mandatory semantic-profile declaration; soundness stated relative to it |
| Counterfactual revision ties explode | Stratum C | declared entrenchment ordering; genuine tie → `unknown`, never branch |
| Stale materialization or counterfactual cache | Nemo/oxigraph store | content-hash-keyed graph snapshots ([runtime](LOGIC-RUNTIME.md#graph-versioning-and-staleness)) |
| Undecidability / non-termination | canonical layer | decidability is a projection/profile property; budget → `unknown`/`incomplete` |
| BFO/DOLCE overclaimed as truth-preserving | foundation projection | bridge views labelled as such; not truth-preserving unless certified per fragment |
