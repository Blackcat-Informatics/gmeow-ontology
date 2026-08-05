<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — The Operational `ReasoningSession` Façade

> The **session** member of the
> [GMEOW Logic document set](LOGIC.md#the-document-set). It specifies the stable, stateful
> operational surface an external runtime consumer pins against when it wants to *maintain* a
> reasoned closure under a stream of edits, rather than decide a single query. It is the
> operational sibling of the identity-only [`EngineContract`](LOGIC-RUNTIME.md#the-native-physical-engine--execution-and-optimization)
> pin. The engine architecture it sits on top of is in [LOGIC-RUNTIME.md](LOGIC-RUNTIME.md); the
> incremental algebra it promotes to a public surface is in
> [LOGIC-PERFORMANCE.md](LOGIC-PERFORMANCE.md); the reasoning contract it folds is in
> [LOGIC-CONTRACT.md](LOGIC-CONTRACT.md); the state-change discipline it borrows (supersession,
> never erasure) is in [LOGIC-TRANSACTION.md](LOGIC-TRANSACTION.md).

## Purpose — the stateful sibling of the engine pin

The reasoning runtime exposes two kinds of stable surface, and they are duals. The
[`EngineContract`](LOGIC-RUNTIME.md#the-native-physical-engine--execution-and-optimization) is the
**identity** surface: a self-describing, content-addressed pin an external consumer records at load
and re-asserts before trusting a previously-minted answer, so engine drift is *detected* rather than
silently tolerated. It is stateless — it names *which engine* decided *a* query. The
`ReasoningSession` is the **operational** surface: a long-lived, stateful handle that promotes the
crate-internal incremental maintenance engine — the DBSP-style signed-Z-set maintainer over
**finite positive binary Datalog** described in [LOGIC-RUNTIME.md § the native physical
engine](LOGIC-RUNTIME.md#the-native-physical-engine--execution-and-optimization) (lever 3) and
[LOGIC-PERFORMANCE.md](LOGIC-PERFORMANCE.md) — into a public session an external runtime consumer
can pin against and *keep fresh* across edits. Where the engine pin answers "is this the engine I
trusted?", the session answers "is this the maintained closure I have been extending, over the world
and rules and engine I trusted, and can I extend it once more, soundly, right now?"

The session lives in `gmeow_logic::runtime` — the same single import path as the `EngineContract`
and the dispatch/materialization surface. There is exactly one door. A consumer that wants
incremental maintenance takes `ReasoningSession` from `runtime`; it never reaches into the
`crate::cost` maintainer, the `crate::physical` fragment classifier, or the `crate::seam` fact
sources, all of which remain greenfield and free to churn. As with the engine pin, **stability is
delivered consumer-side, not as a backwards-compat freeze of the core**: the surface is stable
within a pinned git tag, and drift across tags is caught by the content-addressed descriptor
(see [Semver governance](#semver-governance--non_exhaustive-the--v1-tags-and-the-git-tag-pin)).

The session is deliberately *narrow*. It maintains exactly one certified fragment — finite positive
binary Datalog, the set the incremental maintainer accepts — and it is honest about everything
outside that fragment: a program the maintainer cannot incrementalise is either **routed** to a
labelled full rebuild or **refused** as unsupported, never silently approximated. This is the
no-optionality discipline applied to an operational surface: an explicitly selected incremental
profile is first-class, deterministic, cache-keyed, and fully validated; a fragment outside it is a
hard, typed disposition, not a weaker answer.

## The seven-axis `SessionIdentity`

A maintained closure is only meaningful relative to *everything it was maintained from*. The
`SessionIdentity` is the content-addressed binding of the seven identities a session depends on,
folded into one framed-BLAKE3 `descriptor_hash`:

1. **Published data-generation** — the `urn:blake3:` content address of the authorized EDB facts,
   minted through the *same* production typed-EDB bridge the maintainer prepares from, then rendered
   as sorted `(predicate, term-N3…)` rows and framed. Because the rendering is a deterministic
   function of the fact *set* (order-independent), `open` and `restore` mint the identical
   generation from the identical authorized EDB — the invariant `restore` gates on.
2. **Rule/program digest** and **slice digest** — two distinct framed digests of the canonical
   `logic:` `LogicProgram`: the program digest over its canonical rendering, and the slice digest
   over its authored source-IRI provenance (or, absent provenance, the canonical rendering under a
   *distinct* domain tag). Keeping them separate means a slice-provenance drift is a detectable
   identity change even when the compiled rule text is byte-identical.
3. **`ReasoningContract`** — pinned on its `content_digest`, which already folds every facet
   selection **including the resource policy** (budget, certified-fragment declaration). The
   resource policy is therefore not a separate eighth axis; it rides inside the contract digest,
   exactly as the runtime's materialized-graph key folds it (see
   [LOGIC-RUNTIME.md § Graph versioning](LOGIC-RUNTIME.md#graph-versioning-and-staleness)).
4. **Engine implementation/version** — the whole `EngineContract::current().descriptor_hash`. The
   session identity *folds the engine descriptor into itself*, so it is strictly finer than the
   engine pin: any drift the engine pin would catch, the session identity also catches, plus drift
   in the data, rules, contract, algebra, or fragment the engine pin is blind to.
5. **Tuple-annotation algebra** — the `AnnotationContract` canonical key: the `⊗`/`⊕` semiring the
   maintained tuples are annotated under (see [LOGIC-RUNTIME.md § Query-scoped external
   relations](LOGIC-RUNTIME.md#query-scoped-external-relations) for the algebra's role). A closure
   maintained under one annotation algebra is not interchangeable with one under another.
6. **Supported incremental fragment** — the fixed IRI naming finite positive binary Datalog, the
   exact set the maintainer certifies. It is an explicit axis so that widening the certified
   fragment in a later engine is a visible identity change, not a silent capability shift.

All framed together yield the `descriptor_hash` — the value a checkpoint pins and `restore` gates
on. The framing is the shared domain-tagged, **length-prefixed** BLAKE3 discipline used throughout
the runtime: each field is length-prefixed so no field boundary can collide with another, and every
digest carries a `-v1` domain tag (see [Semver governance](#semver-governance--non_exhaustive-the--v1-tags-and-the-git-tag-pin)).
Like the engine pin, the identity offers `assert_matches` (a typed hard-fail on drift) and a
`to_nquads` lossy projection so a consumer can fold the session identity into its own signed ledger
**as data**.

> **Strictly finer than the engine descriptor.** Because axis 4 embeds the whole engine descriptor,
> `SessionIdentity ⊒ EngineContract` as a drift detector: two sessions with equal `descriptor_hash`
> necessarily ran the same engine, but two runs of the same engine may carry different session
> identities. This is the property that lets a checkpoint refuse restore against a rebuilt engine —
> the engine descriptor is *inside* the identity, so a bumped engine mints a different
> `descriptor_hash` and the identity gate declines (see [Content-addressed
> checkpoints](#content-addressed-checkpoints)).

## `SessionDelta`, its two anchors, and `Suppression`

A `SessionDelta` is a content-addressed unit of change from an *already-authorized* workspace
commit. It carries only RDF datasets — additions (facts to insert, weight `+1`) and retirements
(active state to suppress, weight `-1`) — plus an optional committed-derivation budget, and it holds
**no authority handle**. This is the load-bearing boundary: **the session is not an authority
writer.** It references an authorized generation and extends a maintained closure over it; it never
mints a new authorized generation of the workspace. Authorship of authorized facts lives upstream,
in the workspace commit the delta departs from.

Because the session never mints authority, a delta needs **two distinct anchors**, and conflating
them would break the double-apply guard:

- **`base_commit` — the authorization anchor.** The authorized-EDB `WorldSourceIdentity` the delta's
  facts depart from. `apply` checks `base_commit == identity.data_generation`. This anchor is
  **invariant across applies**: since the session never mints a new authorized generation, the bound
  data-generation is unchanged before *and* after a delta commits. It therefore certifies *whose
  authorized world* the delta is entitled to extend — but it **cannot** discriminate a re-submitted
  delta, precisely because it does not move.
- **`expected_head` — the transition anchor.** The prior journal state-hash the delta must extend.
  `apply` checks `expected_head == head` (the current journal head). Unlike `base_commit`, the head
  **advances on every commit**, so this is the field that makes double-apply structurally
  detectable (see [Hash-linked transition journal](#the-hash-linked-transition-journal)).

Both preconditions are checked, in order, before any engine work: authorization first, then
transition. A mismatch on either is an `Invalid { PreconditionMismatch }` outcome — never a panic,
never a silent no-op advance.

**Retirement is suppression, never erasure.** A `Suppression` moves a set of rows' closure
membership `1 → 0`; it does **not** delete the physical fact. The physical fact arena is *monotone*:
the arena row and its provenance survive a suppression, aligned with the supersession-not-erasure
discipline of the transaction executor (see
[LOGIC-TRANSACTION.md](LOGIC-TRANSACTION.md)). This keeps deletion-survival decidable and keeps the
provenance graph a DAG — a suppressed fact that is later re-derived by an independent justification
correctly re-enters the closure, because its arena identity and alternative justifications were
never destroyed.

The delta's `delta_identity` is a framed-BLAKE3 content address over both anchors, the canonical
sorted content digest of the additions, each suppression's sorted content digest in list order, and
the step budget — deterministic and independent of the internal quad order within each dataset. It
is this content address, not the delta's in-memory identity, that the journal records.

## The hash-linked transition journal

The session is event-sourced. Each committed transition is a `TransitionEntry`:

```text
TransitionEntry = ( prev_state_hash, delta_identity, outcome_tag, new_state_hash )
  where new_state_hash = frame( "gmeow-logic-transition-v1",
                                prev_state_hash, delta_identity, outcome_tag-as-byte )
```

The **genesis head** is the `SessionIdentity` descriptor hash: a fresh session starts its chain
anchored to *what it is*, so the very first transition is already bound to the full seven-axis
identity. Every subsequent commit advances `head` to the new entry's `new_state_hash`, a framed link
over the prior head, the applied delta's content address, and the data-only outcome tag.

This makes **double-apply structurally impossible**, not merely guarded by a seen-set. A committed
delta advances the head to `new_state_hash`. Re-submitting the *same* delta carries the *same*
`expected_head` — which is now the stale prior hash — so the transition precondition
`expected_head == head` fails and the re-submission is refused `Invalid` before any engine work. No
in-memory set of seen deltas is needed: the guard is a property of the hash chain itself. Because
`head` is also what a durable checkpoint stores and `restore` re-adopts (see [Content-addressed
checkpoints](#content-addressed-checkpoints)), the guard **survives crash and restart** — a delta
committed before a crash cannot be re-applied after recovery, since the restored head already reflects
its commit.

The `outcome_tag` folded into the link is the *data-only* discriminant of the outcome — a stable
wire byte per outcome class — never the (non-hashable) evidence payloads. The chain therefore records
*which class* of transition occurred at each step, hash-linked and replayable, without dragging the
run's cost vector or diagnostic into the hash.

## Content-addressed checkpoints

A `Checkpoint` is a durable, content-addressed snapshot of a session's *position*, not its *circuit*.
It stores exactly three things — the seven-axis `SessionIdentity`, the authorized EDB generation, and
the journal head — plus a framed-BLAKE3 `content_address` over those three. It carries **no circuit
state**: no serialized DBSP dataflow, no iteration history, no maintained arena.

**Restore re-materializes; it does not deserialize the circuit.** `restore` re-runs `open` over the
authorized EDB, program, contract, and annotation, deterministically rebuilding the maintained
closure from scratch, then re-adopts the checkpoint's durable head. This is a deliberate design
choice with two justifications:

- **The iteration history is load-bearing and fragile.** A DBSP circuit's incremental state is its
  accumulated iteration history — a representation tightly coupled to the exact solver version,
  join-plan, and internal id assignment that produced it. Serialising it would make a checkpoint
  un-restorable across any solver-version bump that touched that representation, turning an internal
  optimization into a compatibility burden. Re-materialization has no such coupling.
- **Re-materialization is deterministic and there is no measured perf need.** `open` over a fixed
  authorized EDB and program produces the byte-identical closure every time (the same discipline the
  data-generation axis relies on), so a re-materialized session is *indistinguishable* from the one
  the checkpoint captured. Absent a measured cost pressure, the deterministic rebuild is strictly
  preferable to a fragile serialized circuit.

A restore passes a strict, ordered gate — each step a typed refusal carried on the API's own
`OperationOutcome`, never a panic and never a silently-coerced rebuild:

1. **Content integrity.** The checkpoint's `content_address` is recomputed and compared; a mismatch
   is `Invalid { CorruptCheckpoint }` (the bytes were tampered with or corrupted).
2. **Deterministic re-materialization.** `open` rebuilds the session; an engine fault here is
   `EngineFailure`.
3. **Identity gate.** The checkpoint's `descriptor_hash` must equal the freshly-reconstructed one.
   Because all seven axes fold into that one hash, a mismatch on **any** axis — data, rules, slice,
   contract, engine, annotation algebra, or fragment — is a single `Invalid { IdentityMismatch }`.
   This is why `restore` takes the *full* `open` inputs including the contract and annotation:
   `contract_hash` and `annotation_identity` cannot be reconstructed from `(edb, program)` alone, and
   all seven axes must be re-derivable to be gate-checked.
4. **Explicit data-generation gate.** A precise `IdentityMismatch` on the EDB axis specifically, so a
   drifted authorized world is refused with a pinpointed cause.

The identity gate is what makes a checkpoint safe against a rebuilt world. **A rebuilt engine refuses
the checkpoint** because the engine descriptor is one of the seven axes inside the identity: a bumped
engine mints a different `descriptor_hash`, step 3 declines, and the stale incremental position is
never resurrected under an engine that might interpret it differently. A checkpoint is thus restorable
*only* against matching data / rule / slice / engine / contract / annotation / fragment identities —
content-addressed and identity-gated together.

## The total, six-way `OperationOutcome`

Every `apply`-family method (`apply`, `restore`, `restart`) is **total**: it never panics and always
returns exactly one `OperationOutcome`. The six variants are disjoint and exhaustive, and each
carries the typed evidence that justifies its classification, so a consumer never re-derives the
reason from a string:

1. **`Applied`** — a genuine incremental maintenance; the session state advanced. Carries the full
   `NativeIncrementalRun` evidence (signed closure changes, the decomposable `(rule, predicate,
   stratum)` cost vector, per-fact derivations, consumed steps) and the new hash-linked journal head.
2. **`RequiresFullRebuild`** — sound, but not servable incrementally; the caller must rebuild from
   scratch. State unchanged. Carries a typed `RebuildReason` (additions outside the incremental
   fragment; a step-budgeted delta that also retires state, whose bounded retraction has no sound
   partial-delete frontier; or contract/engine drift since a checkpoint).
3. **`UnsupportedFragment`** — the *fixed* program is outside the maintainable fragment; no
   incremental application is possible and no approximate closure is presented. Carries the typed
   `UnsupportedFragment` condition (non-stratifiable negation, cut, arithmetic, non-binary atom,
   floundering, non-terminating existential/arithmetic, clause-body-too-wide).
4. **`Incomplete`** — the operation ran under a budget that cut it before fixpoint; state unchanged
   (the maintainer commits only on a complete `Ok` run). Carries the `BudgetStatus` at the cut and
   the typed `IncompleteCause` (step budget, cancellation, deadline, or a paged source's page/byte
   budget).
5. **`Invalid`** — a precondition or integrity gate refused the operation; state unchanged. This is
   how double-apply, mismatched restores, corrupt checkpoints, and illegal signed transactions are
   refused, each a typed `IntegrityFault`.
6. **`EngineFailure`** — a genuine engine fault, distinct from an unsupported fragment (which is
   classified typed at `open`). Carries the raw diagnostic.

Underneath the per-operation outcome sits the **two-tier `FragmentDisposition`**, decided **once** at
`open` over the *fixed* program and consulted by every `apply`:

- **`Incremental`** (Tier 1) — within finite positive binary Datalog. A live maintainer is prepared;
  this is the sole path that yields `Applied`.
- **`RequiresFullRebuild`** (Tier 2) — outside the incremental fragment but **decidable by the full
  native reasoner** (e.g. stratified NAF, a terminating/weakly-acyclic existential chase). Every
  `apply` routes to `RequiresFullRebuild`.
- **`Unsupported`** (Tier 3) — a **hard gap** (non-stratifiable negation, a chase whose termination
  cannot be certified, an unsafe/floundering rule, a clause body wider than the backward solver's
  64-literal selection mask). Every `apply` returns `UnsupportedFragment`.

The disposition is **single-sourced against the existing engine certifiers** — it never
re-implements a stratification or acyclicity checker. Tier 1 is the incremental-fragment certifier;
the exact static gaps (non-stratifiable, clause-body-too-wide, non-terminating existential) are the
seminaive core's and chase-admission certifier's own verdicts; the residual Tier-2/Tier-3 split is
decided by whether the **full native reasoner accepts the program under a bounded, guaranteed-
terminating probe** (`Ok` ⇒ decidable ⇒ Tier 2; `Err` ⇒ a hard lowering/planning gap ⇒ Tier 3), with
the unsupported *kind* taken from the typed incremental refusal, never a string match. The
disqualifying principle is uniform: **an unsupported fragment is refused or explicitly routed to a
full rebuild, never silently approximated.** There is no fallback engine that turns a gap into a
plausible-but-wrong closure.

### Forward-reachable vs. backward-only `UnsupportedFragment` kinds

The `UnsupportedFragment` enum is the **shared public vocabulary** of both the forward classifier a
`ReasoningSession` consults and the backward SLD/magic-set + FOL-resolution engines. Only a subset of
its kinds can be produced by a forward `open`/`apply`; the acceptance suite
(`reasoning_session_refusal.rs`) asserts EXACT labels only for those:

- **Forward-reachable, exact-labelled** — `NonStratifiable` (a negative dependency cycle) and
  `ClauseBodyTooWide` (a clause body wider than the backward solver's 64-literal selection mask). Both
  are asserted at their exact typed label at `open` (`FragmentDisposition::Unsupported(kind)`) and at
  `apply` (`OperationOutcome::UnsupportedFragment { kind }`).
- **Not forward-reachable — covered only by the universal never-`Applied` property**:
  - `NonTerminatingExistential` — an authored `Formula` existential (even an n-ary head) lowers into
    single-head eval rules; the chase-admission gate inspects only `nary_head_rules`, which stays
    empty for a formula-authored forward program, so the forward classifier routes such a program to
    `RequiresFullRebuild`, never this label.
  - `Floundering`, `NonTerminatingArithmetic`, `Cut` — **backward-reasoner-only** kinds, produced by
    the query-directed backward engines (`physical/magic.rs`, `physical/resolve_fol.rs`) for a goal,
    never by the forward incremental/full-native classifier. `Cut` is not even constructible on the
    authored forward Horn surface (no `!` control construct). These remain live enum variants used by
    the backward reasoner; the forward session simply cannot reach them, so the suite asserts only the
    universal "never silently `Applied`" guarantee for them.

## Paged world-source composition

A session need not open over a resident EDB. `open_paged` composes the session over the paged RDF 1.2
view backend — the succinct-pack / paged world-source — driving it through the same fallible-view
seam the runtime's paged dispatch uses (see [LOGIC-RUNTIME.md § Query-scoped external
relations](LOGIC-RUNTIME.md#query-scoped-external-relations)). It pages in every quad of the single
authorized named world, **paying page faults** through the demand provider, freezes the collected
facts into a resident EDB, and prepares the incremental maintainer over it.

Two things make this a first-class composition rather than a convenience wrapper:

- **The paged data-generation is threaded into the session identity.** The paged source's
  `WorldSourceIdentity` becomes axis 1 of the `SessionIdentity`, so a session composed over the paged
  backend is drift-detectable exactly as a resident one is — and a cross-view test can assert that
  the resident, paged, and pack folds of the *same* world produce the identical closure fingerprint,
  while the `PagedCompositionMetrics` (structural source-access metrics plus the backend's
  per-page-fault accounting) attest the paging actually happened.
- **Operational failures and query faults map to distinct outcomes.** A page fault surfaces through
  the fallible view's *operation status*, so the post-scan status is authoritative. An **operational**
  paged failure — cancellation, deadline, page/byte budget exhaustion, or stale generation — maps to
  `Incomplete` with the precise `IncompleteCause`; a data-corruption / invalid-data fault, or any
  other engine/materialization failure, maps to `EngineFailure`. The mapping is typed off the paged
  error variants, **never a string match**. This preserves the runtime's standing distinction: an
  operational incompleteness is never reported as semantic absence, and never conflated with a genuine
  engine fault.

The composition is otherwise identical to a resident `open`: the same fragment classification, the
same seven-axis identity binding (now over the paged generation), the same genesis journal head, the
same total outcomes on every subsequent `apply`.

## The ontological twins of these constructs

Every construct specified above was, for a time, specified **in Rust prose with no ontological
twin** — a durable-execution vocabulary that the runtime implemented and the ontology could not
describe, validate, derive over, or certify. That gap is closed: the enactment kernel
([`LOGIC-ENACTMENT.md`](LOGIC-ENACTMENT.md)) mints the twin of each, domain-neutrally, so the same
distinctions are available to a consumer describing *its own* durable work rather than only to a
consumer maintaining a reasoned closure. The twins are named, not paraphrased:

| Rust construct here | Ontological twin | Note |
| --- | --- | --- |
| the hash-linked journal | `logic:TransitionJournal` | one per `logic:Enactment`; its head is `logic:journalHead` |
| `TransitionEntry` (`prev_state_hash`, `delta_identity`, `outcome_tag`, `new_state_hash`) | `logic:JournalEntry` with `logic:journalPrevHead`, `logic:journalDeltaIdentity`, `logic:journalOutcomeTag`, `logic:journalNewHead` | chain integrity is `logic:JournalChainIntegrityConstraint`: `prevHead` must equal the predecessor's `newHead` |
| `SessionDelta` — additions plus suppressions, content-addressed, order-independent | `logic:SnapshotDelta` | the construct that makes "what changed since the last occurrence" a modelled fact rather than a diff someone recomputes |
| `expected_head`, the transition anchor | `logic:expectedJournalHead` on `logic:DispatchIntent` | carried into the pre-effect record, which is what makes no-blind-retry a structural property of the chain rather than an authored rule |
| `Suppression` — closure membership `1 → 0`, never physical deletion | the supersession quartet, reused unchanged | the twin already existed; the kernel adds no second retirement mechanism |
| `Checkpoint` — identity, generation, and head, content-addressed | `logic:EnactmentCheckpoint` / `logic:StepCheckpoint`, with `logic:checkpointDescriptorHash` | the folded descriptor mirrors `SessionIdentity::to_nquads` |
| the seven-axis `SessionIdentity` | the seven checkpoint identity axes, twinned individually | a checkpoint recording six of seven is rejected; `logic:CheckpointRestoreIdentityConstraint` pins the four-step restore gate **in order** — content integrity, re-materialization, identity gate, data-generation gate — so a corrupt checkpoint is refused before an identity is computed over it |
| the total, six-way `OperationOutcome` | `logic:OperationOutcome` with `OutcomeApplied`, `OutcomeRequiresFullRebuild`, `OutcomeUnsupportedFragment`, `OutcomeIncomplete`, `OutcomeInvalid`, `OutcomeEngineFailure` | closes a real gap: `logic:EvaluationStatus` is three-way with no cancellation and no engine-failure value |
| `RebuildReason`, `UnsupportedFragment`, `IncompleteCause`, `IntegrityFault` | `logic:RebuildReason`, `logic:UnsupportedFragmentCondition`, `logic:IncompleteCause`, `logic:IntegrityFault` | **all four** typed evidence enumerations, not the two that are easy; `logic:IntegrityFault` is the restore gate's own taxonomy |
| `restart` versus `restore` versus re-`open` | `logic:ContinuationKind` — `logic:ContinuationResume` / `Repeat` / `Revise` | pairwise disjoint with distinct required bindings, so the three are not reconstructable-by-convention |

The twins are pinned against these types rather than merely inspired by them: the outcome and cause
enumerations are cross-checked against the Rust enums in the manner of the existing
reasoning-result cross-check, so divergence fails the build rather than accumulating silently.

## Semver governance — `#[non_exhaustive]`, the `-v1` tags, and the git-tag pin

The session surface follows the same consumer-side stability model as the `EngineContract`, with
three mechanisms:

- **Every public session type is `#[non_exhaustive]`.** `ReasoningSession`, `SessionIdentity`,
  `SessionDelta`, `Suppression`, `Checkpoint`, `TransitionEntry`, `OperationOutcome`,
  `FragmentDisposition`, `RebuildReason`, `UnsupportedFragment`, `IncompleteCause`, `IntegrityFault`,
  `OutcomeTag`, and `PagedCompositionMetrics` are all non-exhaustive, and construction is only via the
  provided constructors (fields are private on the façade). Adding a variant, field, outcome, or
  identity axis is therefore an **additive (minor)** change that cannot break a conforming consumer's
  match arms or constructions.
- **The `-v1` domain tags are the semver-stable serialization contract.** Every framed digest and
  hash-link carries an explicit `…-v1` domain tag — the identity descriptor, the EDB generation, the
  program/slice hashes, the delta identity and its additions/suppression sub-digests, the checkpoint
  address, and the transition link. Those tags, together with the `OutcomeTag` wire bytes, are the
  stable serialization contract: a byte that a consumer records today re-verifies tomorrow under a
  pinned tag, and any *incompatible* change to a framing bumps its domain tag (`…-v2`) — a visible,
  detectable break rather than a silent reinterpretation of old bytes.
- **Drift is detected by the descriptor hash, exactly as for `EngineContract`.** Stability is
  delivered within a pinned git tag; across tags the core is greenfield and free to churn. A consumer
  is protected by its git-tag / vendor pin *plus* the content-addressed `SessionIdentity`, not by a
  repo promise to preserve these names. Because the session identity folds the engine descriptor, a
  consumer that pins a session descriptor gets the engine pin's drift detection *and* the extra
  data/rule/contract/annotation/fragment coverage for free — the same `assert_matches` hard-fail
  discipline a signed-ledger consumer already uses to refuse an entry under a wrong signature.

---

*Types named here — `ReasoningSession`, `SessionIdentity`, `SessionDelta`, `Checkpoint`,
`OperationOutcome`, `FragmentDisposition` — are the `gmeow_logic::runtime` re-exports; the incremental
maintenance algebra beneath them is in [LOGIC-PERFORMANCE.md](LOGIC-PERFORMANCE.md), and the engine
architecture they sit on is in [LOGIC-RUNTIME.md](LOGIC-RUNTIME.md).*
