<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — Performance Doctrine

> The **performance** member of the
> [GMEOW Logic document set](LOGIC.md#the-document-set). It defines how the native physical
> engine of [LOGIC-RUNTIME.md](LOGIC-RUNTIME.md) is made fast — the deterministic-performance
> contract, the data-shape and join doctrines, demand transformation, the incremental algebra,
> the chase-termination ladder, deterministic parallelism, provenance cost bounds, the
> grounding-layer computability seam, the Rust mechanical-sympathy standard, and the measurement
> regime. The semantics being accelerated are fixed by
> [LOGIC-SEMANTICS.md](LOGIC-SEMANTICS.md); nothing in this document may weaken them. The
> Rust-level engineering rules it specializes are in
> [`docs/RUST-OPTIMIZATION.md`](../../../../docs/RUST-OPTIMIZATION.md).

## The thesis

The native physical engine subsumed its bootstrap substrates by matching them
fragment-by-fragment under oracle gates. Performance doctrine states the second half of that
obligation: the native engine must not merely match the engines it subsumes — it must
**decisively exceed them**, on external ground truth, without surrendering a single semantic or
determinism guarantee. Speed is not a quality-of-implementation nicety here; the reasoning
runtime is the pipeline's single largest computation, the conjecture and counterfactual surfaces
re-enter it in loops, and every consumer of `gmeow.gts` inherits its latency. A slow canonical
reasoner would be a standing argument for bypassing the canon; a fast one is the strongest
argument for maximal ontological use.

The published record of the subsumed substrates fixes the beat-line precisely. The
state-of-the-art forward-engine family is built on columnar trie storage,
leapfrog triejoin, semi-naive evaluation, and the restricted chase — and is, by its own account,
single-threaded, non-incremental, and batch-only. The backward substrate (embedded Prolog)
carries process-global mutable symbol state that forces whole-lifecycle serialization and
per-query instance construction. The doctrine below adopts what those systems got right, and
targets exactly what they left on the table: deterministic parallelism, incremental maintenance,
demand-bounded evaluation, and a zero-allocation interned data plane.

## Deterministic-performance doctrine

Determinism is not a tax on performance; it is a design input that selects *which* fast designs
are admissible.

- Determinism comes from **sorted emission at commit points** and **dense first-seen interned
  identifiers** — never from hasher choice, container iteration order, or scheduling accident.
  Hash-based structures are permitted anywhere on lookup paths precisely because no artifact,
  diagnostic, golden, or budget observation may depend on their order.
- The per-round sorted commit of the semi-naive loop is the **only reproducible counting point**:
  budgets are charged per committed derivation in committed order, and every optimization must
  preserve that observation point exactly. An optimization that makes budget charging
  order-dependent is wrong even if its fact set is identical.
- A performance-only change produces **byte-identical output**. A change that alters bytes is by
  definition a semantic change: it re-mints the engine's contract identity, invalidates
  downstream caches, and must update goldens and parity expectations explicitly (see
  [LOGIC-RUNTIME.md § Graph versioning and staleness](LOGIC-RUNTIME.md)).
- Derivation identity remains content-addressed and interner-independent: numeric runtime IDs
  never enter a derivation hash. Every representation change below preserves that seam.

## Data-shape doctrine

The engine's data plane is **dense, interned, columnar, and sorted**; strings exist only at the
ingestion, diagnostic, and serialization edges.

- **Every entity class the engine touches carries a typed dense ID**: terms, predicates, rules,
  rows, strata, worlds. Predicate names, rule IRIs, and other string-shaped keys are interned
  once at plan construction; the hot loops compare and hash machine words only. Niche-optimized
  non-zero representations keep optional IDs pointer-free, and phantom-typed newtypes make
  cross-class ID confusion unrepresentable.
- **Interning keys on structural value at the edge**, and structural identity — not display
  surface — is the dedup key. Rendering a term to text is a serialization concern; the interner
  is not a stringifier.
- **Facts live in flat columns, not per-fact heap allocations.** Argument tuples are offsets
  into shared arenas whose lifetimes follow the fixpoint's phase structure: allocate within a
  round, sort-commit, reset. Rows store IDs, never cloned term values; the postings that index
  them store dense row indexes.
- **Relations are maintained as sorted immutable batches with amortized merging** (the
  shared-arrangement discipline): a mutable tail absorbs the current round, and consolidation
  merges batches geometrically. Sorted batches make merge joins, galloping search, delta
  extraction, and byte-stable emission all fall out of one representation — and they are the
  common substrate the join and incrementality doctrines below both require.
- **Membership tests on the hot path are dense**: delta membership over row indexes is a bounded
  index test, not a hash probe of a composite key. Because row ids are minted densely in
  FactKey-sorted commit order, a semi-naive round's committed rows are always a contiguous row-id
  span, so the native engine specializes dense delta membership to a single `[lo, hi)` range
  compare — the round batch IS the delta, with no per-round bitset or arena round-trip. (The
  reference reduct engine retains the equivalent dense bitset; the range is its byte-identical
  contiguous-span specialization.)

## Join doctrine

Join execution is **hybrid by measurement, never ideological**.

- The default plan is a **binary indexed plan** with cardinality-aware sideways information
  passing: body atoms are ordered by estimated selectivity, and each step probes sorted batches
  by galloping cursor or postings intersection. For the acyclic joins that dominate ontology
  rule bodies, well-ordered binary plans are optimal and anything fancier is overhead.
- **Worst-case-optimal join execution is reserved for certified-cyclic sub-plans** — triangle-
  and clique-shaped body patterns whose intermediate results can explode under any binary
  order. The engine detects cyclicity at plan time and lowers only those sub-plans to a
  multiway sort-based leapfrog-triejoin over the sorted batches; the lazy-trie ("free join")
  refinement, which degenerates gracefully back to binary behavior, is the sanctioned evolution
  of that operator. WCOJ everywhere is an anti-goal: the doctrine is *binary until proven
  cyclic*.
- Join operators are **kernels over the columnar substrate**, monomorphized per scan mode,
  adornment, and arity, so the inner loops carry no per-tuple interpretive branching.

## Demand doctrine

Goal-directed evaluation is **demand transformation over the single bottom-up core** — one
engine, two directions.

- The forward/backward split is dissolved algebraically, not by building a second engine:
  backward goals are answered by rewriting the program under the goal's adornment and running
  the same stratified semi-naive fixpoint over the demand-restricted program. Tabling and magic
  sets are the same fixpoint computed from different ends; the engine keeps the bottom-up end.
- The sanctioned rewrite is **subsumptive demand transformation**: a demand annotated with a
  more general binding pattern answers every more specific request, so repeated sub-goals reuse
  answers instead of re-deriving them. Subsumptive demand strictly dominates classic
  variant-based magic sets and is the reason a native SLG resolution engine — suspended goals,
  answer tables, delay lists — is **explicitly rejected**: it would duplicate the fixpoint core
  to recover behavior the rewrite already provides.
- Procedural control (cut and its relatives) is retired. Programs are rewritten
  declaratively at authoring time; the native engine rejects cut and never simulates
  control flow inside a fixpoint.
- Budgets transfer unchanged: the demand-transformed program is charged through the same
  committed-derivation counting point, so a backward query's budget semantics are identical to a
  forward materialization's.
- Query-scoped external relations are **bound-producing physical EDB operators**, not a scratch-
  graph materialization lane. The current SIPS binding, ordered-prefix limit, and total order are
  pushed into the provider at the relation atom's authored position; only returned rows that survive
  the ordinary joins enter the fixpoint. A complete validated batch may be reused within that query,
  but no provider cache or registration escapes into ambient process state. This retains one planner
  and one recursive semantics while avoiding the I/O, allocation, and later filtering of copying a
  candidate universe into RDF first.

## Incrementality doctrine

Incremental maintenance is the largest single lever the runtime owns, because its consumers are
loops: conjecture testing, counterfactual construction, and re-reasoning after slice edits all
re-enter the engine with small deltas over large stable worlds.

- The target algebra is the **Z-set / DBSP circuit calculus**: facts carry integer weights,
  rules become weighted relational operators, and the incremental version of the whole recursive
  program is derived mechanically rather than hand-maintained. Weights form a ring — retraction
  is an additive inverse, not a special-cased deletion pass. Delete-heavy workloads may fall
  back to backward/forward-style re-derivation checks where weight bookkeeping would dominate.
- Incrementality **rides the sorted-batch substrate**: a delta is just another batch with signed
  weights, consolidated by the same merge machinery. This is why the data-shape doctrine
  precedes it.
- The **contract identity remains the invalidation key**: an incremental result is only valid
  under an unchanged contract hash, rule-set hash, and solver version. Incrementality
  accelerates re-evaluation under a fixed contract; it never blurs cache identity.
- Honesty about the frontier: incremental maintenance of the existential chase and incremental
  **solving** for well-founded/stable-model semantics are open research areas, not engineering
  backlog. Grounding for the non-monotonic fragments is incremental; the solving step remains
  flagged non-incremental until the theory exists. A flagged non-incremental fragment is a ledger
  entry, never a silent slow path.

The native implementation covers the fixed-contract, finite positive binary-Datalog fragment.
It stores the recursive inner-iteration history, differentiates every rule join by the same
telescoping product law, and settles signed insertions and retractions to the new least fixed
point. Ground IRI-or-literal conjecture candidates use an insert-only governed transaction;
literal objects remain typed opaque facts for the fixed class/property rule program and still
feed the literal-aware DL post-pass. Unbounded positive counterfactual branches fork a cached
base session and apply their functional-slot
`-1/+1` revision. Rule-program-changing conjectures, bounded retractions, and counterfactual
programs carrying NAF, builtins, or rule facts stay on named native fallbacks and appear in the
performance ledger. This boundary is an identity boundary too: the session pins the contract
hash, rendered rule-set hash, and native incremental-solver version.

The non-monotone sibling (`physical/incremental_grounding.rs`) keeps the ground-and-solve
boundary literal. It removes NAF literals to maintain the finite positive candidate universe
with the same recursive signed session, then differentiates every source rule's grounding join
with `new[..p] × delta[p] × old[p+1..]`. Fully-ground rule instances carry checked integer support
weights; only a `0 ↔ positive` crossing changes the active program. The exact solver slice is the
asserted EDB plus those active rules, so asserting a fact already present in the candidate
universe still invalidates the solver. Every head, NAF, and inequality variable must be bound by
a positive body atom; builtins, positive-body-free rules, and blank-node constants hard-fail at
this seam rather than silently falling back.

`IncrementalWellFoundedSession` and `IncrementalStableModelSession` cache the last answer only
when that complete slice is unchanged. A changed slice reruns the existing alternating-fixpoint
or cautious stable-model evaluator from scratch and emits a per-shot
`FlaggedNonIncremental` run row naming that fact. This follows the ground-and-solve separation in
[Calimeri et al., *ASP-Based Multi-Shot Reasoning via DLV2 with Incremental Grounding*](https://doi.org/10.1017/S1471068425000067):
grounding state persists across shots while the solver remains non-incremental. It does **not**
claim incremental WFS or answer-set solving.

Loop forks share the immutable rule plan, fact arena, EDB, and cached inner-iteration histories
by reference; a branch installs new roots only when its signed transaction commits. The committed
`relational-core-mini/incremental-transitive-closure` cost lane prepares that base outside the
measurement boundary, then proves insert and retract closure fingerprints against clean native
rebuilds. Its deterministic observation is 13 charged incremental derivations versus 91 for the
clean rebuild, with `peak_live_bytes` 130246 versus 231822; these raw counts are projected into
`generated/bench/cost-ledger.md`. Wall-clock is not part of the claim.

The committed `relational-core-mini/incremental-wfs-grounding` lane makes the second boundary
equally falsifiable. It measures a signed ground-program insertion against a clean ground+WFS
rebuild, checks the full result fingerprint, retracts the batch back to the base fingerprint,
and records ground-rule commits, candidate probes, signed join rows, and exact peak-live bytes.
The deterministic observation is 1 active ground-rule commit versus 13 on rebuild, 28 candidate
probes versus 195, and `peak_live_bytes` 238187 versus 239714. The ledger also records
`solver_status = flagged-non-incremental` and `solver_reran = true`; the measured win is grounding
work only. Wall-clock remains advisory.

## Chase doctrine

Existential-rule execution keeps the restricted chase and grows by **certifying broader
termination classes**, never by weakening the refusal discipline.

- Termination certificates form a **ladder of strictly increasing power**: weak acyclicity ⊊
  joint acyclicity ⊊ super-weak acyclicity ⊊ model-summarizing acyclicity ⊊ model-faithful
  acyclicity, with the restricted-chase-specific refinements beyond that. The certifier reports
  the strongest class it can establish; anything uncertified refuses or runs under budget,
  exactly as today.
- The polynomial classes (joint and super-weak acyclicity) are checked structurally.
  **Model-summarizing acyclicity is decided by the engine itself**: the check is Datalog
  entailment over the critical instance, so the certifier is a self-hosted reasoning program —
  the engine dogfooding its own fixpoint as its own termination analysis.
- Non-termination verdicts are treated more cautiously than termination verdicts: the published
  proof landscape for restricted-chase non-termination criteria has had repairs, so a
  "non-terminating" classification demotes to budgeted execution rather than hard refusal.
- Witness distinctness for counting existentials is carried **structurally** through the
  lowering — pairwise-inequality guards travel with the rule object, never through a text
  surface that cannot express them.

## Parallelism doctrine

The engine parallelizes **only where determinism is free**, and the beat-line makes this the
cheapest differentiator: the subsumed forward substrate is single-threaded.

- **World-level parallelism** stays: worlds are independent EDBs; results fold in sorted world
  order and reproduce the sequential bytes.
- **Within a world, parallelism follows the round structure**: within one semi-naive round, rule
  firings over disjoint delta partitions evaluate concurrently into thread-local buffers, and
  the round's single sorted commit merges them; since commit order is sorted rather than
  arrival-ordered, the merge erases scheduling nondeterminism by construction. Keyed sharding of
  the weighted-batch operators extends the same argument to the incremental circuits.
- **Budgeted execution parallelizes at round granularity**: a budget is charged against the
  round's committed, sorted derivation sequence, so the observation "budget exhausted after N
  committed derivations" is identical however many threads produced the round's candidates. A
  budget that would need intra-round arrival order is a design error.
- Parallelism never changes emitted bytes, ledger contents, or budget observations; a parallel
  path that cannot prove that equivalence stays sequential.

## Provenance doctrine

Provenance is bounded-cost and in-band, never an afterthought pass.

- **Record mode** carries a **minimal-proof-height annotation** per derived fact through the
  semi-naive loop — a single small integer per fact, maintained over the bounded
  `(N ∪ {∞}, min, max)` semiring with a checked one-level rule lift, so a firing is annotated
  `1 + max(body heights)` and alternatives select `min`. The selected annotation is carried on
  the native row/oracle provenance seam; asserted leaves carry height zero. A reusable lazy
  explanation index stores only row identity up front and reconstructs a proof by descending the
  selected antecedents of the queried fact. An unrelated malformed proof component is not
  traversed; querying that component still hard-fails. This is the bounded annotation/backward-
  search design evaluated by [Zhao, Subotić, and Scholz](https://doi.org/10.1145/3379446), not an
  eager proof-tree forest.
- Full symbolic how-provenance (polynomial lineage) over recursive programs is **banned as a
  materialization target**: it has no polynomial-size representation in general. Where algebraic
  provenance is required, absorptive semirings — which do admit compact circuits — are the
  sanctioned form, and the integer-weighted Z-sets of the incremental layer double as counting
  provenance for free. Both carriers implement the same checked provenance-semiring plug point;
  the Z carrier additionally exposes additive inverse, and the production incremental circuit
  routes weight sum, product, and retraction through it. Overflow is a hard diagnostic, never
  saturation or wrapping.
- **Caller-supplied score annotations use that same physical seam, not a post-hoc answer join.**
  `dispatch_query_annotated` and `materialize_program_annotated` accept an opaque
  `TupleAnnotationAlgebra`: `multiply` (`⊗`) combines the positive premises of one rule firing and
  `add` (`⊕`) combines alternative firings for the same tuple. The native planned join retains each
  direct contribution beside the combined answer, so a BM25/vector/name-similarity score arrives
  with its one-hop proof lineage; it does not lose the derivation that produced it. Magic demand
  predicates are unit-valued control tuples and are excluded from score products, preventing the
  demand transform from counting a scored prefix twice. This bounded direct-edge carrier is not an
  eagerly expanded provenance polynomial.
- **Provider annotations enter through the same algebra, with their meanings still explicit.** A
  provider descriptor must name the selected algebra identity and the dimension of its opaque
  element. Similarity, rank, distance, persistence, and epistemic confidence remain distinct even
  though all compose through the same checked `⊗`/`⊕` interface. Provider tuple sources remain in
  derivation lineage, and the query receipt records provider, artifact generation, model, request,
  response hash, contribution, and preservation. This is in-band provenance, not a later score join.
- **Algebraic deviations are admitted only by an explicit, structurally checked contract.** An
  exact contract warrants the ordinary semiring laws. A non-semiring algebra must name each
  violated law and the positive query classes for which the caller warrants a complete
  over-approximation; the engine classifies the actual IDB graph as acyclic or recursive and
  refuses an out-of-scope declaration. Annotation recursion must reach a deterministic fixed point
  within the declared round guard or hard-fail. Negation-as-failure and value-inventing existential
  heads currently have no declared score algebra and are typed refusals, never silent score erasure.
- The Record/Skip capability boundary is unchanged: Skip mode commits the identical fact set in
  the identical order under the identical budget, minus the annotations. Provenance remains a
  capability, not a correctness fork.

External-relation optimization claims use **structural evidence**: distinct provider calls, cache
hits, bound calls, delivered rows, and unique admitted rows. A fixture may prove that a bound call
delivers fewer rows than its equivalent all-tuples scratch representation; that is a deterministic
materialization reduction, not a fabricated wall-clock speedup. Timing claims still require the
benchmark/perf-ledger discipline below.

## Grounding-layer computability doctrine

The grounding slices (`math:`, `lang:`) are engine workloads, and their designs already fix the
seams; performance work honors them rather than inventing parallel machinery.

- **One shared content-addressed structured-term arena serves `logic:`, `lang:`, and `math:`.**
  Expression ASTs, lifted source forms, and proof objects are hash-consed term DAGs over the
  engine's arena substrate: alpha-normalized content keys give identical subexpressions one node
  and O(1) structural equality; the binder declaration/occurrence split makes α-equivalence a
  graph isomorphism and substitution capture-avoiding by construction. There is exactly one term
  representation — a lifted script that lowers into `logic:` terms is *already* in engine
  format, with nothing to convert at the reasoning boundary.
- **Heavy computation crosses the solver seam** as budgeted, provenance-bearing observations —
  matrix decompositions, samplers, proof checkers, and symbolic evaluators hand off across the
  solver profile and re-enter as vantage-held results. The classification core never inlines
  open-ended numeric or symbolic computation.
- **Reasoning-adjacent mathematics is a native moded-builtin family**, held to the same
  discipline as the integer builtins: exact rational arithmetic over numerator/denominator
  pairs, dimension-vector equality and homogeneity over the seven-dimensional SI basis, and
  bilinear-form norms and distances for prototype-style geometry. These are mode-constrained
  relations in the sideways-information-passing order, deterministic, budget-governed, and
  oracle-anchored — and their dense rational and vector columns are a sanctioned SIMD surface.
- **Normalization and simplification target the e-graph direction**: equality saturation over
  the same relational fixpoint engine (the Datalog/e-graph unification substrate) is the
  sanctioned route to normal forms and algebraic rewriting, keeping "keep it computable" inside
  the one engine rather than bolting on a symbolic system.
- **Downprojection is always a correspondence lens** with a genuine round-trip witness (see
  [LOGIC-CORRESPONDENCE.md](LOGIC-CORRESPONDENCE.md)): a lifted artifact that cannot be put back
  through its lens with a section discharge is not "supported", and no performance shortcut may
  fabricate that witness.

## Rust mechanical-sympathy doctrine

The engine core is held to the same advanced-feature standard the carrier already meets; "plain
std Rust" is not a neutral default on a hot path, it is a measured cost.

- **Typed niche IDs** for every entity class (non-zero, phantom-branded); options over them are
  pointer-free.
- **Borrowed-key raw-entry probes** for every dedup and lookup structure — no owned-key clone
  per probe, ever. Fast fixed-seed hashers are permitted on lookup-only structures because
  determinism never derives from them; hash values are never persisted.
- **Phase-scoped arena allocation** matching the fixpoint's round/stratum structure:
  allocate-commit-reset, no per-tuple `malloc` traffic.
- **Const-generic monomorphized kernels**: scan mode, adornment, and arity are compile-time
  parameters of the join and filter kernels, not runtime enum branches inside inner loops —
  compile-don't-interpret expressed as a language feature. The runtime-rule interpreter
  dispatches once per operator into these kernels (enum dispatch, not trait objects); the
  published evidence that well-built plan interpreters sit within a small constant factor of
  synthesized code is why machine-code generation stays in reserve rather than on the roadmap.
- **Dense bitsets** for round-local membership; **lending-iterator galloping cursors** over
  sorted batches instead of materialized per-stage solution vectors — the carrier's
  zero-allocation borrowed-iterator doctrine extended into the engine.
- **Type-state plan pipeline**: parsed → stratified → planned → executable as distinct types, so
  an unstratified or unplanned program is unrepresentable at the executor boundary.
- **Content-addressed owned plans**: the terminal executable owns its immutable rule IR behind an
  `Arc` and is cached under `(contract_hash, canonical_rule_hash, solver_version)` in a bounded
  LRU. The rule digest length-prefixes every execution-relevant term, atom, guard, builtin, and
  authored ordering; non-stratifiable results are negative-cached as declared gaps. Cache locking
  ends before evaluation, and eviction drops only the cache reference, never an in-flight plan.
  Both native forward materialization and demand-transformed backward evaluation consume this one
  executable type.
- **Flat binding frames**: every rule assigns variables stable authored-first-occurrence slots and
  lowers acyclic positive atoms into a deterministic binding-aware SIPS order, recording the
  guaranteed `Any`/subject/object/both index shape at each step, then selects one of four
  variable/constant operator shapes. Provenance sources are restored to authored body order after
  physical execution. Binary and certified-
  cyclic kernels read those slots directly and copy the interner's cached display surface only when
  first binding a slot; they do not interpret `EvalTerm` or search `(name, value)` pairs per tuple.
  The shared named solution form is reconstructed once after the positive join for builtin, NAF,
  head, and provenance semantics.
- **SIMD only on dense ID or rational/vector columns with a measured win**, per the standing
  optimization rules; layout for autovectorization first, explicit portable SIMD second.
- All of it under the standing order of operations: **data shape, ownership shape, dispatch
  shape first**; compiler-flag churn last, and never at the cost of the build-memory or
  debug-assertion constraints.

## Measurement doctrine

Performance claims are made against external ground truth or not at all, and a performance
*gate* is a **deterministic count**, never a wall-clock timing. Wall-clock on a shared,
contended host is not measurement — a zero-change re-run of a low-sample group swings tens of
percentage points from scheduling noise alone, so a timing threshold gate is a nicer lie, not a
truth. The gate is grounded instead in quantities that are pure functions of `(engine version,
corpus)`: bit-for-bit reproducible, immune to the scheduler.

- **Three measurement tiers, each with a fixed gate status.**
  - *Correctness (golden)* — a native verdict compared against the **published golden verdicts**
    of an external corpus is a deterministic set comparison and gates **on-gate**, with no oracle
    process required.
  - *Cost* — the engine's own operational counters are the gating cost signal: the per-round
    committed-derivation count, **peak simultaneously-live bytes**, and the **total allocation
    bytes / allocation count**. The count and peak-live are byte-reproducible and gate **on-gate**
    by exact drift-match; externally-instrumented retired-instruction counts corroborate them in a
    maintainer lane. The two total-allocation scalars are NOT byte-reproducible — the native core
    emits a small quantized run-to-run allocation transient (allocation counts move in a
    14-allocation quantum; after flat-slot kernels lowered small-query totals, a 12-process
    `ancestor-query` soak observed a maximum 42-allocation span; the transient is empirically
    irreducible: it survives a process-global total, an inline
    single-thread parallel pool, and a fully serial engine, so it is genuine per-run engine jitter,
    not a threading artifact). Rather than leave them advisory, they gate through a **one-sided
    tolerance band**: bytes use `fresh ≤ baseline·1.01`; counts use
    `fresh_count ≤ baseline_count + max(ceil(baseline_count × 0.01), 42)`. It is folded through the SAME divergence ledger
    as the exact signals: a within-band run
    is a non-blocking `Agree`, a breach a blocking `CorpusOnly` cost-regression finding. The band is a
    deterministic verdict (a pure function of `(fresh, baseline, ε)`) that never flakes yet still bites
    the gross allocation regression the doctrine targets — a "fewer clones / fewer owned-key
    allocations" backslide re-adds allocations far above ε; a sub-ε change is below the engine's own
    allocation-noise floor and is not gate-detectable, the honest limit of a global-allocator counter.
  - *Speed* — wall-clock and peak-RSS are **advisory evidence only, never a gate**: engine-vs-engine
    leaderboard rows over a named, version-pinned corpus.
- **Cost is an algebra, not a scalar.** Cost is a tropical / counting semiring over the
  evaluation — `cost(fact) = ⊕ over derivations of (rule-cost ⊗ ⊗ᵢ cost(antecedentᵢ))`, the same
  algebraic shape as the minimal-proof-height annotation of the provenance doctrine above. It is
  carried as a **decomposable cost vector keyed by (rule, predicate, stratum)**, reusing the
  stratification the certifier already computes; the committed-derivation count, allocation
  bytes/count, and peak-live bytes are its scalar projections. A regression therefore attributes
  to a rule family, not merely to a benchmark group — which is exactly what a fragment-by-fragment
  performance-lever program needs to state "this change reduced the cost of *this* fragment".
- **Plan reuse is measured as repeat evaluation, not a cache microbenchmark.** Every committed
  forward mini-corpus case runs two complete materializations over identical EDB and rules with
  parsing, EDB loading, and certification outside both measured regions. The cold run must report
  one plan build and nonzero planning units; the warm run must report a cache hit, the identical
  immutable executable, zero builds, and zero planning units. Closure digest, committed steps, and
  the full `(rule, predicate, stratum)` vector must match exactly, while allocation count and
  peak-live bytes must both strictly fall. These integer/boolean observations are projected into
  `generated/bench/cost-ledger.md`; wall-clock is absent from the claim.
- **Provenance overhead is measured Record-versus-Skip over the same warm plan.** After the
  cold/warm plan-cache evidence, every forward mini-corpus case performs a separate fair pair:
  one complete bounded-Record evaluation and one complete facts-only Skip evaluation. Both
  projections do the same fact-only hash/count post-work; Record alone reads its two annotation
  scalars.
  The fact-only closure hash and committed-step count must match exactly, Record must carry one
  height annotation per closure fact, and Skip carries zero. Per-mode peak-live bytes and the
  resulting Record overhead are exact drift-gated integers; total allocation-count deltas are
  retained as advisory corroboration under the allocator-noise doctrine. The committed table is
  `generated/bench/cost-ledger.md`; it contains no wall-clock percentage.
- **The engine-vs-engine lanes on external corpora remain** — the standard chase benchmark
  scenarios, the subsumed forward engine's own published evaluation sets for the existential and
  materialization fragments, and the transitive-closure/points-to program families the
  Datalog-systems literature compares on for the relational core. Internal goldens prove
  correctness; external corpora prove speed; the deterministic cost vector is what makes "not
  regressed" a byte-checkable proposition rather than a noisy sample.
- **The reporting spine stays, but its gate is deterministic.** The committed baseline, the
  leaderboard, and the drift gate remain; the committed baseline is now the **integer-valued
  deterministic cost vector**, and the drift gate is a **content-addressed cost-regression finding
  in the divergence ledger** — an equal cost descriptor folds through as a non-blocking `Agree`
  corroboration, a divergent one as a **blocking `CorpusOnly` regression finding** carrying
  content-addressed identity, and the committed cost-ledger projection is additionally enforced by
  the strict-sync drift check; a cost regression is thus a ledger refutation / drift failure.
  **The wall-clock duration budget is retired as a gate** — timings fold into the advisory
  leaderboard only. Retired-instruction counts gate only through their own deterministic column;
  estimated cycles and cache figures are microarchitecture-dependent and stay advisory.
- **Numbers are measured, never invented.** A claim of the form "faster than the subsumed engine"
  is admissible only as a leaderboard row produced by a committed lane over a named corpus, pinned
  to versions on both sides.
- **A benchmark group whose sample count sits below the criterion default is a red flag** — either
  raise it or mark the group advisory-only; a low-sample wall-clock number never gates.
- Every deliberately non-incremental, refused, or demoted fragment is a **ledger entry** — the
  perf ledger is the honesty surface that keeps "not yet fast" from silently reading as "fast".
- **Gap-zero is a soak-window claim, not a one-shot tally.** The `bench-soak` gate re-runs the
  deterministic native-vs-published agreement check over the committed mini corpora N times
  (window ≥ 2) and hard-fails unless EVERY run is gap-zero (`corpus_only == 0 && dl_gap == 0`) AND
  its divergence-ledger finding-graph blake3 is byte-identical across the whole window — a drifting
  fingerprint over a fixed corpus is itself a divergence finding. The committed
  `generated/bench/soak.md` record projects the invariant per-corpus finding-graph digest and is
  drift-gated by strict `sync`, so "ledger gap-zero over a soak window" is a checkable claim
  rather than a single sample.

Consistency with the release-as-evidence principle: perf **timings** remain carried as data and
never as a gate — they are advisory here. What gates is a deterministic **count**, a different
kind of observation entirely: reproducible bit-for-bit, independent of the machine and the
scheduler, and therefore admissible as a checkable claim rather than a leaderboard verdict.

## Perf ledger — recorded fragments

Every deliberately non-incremental, refused, or demoted fragment is recorded here as a ledger
entry, each carrying a named forward path. A named forward path is not a deferral — it is what
keeps "not yet fast" or "not yet reachable" honest instead of silently reading as "fast" or
"done".

- **Native full-scale EL/Galen non-completion.** The native n-ary reified chase is *correct* —
  captured-golden parity is proven on small recursive CURIE programs and an EL-shaped multi-arity
  recursion, and native actively derives on the real EL/Galen corpus — but the naive,
  non-incremental restricted chase (`crates/logic/src/physical/chase.rs` re-derives against the
  full store every round) does not reach the ~2,025,426-tuple EL/Galen fixpoint within a
  practical time/memory window. The engine-vs-engine CORRECTNESS is demonstrated; the at-scale
  PERFORMANCE is not yet there. Forward path: incremental / semi-naive maintenance (the
  perf-lever program), orthogonal to the n-ary correctness work.
- **ChaseBench upstream is unlicensed.** dbunibas/chasebench carries no license, so its fetch
  lane (`maint-chasebench-corpus`) HARD-FAILS on the missing license (honest no-optionality).
- **Published-scenario native-fragment gap.** The native fixed-arity positive-existential
  fragment refuses negation-as-failure, arithmetic/comparison builtins, aggregates, and
  Skolem-FUNCTION existentials — so the published ChaseBench deep/doctors/lubm/ontology-256
  scenarios cannot run in the native on-gate fragment.
  Committed on-gate coverage therefore uses license-clean, native-completing scenarios that
  mirror those families' shapes; the large EL-Galen example remains off-gate because its
  ~45k-row EDB is too large for an on-gate committed corpus.
- **Allocation gate is a tolerance band, not exact match.** The native forward core exhibits a
  genuine residual ~0.059% per-run allocation jitter (a single quantized ±14-alloc event deep in
  the forward core; the engine already fixed-seeds its hashers) that neither process-global
  counting nor fully-serial execution eliminates. `alloc_bytes`/`alloc_count` therefore gate
  through one-sided bands (bytes: 1%; counts:
  `fresh_count ≤ baseline_count + max(ceil(baseline_count × 0.01), 42)`);
  `peak_live_bytes` gates by exact drift-match. Forward path: de-randomize the residual
  allocating structure in the forward core to restore exact-match allocation gating.
