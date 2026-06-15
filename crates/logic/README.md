<p align="center">
  <a href="https://github.com/Blackcat-Informatics/gmeow-ontology">
    <img src="https://raw.githubusercontent.com/Blackcat-Informatics/gmeow-ontology/main/docs/gmeow-logo.svg" alt="GMEOW logo" width="120" height="120">
  </a>
</p>

# `gmeow-logic` — Rust Reasoning Engine Core

[![crates.io](https://img.shields.io/crates/v/gmeow-logic.svg)](https://crates.io/crates/gmeow-logic)
[![docs.rs](https://docs.rs/gmeow-logic/badge.svg)](https://docs.rs/gmeow-logic)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](./LICENSE)
[![Repository](https://img.shields.io/badge/repo-Blackcat--Informatics%2Fgmeow--ontology-181717.svg)](https://github.com/Blackcat-Informatics/gmeow-ontology)

> **An LLM output is a claim, not a truth.**

`gmeow-logic` is the Rust core of the **GMEOW reasoning engine**. It models
possible worlds as [oxigraph](https://oxigraph.org/) named graphs and provides
world-indexed entailment queries, gated against the same language-neutral
conformance corpus as `gmeow-gts`.

---

## What this crate is

`gmeow-logic` is the Rust counterpart of the Python reference oracle
(`src/gmeow_tools/`) for the `logic:` vocabulary. It is the production engine
that backs world construction, entailment queries, and provenance capture.
Python remains the conformance oracle (slow, simple, correct); this crate is
the fast path.

The current scope is the **world-indexed storage layer**: an in-memory
`WorldStore` wrapping oxigraph that enforces isolated named graphs as worlds.
Nemo-based rule materialization and PyO3/wasm bindings arrive in later tasks.

---

## Oracle-parity discipline

- **A world is a named graph.** Every insert targets a specific named graph IRI;
  every query is scoped to a single named graph. No cross-world union queries
  are provided: a triple inserted into world `A` is never visible through a
  query on world `B`.
- **World-indexed only.** The public API exposes only `insert_quad(world, s, p, o)`
  and `quads_in_world(world)`. There is deliberately no dataset-union method.
- **Same conformance corpus.** The unit tests validate isolation semantics
  that both the Python oracle and this crate must satisfy identically.

---

## Build

> **Toolchain requirement:** nightly Rust is required. The `nemo` engine (a
> hard dependency) uses unstable features (`macro_metavar_expr`,
> `iter_intersperse`, `slice_swap_unchecked`) that are not available on stable.
> The repo ships a `rust-toolchain.toml` at the root that pins the channel to
> `nightly`; `cargo` and `rustup` pick this up automatically.

```bash
cargo build -p gmeow-logic
```

Via the Makefile:

```bash
make logic-build
```

---

## Test

```bash
cargo test -p gmeow-logic
```

Via the Makefile:

```bash
make logic-test
```

---

## Library API

```rust
use gmeow_logic::store::WorldStore;

let store = WorldStore::new();

// Insert triples into two isolated worlds
store.insert_quad("http://world/A", "http://ex.org/s", "http://ex.org/p", "http://ex.org/o1");
store.insert_quad("http://world/B", "http://ex.org/s", "http://ex.org/p", "http://ex.org/o2");

// World-indexed query: only world A's quads are returned
let a_quads = store.quads_in_world("http://world/A");
assert_eq!(a_quads.len(), 1);

// List worlds
let mut worlds = store.worlds();
worlds.sort();
assert_eq!(worlds, vec!["http://world/A", "http://world/B"]);
```

---

## Developer documentation

- [Logic Runtime Architecture](../../slices/core/logic/design/LOGIC-RUNTIME.md)
- [Logic Semantics](../../slices/core/logic/design/LOGIC-SEMANTICS.md)
- [Project Rationale](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/docs/RATIONALE.md)
- [GMEOW Constitution](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/CONSTITUTION.md)
- [Repository AGENTS.md](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/AGENTS.md)

### Building and testing locally

```bash
cd crates/logic
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

---

## Project and community

`gmeow-logic` is developed by [Blackcat Informatics® Inc.](https://blackcatinformatics.ca)
as part of the [GMEOW ontology and tooling](https://github.com/Blackcat-Informatics/gmeow-ontology)
suite.

Related packages:

- `gmeow-gts` — Graph Transport Substrate format engine (Rust)
- Python oracle: `src/gmeow_tools/` (PyPI: `gmeow`)

---

## License and copyright

Copyright © 2026 Blackcat Informatics® Inc.

This crate is licensed under the **Apache License, Version 2.0** — see the
[`LICENSE`](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/LICENSE)
file in the repository root.
