<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Rust / GTS integration and version policy

This note explains how the `gmeow-gts` Rust crate is wired into the
`gmeow-ontology` build, why it is a required (not optional) dependency, the
nightly-toolchain compatibility policy that governs it, the licensing boundary
between this workspace and the crate, and how the GTS-backed validation path
works. It is aimed at new contributors who need to understand why a stable,
permissively-licensed crate is consumed under this workspace's nightly,
copyleft toolchain without contradiction.

For the architectural role of GTS as the single export artifact, see
[the narrow waist](./gts-narrow-waist.md). This note is the build- and
toolchain-level companion to that doctrine.

## Why `gmeow-gts` is a required dependency

`gmeow-validate` reuses the GTS bundle codec and verification logic to validate
a folded `.gts` snapshot directly, rather than re-implementing bundle parsing.
The dependency is declared unconditionally in `crates/validate/Cargo.toml`:

```toml
gmeow-gts = "0.9.5"
```

This is a hard dependency, not a feature-gated convenience. The project does
not ship optional capabilities or degraded fallbacks: a required component that
is missing is a build failure, surfaced and stopped, never silently routed
around. There is no "validate without GTS" mode — the `--gts` input path and
the repo-source path share one validation engine (see below), and that engine
links the crate unconditionally.

Two distinct things are both named `gmeow-gts`; keep them separate:

- The **library crate** `gmeow-gts` (version `0.9.5`) — a direct dependency of
  `gmeow-validate`. This is what this note is about.
- The **engine binary** `gmeow-gts` — obtained out-of-band via
  `cargo install gmeow-gts` and developed in its own repository
  ([Blackcat-Informatics/gmeow-gts](https://github.com/Blackcat-Informatics/gmeow-gts)).
  The workspace `Cargo.toml` documents this; the binary is not built from this
  tree.

## MSRV and nightly compatibility policy

This workspace builds on **nightly Rust**. The pin lives in
`rust-toolchain.toml`:

```toml
[toolchain]
channel = "nightly"
components = ["rustfmt", "clippy"]
```

Nightly is mandatory, not preferred. The native `gmeow-logic` engine uses
`portable_simd`; there is no stable fallback for the logic stack.

`gmeow-gts`, by contrast, targets **stable** Rust. This is not a conflict:
stable Rust is a subset of nightly, so a stable-targeting crate compiles
cleanly under the workspace's nightly toolchain. Nightly is a strict superset
of the surface a stable crate can use, so consuming `gmeow-gts` under nightly is
forward-compatible by construction. The workspace as a whole therefore requires
nightly for the native reasoning kernels, while individual crates like `gmeow-gts`
neither require nor are harmed by it.

The toolchain policy is to **track the current nightly** rather than freeze a
date:

- `rust-toolchain.toml` selects the floating `nightly` channel.
- CI selects the same floating channel: every `dtolnay/rust-toolchain` step in
  `.github/workflows/ci.yml` requests `toolchain: nightly`. (The `# nightly`
  comment on those steps annotates the pinned *action* commit, not a toolchain
  date.)

Local and CI toolchains are kept in lockstep this way. A specific nightly may be
**temporarily** pinned in both places when an upstream miscompile demands it —
but pinning is the exception, applied deliberately and removed once the upstream
issue clears. At present neither file pins a date; both float.

When bumping the toolchain, change `rust-toolchain.toml` and the CI
`toolchain:` inputs together so they never diverge.

## License implications

The two sides of this integration carry different licenses, and that is
intentional and sound:

- This workspace's code is **AGPL-3.0-only** (`license.workspace = true`).
- `gmeow-gts` is **Apache-2.0 / MIT** and stays that way.

An AGPL project depending on a permissively-licensed crate does **not**
relicense that crate, and the permissive license imposes **no** copyleft
obligation back onto the consumer. Apache-2.0 and MIT are deliberately
compatible with copyleft downstreams: `gmeow-validate` may link `gmeow-gts`
under AGPL terms, while `gmeow-gts` itself remains Apache/MIT for everyone else.
The dependency direction matters — GMEOW depends on GTS, never the reverse — so
no GMEOW copyleft term reaches into the GTS crate.

This documentation file is licensed **CC-BY-4.0**, matching the rest of
`docs/`; the SPDX regimes are by file role (docs → CC-BY-4.0, code and build
config → AGPL-3.0-only).

## How the GTS-enabled validation path works

`gmeow-validate` can validate either the repository's Turtle sources or an
already-folded `.gts` bundle. The bundle path is selected with `--gts`:

```bash
gmeow validate --gts generated/dist/gmeow.gts
```

Under the hood, `validate_all(gts_input=...)` is a thin Python wrapper over the
Rust-native orchestration `gmeow_validate.validate_all_native`. The Rust
engine builds the ontology store once, parses the SHACL shapes once, and runs
every phase against that shared store. `gmeow-gts` decodes and verifies the
bundle; `gmeow-rdf` then materializes its N-Quads projection into the same
Oxigraph in-memory store (with RDF 1.2 features enabled) that the rest of the
validator already uses.

In GTS mode the store is built from the bundle rather than from individual
source files, so the engine skips the phases that only make sense for a source
tree:

- the per-file Turtle **syntax** check and the **`owl:sameAs` ban**, which
  apply to individual source files, not to an already-folded graph;
- the source-layout phases (slice ownership, example coverage, per-example
  SHACL, mapping/statement DSL SHACL) and the Python-side per-file lints
  (guide-anchor, i18n PO), whose filesystem inputs are absent for a bundle.

Every **store-based** phase — structural lint, term naming, reasoning / gUFO
invariants, and the merged SHACL — runs unchanged. As the validator's own
docstring puts it, `--gts` differs only in input provenance, never in
validation semantics: the same store-based checks produce the same verdict
whether the triples came from the source tree or the bundle.

This mirrors the gts / gmeow seam: `gts` performs file-level operations
(verify, never convert), while `gmeow` performs ontology-level operations.
Validating a bundle amounts to GMEOW reading a graph that GTS produced and
verified, through the one narrow waist described in
[the narrow-waist doctrine](./gts-narrow-waist.md).
