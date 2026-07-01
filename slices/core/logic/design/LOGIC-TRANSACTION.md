<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — Transaction Logic

> The state-change member of the GMEOW Logic design set. Transaction Logic is the
> meaning of the **Evolution = transaction-path** facet of the reasoning contract
> ([`LOGIC-CONTRACT.md`](LOGIC-CONTRACT.md)); it is a value of one orthogonal facet, **not** a
> separate profile. Member of the GMEOW Logic design set ([`LOGIC.md`](LOGIC.md)). The lineage is
> the Transaction Logic of Bonner and Kifer (see [`LOGIC-REFERENCES.md`](LOGIC-REFERENCES.md)). The
> correspondence calculus's `get`/`put` legs are transaction-programs in this sense
> ([`LOGIC-CORRESPONDENCE.md`](LOGIC-CORRESPONDENCE.md)).

## Why state change is its own facet

Classical logic evaluates truth at a single state of affairs. Reasoning about *change* — an agent
performing actions, a memory absorbing updates, a plan advancing — needs truth evaluated over a
**sequence** of states. This is an orthogonal concern: a state-change program may run under any model
semantics — least-model, well-founded, or stable-model — with any uncertainty measure (probabilistic,
weighted, fuzzy) carried alongside; over open or closed worlds; with or without world-indexing.
(Probability is an `Uncertainty`-facet *measure*, not a consequence relation — see
[`LOGIC-CONTRACT.md`](LOGIC-CONTRACT.md).) Because the choice is independent, state change is the value
`transaction-path` of the Evolution facet, composed with the others, never a parallel mode that
re-bundles them.

## Path semantics

A query in this facet is evaluated not over one state but over a **path** — an ordered sequence of
states ⟨s₀ … sₙ⟩. A formula's truth is a property of the path it executes along. The base
relations are:

- **elementary state transitions** — the smallest steps that carry one state to the next;
- **serial conjunction (⊗)** — *do φ, then ψ*: the path splits so that φ holds over a prefix and
  ψ over the remaining suffix. Serial conjunction is a program combinator, typed apart from
  ordinary conjunction (which holds *at* a state) — see the formula-versus-program operator typing
  in [`LOGIC-IR.md`](LOGIC-IR.md);
- **guarded choice** — *if guard then φ else ψ*: selects between two sub-programs based on
  whether a situation type (`logic:guardSituation`) holds at the current state. Typed apart from
  a conditional formula connective (which holds at a state) — `logic:Choice` is the structural
  operator, dispatching control rather than evaluating truth;
- **concurrent composition** — *φ ∥ ψ*: two sub-programs advance together with their elementary
  steps interleaved over a shared path. The composition operator (`logic:ConcurrentComposition`)
  is DISTINCT from the three separately-declared correctness concerns (serializability criterion,
  isolation level, and concurrency-control protocol) — see §Concurrent transactions;
- **iteration** — *while cond do body*: repeats a body sub-program while a loop condition holds.
  A loop has a body (`logic:iterationBody`) and a condition (`logic:iterationCondition`), not two
  co-equal operands, so dedicated properties replace the binary left/right pair;
- **fallback** — *try φ else ψ*: attempts the primary sub-program; when the primary's executional
  entailment does not hold, executes the alternate. Typed apart from action-level
  `logic:compensation`, which undoes an action that *ran* and reached an outcome;
- **executional entailment** — a transaction *succeeds* when there exists a path from the start
  state along which the program holds; failure leaves the start state untouched.

Static reasoning is the degenerate one-state path; it remains the default Evolution value, so the
rest of the system is unaffected unless a contract selects `transaction-path`.

## Materialized outcome

Executing a program is not a side effect that vanishes — the verdict and the executed path are
**carried into the graph**. The native evaluator runs every executable program root (a combinator
that declares its start with `logic:transitionFromState`) and records a **`logic:TransactionOutcome`**:
it carries `logic:outcomeOfProgram` (the program run), `logic:transactionStart` (the start state),
and `logic:transactionSucceeds` (the boolean executional-entailment verdict — *a path exists from the
start*). On success the outcome also carries the executed run as a `logic:Path` of states ordered by
`logic:temporallySucceeds` and linked through `logic:executedAlongPath`. Each elementary step along the
path is itself materialized as a first-class **`logic:TransactionStep`** — the schema it runs
(`logic:instantiatesSchema`) and the path edge it walks (`logic:transitionFromState` →
`logic:transitionToState`) — and that step is the node the supersession quartet attributes its retirals
to (`logic:retiredByTransaction` / `logic:supersededBy`, §Elementary updates). The step is minted per
run position, so every pass of a `logic:Iteration` over the same primitive is a **distinct** step and the
audit trail never collapses two passes onto one node. Its provenance is grounded on that step's own
`logic:effect`. On failure only the outcome node is recorded — the start state is untouched, realizing
"failure leaves the start state untouched".

The outcome is typed apart from neighbouring verdicts it must never be confused with: it is **not** a
`logic:GoalEvaluation` (at-a-state satisfaction of a goal) and **not** a `logic:PlanSuccessMode` (a
plan's success classification over a nondeterministic outcome set) — it records one concrete run's
success or failure under executional entailment.

## Elementary updates are supersession, never erasure

The state-changing primitives are `ins` (assert) and `del` (retire). Three dimensions must be kept
separate:

- **State validity** — whether a proposition holds in a given state of the path. `del` retires **one
  particular active assertion (or tuple)** and advances the path to a successor state in which *that
  support* no longer holds; `ins` introduces a fact that holds from the successor state onward.
  Retiring one support does **not** by itself make the *proposition* cease to hold: if the same
  proposition is still carried by another active assertion, or is derivable from other active facts, it
  remains valid in the successor state. `del` removes a support, not a conclusion — a proposition ceases
  to hold only when its last active support is retired and it is no longer derivable.
- **Historical retention** — whether the prior assertion remains recorded in the store. The project's
  suppression doctrine governs here: **`del` is a supersession, never a physical erasure.** Every
  retired assertion version carries `activeInState`, `supersededBy`, `validUntilState`, and
  `retiredByTransaction` provenance. The store is therefore monotonic and append-only at the
  substrate; the path supplies the before/after that state change requires.
- **Displayability / disclosure** — whether a consumer may see an assertion. This is a **separate
  projection and disclosure policy**, entirely orthogonal to state validity and retention. A fact
  may be fully valid in the current state yet non-displayable to a given consumer for privacy or
  access-control reasons. Conversely, a retired assertion may remain displayable in an audit view
  even though it no longer holds. **`del` does not set `displayable false`** — that would conflate
  state retirement with a disclosure decision that belongs to the projection layer.

The separation means a state-change history remains fully auditable: every superseded assertion
version is recoverable, the path records exactly when and by what step it stopped holding, and
disclosure policy can be applied independently without altering the validity or retention record.

## The hypothetical operator is not modal possibility

A transaction program can be tested **hypothetically** — executed to see whether it *would*
succeed, with its effects discarded rather than committed. This sandbox operator is the value
`HypotheticalExecution` of a **separate, orthogonal single-valued facet, `ExecutionMode`** (commit
vs. discard) — a *sibling* of the Evolution facet, not a value within it. Commitment and state-change
shape are independent dimensions: a program may be hypothetically executed under any Evolution value
(static, state-transition, or transaction-path), and the default `CommittedExecution` commits its
effects. It is **distinct from modal possibility (◇)**: ◇φ asserts that φ is possible in some
accessible world (an alethic or doxastic claim — see the context algebra in
[`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md)); the hypothetical operator asserts that a program *can
execute* from here without committing. The two may share construction machinery, but they are separate
typed operators with separate meanings, and conflating them would let an execution sandbox masquerade
as a statement about what is possible.

### How the sandbox operator is realized

A program reaches its execution mode through `executedUnderContract`, which links a transaction-program
root to the `ReasoningContract` whose `executionMode` it runs under; absence of that link resolves to
the default `CommittedExecution`, so an unannotated run commits exactly as before. The interpreter that
computes the verdict is the same in both modes — the path-existence question "would this program
succeed?" is decided identically whether or not the effects are kept. **The mode changes only what is
emitted.** A committed run materializes the full effect substrate: the executed path, the per-step
records, and the supersession of state. A hypothetical run emits the verdict and a single
content-addressed witness (`executedHypotheticallyAs`) and **suppresses the effect substrate entirely.**

This is why **suppression-never-erasure holds for free**: the discarded effects are never asserted in
the first place, so nothing is erased — hypothetical success is, at the emission layer, isomorphic to
committed failure (verdict present, substrate absent, start state untouched). The witness keeps the
discarded run observable: it records that the run occurred and what it ranged over, so a sandbox test
leaves an audit trace without leaving an effect.

The "shared construction machinery" between this operator and modal possibility is precisely the
**content-addressed keying** that gives each isolated run a stable identity — *not* the query-dispatch
substrate of the counterfactual machinery. Reusing the dispatch would couple the deliberately
effect-free transaction interpreter to a store and blur the operators; reusing only the keying keeps
them separate typed operators that happen to address their isolated runs the same way.

## Concurrent transactions

Concurrent Transaction Logic extends the path model to **interleaved** execution of more than one
program. It adds:

- **concurrent composition** — programs that advance together, their elementary steps interleaved;
- **three distinct, separately-declared concerns** — which this facet keeps apart rather than
  conflating, because they are not interchangeable and an implicit assumption about any one of them is a
  defect:
  - a **serializability criterion** — a *correctness property of a history*: *conflict-serializability*
    (an equivalent serial order exists with no conflicting-operation swap) or *view-serializability*
    (some serial order has the same read-from and final-write relationships). These are history
    properties, not protocols;
  - an **isolation level** — the declared *guarantee strength* a schedule must meet, up to and including
    full serializability and **opacity** (the stronger correctness condition that even aborted or
    in-flight transactions never observe an inconsistent state);
  - a **concurrency-control protocol** — the *implementation mechanism* that enforces the chosen level:
    *strict / strong-strict two-phase locking*, timestamp ordering, optimistic validation, and the like.
    A protocol is *how* a guarantee is achieved; it is never the guarantee itself.
- **serializability anomalies as history-level results** — a schedule that does not satisfy the
  declared notion is described as a `SerializationAnomaly`: a pattern of conflicting operations,
  read-from edges, and happens-before arcs in the transaction history that admits no equivalent
  serial execution under the declared policy. Lost updates, write skew, and read/write anomalies
  are history-level findings of this kind. They do **not** constitute a logical contradiction
  within a state: the final state after such a schedule can be perfectly logically consistent while
  still having no equivalent serial execution. A `SerializationAnomaly` is therefore distinct from
  a contradiction witness (which asserts ⊥ within a state) and must not be modelled as one.
  Non-serializable schedules are surfaced as findings with the dependency cycle or violated
  isolation level described; they are never silently linearized.

This is the level at which several agents acting over a shared memory, or parallel plans touching
overlapping state, are reasoned about.

### How concurrent composition is executed and classified

The native evaluator makes `logic:ConcurrentComposition` **executable**. It runs each leg
independently from the shared start state — the composition's executional-entailment verdict is
*both legs find a path from here* — and then **derives** a `logic:ConcurrentHistory` from the two
executed step sequences rather than reading an authored conflict graph. Each elementary step has a
**read set** (its schema's `logic:precondition` situations) and a **write set** (its effect's
`logic:ins` ∪ `logic:del` situations); two steps **conflict** when they touch a shared situation
that at least one of them writes. Conflict edges are oriented by a single **deterministic
index-order interleaving** — `left[0], right[0], left[1], right[1], …` — so the earlier operation
in that schedule `logic:precedes` the later one. Conflicts at different situations may orient in
*opposite* directions; between two transactions that is exactly the cycle (lost update / write
skew) that `detect_serialization_anomaly` classifies, and the derived edges feed the *same*
serialization-anomaly engine the authored path uses.

Two disciplines are load-bearing. First, **this is classification, not search** (Principle 12):
the engine evaluates one witnessed interleaving's conflict structure in polynomial time, bounded by
the same step governor the sequential core uses — it never enumerates schedules. Second, a
non-serializable run is **never silently linearized**: each leg materializes its own path
faithfully (there is no merged single chain that would impose a false serial order), the
`logic:ConcurrentHistory` is linked to its `logic:TransactionOutcome` by `logic:derivedHistory` and
to its two operands by `logic:concurrentComposedFrom`, and a cycle surfaces loudly as a
`logic:SerializationAnomaly` finding — a history-level result, never a contradiction witness.

Alongside conflict-serializability the evaluator classifies each derived history against the
**three separately-declared concerns** — the second serializability criterion, the isolation-level
guarantee, and the concurrency-control mechanism — each over the *same* witnessed interleaving, and
each a polynomial classification (Principle 12), never a schedule search.

**View-serializability.** The evaluator also decides *view-serializability*: it derives the
`logic:readsFrom` (which write each read observed) and `logic:happensBefore` (the cross-leg
schedule order) edges of the witnessed interleaving, and tests whether that interleaving is
view-equivalent — same read-from and same final-write relations — to one of the **two** serial
orders of its two legs. Because a `logic:ConcurrentComposition` is binary, exactly two serial
orders are checked, so this is a classification, not a search over the factorial schedule space.
The result is recorded as `logic:satisfiesCriterion` (one value per criterion the history
satisfies: `logic:ConflictSerializability` when the conflict graph is acyclic,
`logic:ViewSerializability` when the interleaving is view-equivalent to a serial order). For **two**
transactions view-serializability provably coincides with conflict-serializability — the strictly
weaker gap (a conflict-cyclic yet view-serializable schedule) needs three transactions with blind
writes — so the two criteria agree on every derived binary history; the `logic:readsFrom` /
`logic:happensBefore` edges are the witnessed-schedule structure that classification rests on.

**Isolation-level adequacy.** Because each leg runs independently from the shared start snapshot,
the evaluator realizes **snapshot isolation**. It classifies whether the witnessed schedule meets
the run's `logic:declaredIsolationLevel` and records `logic:isolationLevelAdequacy`: every level up
to and including `logic:SnapshotIsolation` is met by construction, while `logic:SerializableIsolation`
and `logic:OpacityIsolation` are met iff the schedule is conflict-serializable — a **write-skew
cycle**, which snapshot admits, is adequate for snapshot yet inadequate for serializability (with a
`logic:isolationInadequacyReason`). Opacity coincides with serializability here because only
committed runs produce a history, so there are no aborted or in-flight transactions to protect.

**Protocol soundness.** A run records the concurrency-control protocol's **own events** — per-leg
`logic:transactionTimestamp`, and per-action `logic:lockAcquired` / `logic:lockReleased` — so that
verifying whether the witnessed schedule respects the run's `logic:declaredProtocol` is a
polynomial well-formedness classification of the schedule, not the NP-hard search for a legal
schedule that a bare history would demand. The verdict is `logic:protocolEnforced` (with a
`logic:protocolViolationReason` on failure): `logic:StrictTwoPhaseLocking` requires the two-phase
order (no acquire after a release within a leg); `logic:StrongStrictTwoPhaseLocking` additionally
holds every lock to the commit step; `logic:TimestampOrdering` requires every `logic:precedes` edge
to run from the earlier timestamp to the later; and `logic:OptimisticValidation` requires no
cross-leg read-write conflict. A declared protocol whose required events are absent is a hard
error, never a silent pass.

## Where it connects, and what it is not

A transaction path says **how state changes**. It is connected to, but never identical with,
neighbouring notions: an intention says **what an agent commits to**; a causal account says **what
brought an event about**; a plan says **what is intended to be done**. The transaction facet models
the execution, and links to those neighbours; it does not absorb them.

## Constitutional alignment

State change here obeys suppression-not-erasure (every `del` is a supersession), and it enters the
system as one orthogonal facet value rather than a re-bundling of the others — the same
compositional discipline the reasoning contract enforces everywhere.
