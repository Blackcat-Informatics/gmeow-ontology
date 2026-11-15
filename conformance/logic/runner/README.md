<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Conformance Runner Contract

The runner is **language-neutral**: every engine — the Python oracle, the Rust core, and any future
JS/Go port — implements one adapter, and the runner diff-compares each output against the case's
`expected/` files. Identical-files-or-red-build is what makes "oracle ≡ engine" (Principle 7) a
machine-checked guarantee rather than a hope.

> **Status: live (native).** Since the runner is the **native Rust
> `gmeow-conformance` datatest harness** (`crates/conformance`), run via `cargo nextest run -p
> gmeow-conformance` (wired to `make conformance`, and run automatically by the workspace
> `cargo nextest` in CI / `make check`). It drives the `gmeow_logic` engine cores directly —
> zero PyO3, zero Python — and diff-compares each output against the case's `expected/` files
> using the three comparison modes below. It replaced the retired Python `logic_runner.py`
> (the `gmeow-dev conformance` command), whose 31/31 pass-set it reproduced before deletion.
> The **Rust core is the reasoning authority**: every committed `expected/` artifact is the
> output it produces. Golden regeneration is `gmeow-dev conformance-bless <case>…`, which always
> names its cases. This file fixes the adapter interface every engine obeys.

## The adapter

An engine implements one function:

```text
run(input, mode) -> {
  materialized,    # an RDF dataset (named graphs = worlds)
  verdicts,        # world-indexed truth/modality + reasoning_lint-equivalent verdicts (JSON)
  witnesses,       # contradiction witnesses as a GMEOW statement graph (RDF)
  projections,     # the generated OWL-DL/EL, Datalog, N3, gUFO downcasts + preservation ledger
  explanations,    # per-query prose-explanation skeletons
  answers,         # per-query goal/counterfactual answer sets (JSON)
}
```

`input` is a case directory (`input.logic.ttl` + `profile.json` + optional `queries/`); `mode`
selects the engine/fragment (e.g. `native`, `owl-dl`, `owl-el`, `datalog`). The Rust core is the
canonical engine that the committed corpus is keyed to; the Python oracle must pass the identical
corpus as an independent cross-check; any future port self-certifies the same way.

## Comparison semantics

No case may depend on iteration order. Each expected artifact is compared by a fixed rule:

| Output | Comparison |
|---|---|
| `materialized.nq`, `witnesses.nq`, `projections/*.ttl` (RDF) | **graph isomorphism** (blank-node-aware, RDFC-1.0) |
| `projections/{datalog.dl, n3.n3}` (text) | **exact text equality** — deterministic, see below |
| `verdicts.json`, `answers/*.json` | **canonical JSON** — sorted keys, normalized literals |
| `explanation/*.md` | the **cited-IRI and rule-IRI skeleton**, *not* the surface prose |

The text projections are gated by *exact* equality, not graph-isomorphism, because the front-end
RDFC-1.0-canonicalizes every blank-node label before extraction (`parse_logic_store`): a rule atom's
identity is a deterministic function of the source graph (`c14n`-style labels), never the parser's
random per-parse id. A byte mismatch in these targets is therefore a real regression, not codec skew.

The explanation rule is load-bearing: a language model may vary the wording, but never the set of
axioms, rules, and sources it cites (the faithful-by-construction property — a generated explanation
may cite only IRIs that appear in the proof trace or witness graph it explains).

## Wiring

The runner is the native Rust `gmeow-conformance` datatest harness, wired into `make conformance`
(`cargo nextest run -p gmeow-conformance`) and run automatically by the workspace `cargo nextest`
in CI and `make check`. The corpus is an active gate: every `expected/` artifact is committed from
the native solver's output, and the harness fails if the current engine diverges from those
committed artifacts. The native solver output defines the canonical `expected/` files. Re-bless with
the maintenance command, naming each case by its id under `conformance/logic/cases` or by path
(`make maint-conformance-bless CASES="validation/reifier-shape"` runs the same command):

```sh
gmeow-dev conformance-bless validation/reifier-shape
```

The command runs each named case through the same `run_case` the producer uses, rewrites its
goldens, and prints every file that differs; read that diff before committing it. It reads only the
case directory and needs no producer output. It has no form that blesses every case, refuses an
unknown case, and writes nothing if any named case fails to run. A case that selects
`shipped_rules` needs the compiled rule library and is refused. No test may bless: the test runner
only compares.

Bless is **deterministic and idempotent**: because blank labels are canonicalized at the front-end,
re-blessing a fresh checkout produces a byte-identical tree (`bless → git diff` is clean; a second
bless is a no-op). It is also **refresh-only** — each case commits a *curated* subset of goldens (a
query case pins only its `answers/`; a projection case pins the projection set), and bless rewrites
**only goldens that already exist**, so it never spawns a target a case deliberately omits. To seed a
**brand-new** case, set `GMEOW_CONFORMANCE_BLESS_INIT=1` on that command to write the full produced set, then curate
(delete) the targets that case should not gate.

> The `expected/explanation/*.md` skeletons (content-hash-named, prose-bearing) and the
> `witnesses.json` side files are **not** regenerated by the bless mode — the gate compares only the
> explanation cited-IRI skeleton (verified against the existing goldens), and never consumed
> `witnesses.json`. Author those by hand for a brand-new case.
