<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — Transaction Logic

> Status: canonical target architecture for state-change reasoning. Transaction Logic is the
> meaning of the **Evolution = transaction-path** facet of the reasoning contract
> ([`LOGIC-CONTRACT.md`](LOGIC-CONTRACT.md)); it is a value of one orthogonal facet, **not** a
> separate profile. Member of the GMEOW Logic design set ([`LOGIC.md`](LOGIC.md)). The lineage is
> the Transaction Logic of Bonner and Kifer (see [`LOGIC-REFERENCES.md`](LOGIC-REFERENCES.md)).

## Why state change is its own facet

Classical logic evaluates truth at a single state of affairs. Reasoning about *change* — an agent
performing actions, a memory absorbing updates, a plan advancing — needs truth evaluated over a
**sequence** of states. This is an orthogonal concern: a state-change program may run under Horn,
well-founded, stable-model, or probabilistic consequence; over open or closed worlds; with or
without world-indexing. Because the choice is independent, state change is the value
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
- **executional entailment** — a transaction *succeeds* when there exists a path from the start
  state along which the program holds; failure leaves the start state untouched.

Static reasoning is the degenerate one-state path; it remains the default Evolution value, so the
rest of the system is unaffected unless a contract selects `transaction-path`.

## Elementary updates are supersession, never erasure

The state-changing primitives are `ins` (assert) and `del` (retire). Their meaning is bound by the
project's suppression doctrine: **a `del` supersedes — it sets the retired fact non-displayable and
advances the path to a successor state in which the fact no longer holds; it never physically
erases.** `ins` appends. The store is therefore monotonic and append-only at the substrate, while
the *path* supplies the before/after that change requires. This is what lets a state-change history
remain fully auditable: every superseded fact is still recoverable, and the path records exactly
when and by what step it stopped holding.

## The hypothetical operator is not modal possibility

A transaction program can be tested **hypothetically** — executed to see whether it *would*
succeed, with its effects discarded rather than committed. This sandbox operator is a value of the
Evolution facet's execution semantics. It is **distinct from modal possibility (◇)**: ◇φ asserts
that φ is possible in some accessible world (an alethic or doxastic claim — see the context algebra
in [`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md)); the hypothetical operator asserts that a program
*can execute* from here without committing. The two may share construction machinery, but they are
separate typed operators with separate meanings, and conflating them would let an execution
sandbox masquerade as a statement about what is possible.

## Concurrent transactions

Concurrent Transaction Logic extends the path model to **interleaved** execution of more than one
program. It adds:

- **concurrent composition** — programs that advance together, their elementary steps interleaved;
- **isolation and serializability** — the question of whether an interleaved execution is
  equivalent to *some* serial order of the same programs; a schedule that is not serializable is a
  conflict, surfaced as a finding rather than silently linearized.

This is the level at which several agents acting over a shared memory, or parallel plans touching
overlapping state, are reasoned about.

## Where it connects, and what it is not

A transaction path says **how state changes**. It is connected to, but never identical with,
neighbouring notions: an intention says **what an agent commits to**; a causal account says **what
brought an event about**; a plan says **what is intended to be done**. The transaction facet models
the execution, and links to those neighbours; it does not absorb them.

## Constitutional alignment

State change here obeys suppression-not-erasure (every `del` is a supersession), and it enters the
system as one orthogonal facet value rather than a re-bundling of the others — the same
compositional discipline the reasoning contract enforces everywhere.
