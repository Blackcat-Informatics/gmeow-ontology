<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# CLI & MCP Architecture — the Rust `gmeow` / `gmeow-dev` surfaces

> **Status:** as-built. The `gmeow` (consumer) and `gmeow-dev` (developer) command
> surfaces and the MCP interface are native Rust; the Python/Typer CLI and its
> PyO3-seam tests have been retired. This document is the canonical crate-selection
> record and three-surface architecture *as it shipped* — it describes what the code
> does, not a plan for future work.
>
> **Genre.** A normative architecture spec for the two user-facing command-line
> surfaces (`gmeow`, `gmeow-dev`) and the MCP interface. Peer to
> [`PIPELINE_SPINE.md`](./PIPELINE_SPINE.md) (the regeneration spine this CLI
> drives) and [`cli-extensions.md`](./cli-extensions.md) (the subcommand-discovery
> model this CLI respects). Governed by [`CONSTITUTION.md`](../CONSTITUTION.md)
> and [`AGENTS.md`](../AGENTS.md).

## 0. Why this exists

The `gmeow` (consumer) and `gmeow-dev` (ontology-developer) command surfaces were
originally **Python/[Typer](https://typer.tiangolo.com/) + [Rich](https://rich.readthedocs.io/)**
apps that called the Rust core through the `gmeow_native` PyO3 cdylib, verified only
via Python `CliRunner`/subprocess tests — tests that could not be retired until the
CLI itself was Rust. Porting the command surface to Rust (rust-first, greenfield) both
retired those tests and unlocked a **next-generation command-line experience**: one
output engine that serves a *human* developer at a TTY and an *agent* running the tool
thousands of times a session, off the same structured event stream.

Three facts made the port smaller and safer than it first appeared — and they shaped
every decision below:

1. **The reporting backbone was already Rust and canonical.** `crates/diagnostics`
   (`gmeow_diagnostics`) is a serde model that renders to JSON, SARIF, RDF, HTML, text,
   and **NDJSON**, with a ten-way `FindingCategory` taxonomy, GTS wire-coordinate
   `Location`s, and slice attributions. The pipeline emits a structured `RunReport`
   (`crates/pipeline/src/run.rs`) with per-stage/per-level `TimingRecord` critical-path
   timings. The port moved the *presentation seam* from Python/Rich onto this existing
   Rust model — it did not build a diagnostics engine.
2. **The MCP layer was already Rust** (`crates/pipeline/src/mcp.rs`) — a hand-rolled
   JSON-RPC 2.0 server with Transaction-Logic memory semantics. Surface (c) was a
   *native re-launch*, not a from-scratch build.
3. **Greenfield forbids a stub.** [`CONSTITUTION.md`](../CONSTITUTION.md) §6 requires
   removing an inferior element rather than retaining it for compatibility. A
   half-parity Rust CLI living beside the Python one would *be* that violation, so the
   port proceeded **surface by surface as replace-and-delete**, never as a parallel
   implementation.

## 1. Constraints that bind the design

- **Greenfield / no backwards-compat** ([`CONSTITUTION.md`](../CONSTITUTION.md) §6):
  each surface was replaced and its Python predecessor deleted in the same change.
- **Rust-first, no new Python** ([`AGENTS.md`](../AGENTS.md)): the CLIs are Rust
  binaries; the only Python touched was the code being *deleted*.
- **No optionality / hard-fail**: output modes are a closed enum; a missing required
  input stops and reports. No degraded fallbacks, no optional-dep feature gates that
  change core behavior — and no env-activated observability layer that is present or
  no-op depending on the environment.
- **The `gmeow` vs `gmeow-dev` razor** ([`AGENTS.md`](../AGENTS.md) §CLI): `gmeow`
  works **from the installed wheel alone** — the bundled `gmeow.gts` snapshot, no
  source checkout, no repo-local query trees. `gmeow-dev` is repository maintenance and
  may read anything in the tree. This razor is enforced *structurally* (see
  [§4](#4-shared-core--cratescli-core-gmeow-cli-core)).
- **Gate-latency optimization culture** ([`docs/rust-test-performance.md`](./rust-test-performance.md)):
  the tool is run thousands of times a session, so startup latency and per-invocation
  cost are first-class. Heavy dependencies (async runtimes, telemetry exporters) stay
  off the hot path — and, where they add no net value, are not linked at all.
- **PIPELINE_SPINE laws** ([`PIPELINE_SPINE.md`](./PIPELINE_SPINE.md)): the CLI
  *drives* the spine but does not violate it — one terminal, superset law, fanout is
  pure projection.
- **Subcommand-discovery model** ([`cli-extensions.md`](./cli-extensions.md)):
  extension subcommands are discovered via `gmeow:providesSubcommand` manifests /
  entry points; the Rust `gmeow` dispatcher keeps that seam intact.

## 2. Crate record

The crates and versions below are what the shipped CLI crates
(`crates/cli-core`, `crates/gmeow-cli`, `crates/gmeow-dev-cli`) actually depend on.
Bump deliberately, never float.

| Concern | Crate | Version | Rationale |
|---|---|---|---|
| Argument parsing | **clap** (derive API) | 4 | The de-facto standard; the derive API is type-safe and its nested-subcommand support cleanly models `gmeow-dev`'s `box-roles` / `logic` / `i18n` sub-apps. |
| Colour / styling | **anstyle** + **anstream** | 1 / 0.6 | `NO_COLOR`-aware, terminal- and Windows-aware; the transitive styling layer clap already uses, so no additional colour crate is pulled in. |
| Live progress | **indicatif** | 0.17 | `MultiProgress` bars keyed to the scheduler's parallel topological *levels*; TTY-gated so agents never receive progress bytes. |
| Tables | **comfy-table** | 7 | Replaces Rich `Table` for `info` / `coverage` / `audit` tabular views. |
| Diagnostics model | **`gmeow_diagnostics`** (reuse) | in-repo | The canonical serde model with JSON/SARIF/RDF/HTML/text/NDJSON renderers, category taxonomy, and GTS-coordinate locations. Reused, not re-invented. |
| Logging | **tracing** + **tracing-subscriber** | 0.1 / 0.3 | Instrument-once / many-sinks; **stderr-bound**; `EnvFilter` via `GMEOW_LOG` / `RUST_LOG`; near-zero cost when a level is disabled (hot-path safe). Also carries the stdio MCP server's diagnostics (see [§3](#3-two-output-channels)). |
| MCP server | **in-repo** (`crates/pipeline/src/mcp.rs`) + **serde_json** | in-repo / 1 | A hand-rolled JSON-RPC 2.0 stdio server owning the Transaction-Logic memory triad, resource routing, and startup language validation. No external MCP SDK — see [§7](#7-surface-c--mcp-interface). |
| CLI testing | **assert_cmd** + **trycmd** + **predicates** | 2 / 0.15 / 3 | The parity-coverage toolset the port needed; `trycmd` snapshots the human surface, `assert_cmd`/`predicates` assert exit codes and machine output. |

### 2.1 Deliberate rejections (recorded so they are not re-litigated)

- **rmcp / any external MCP SDK** — *not* adopted. Migrating the MCP server onto an
  external SDK would risk the bespoke Transaction-Logic memory semantics (the
  `store_claim` / `recall` / `revise_belief` triad, dry-run, compensation-as-rollback)
  for no functional gain the agents need. The in-repo hand-rolled JSON-RPC 2.0 server
  is canonical ([§7](#7-surface-c--mcp-interface)).
- **OpenTelemetry / OTLP exporters** — *not* adopted. An env-var-activated,
  no-op-when-absent OTLP layer is runtime optionality / a degraded fallback, which the
  no-optionality law ([§1](#1-constraints-that-bind-the-design)) forbids; it also adds
  an async-runtime hazard to a synchronous CLI. Diagnostics stop at the `tracing`
  stderr sink and the machine-readable NDJSON product stream.
- **clap_complete / clap_mangen** — *not* in the shipped surface. Shell completions and
  generated man pages are DX niceties the port did not require to reach parity; they
  are not linked.
- **thiserror error enum** — *not* used by the CLI. Exit status is a small closed
  mapping from the diagnostics `Report` ([§4](#4-shared-core--cratescli-core-gmeow-cli-core)),
  not a typed error taxonomy, so no error-derive crate is pulled in.
- **ratatui** — *not* adopted. Agents do not consume a full-screen TUI, and <!-- codespell:ignore ratatui -->
  developers are well served by `indicatif` + `comfy-table`; a TUI would fight the
  NDJSON contract and inflate startup.
- **miette** — *not* the diagnostics model. `gmeow_diagnostics` is the canonical model
  with SARIF/RDF projections; a second diagnostic model would violate no-optionality.
- **A busybox-style single multi-call binary** — rejected in favour of two bin crates
  sharing a `gmeow-cli-core` library, so the wheel-only razor is *structurally*
  enforceable: the `gmeow` binary never links repo-maintenance code.

## 3. Two output channels

Every surface obeys one rule: **stdout is product output; stderr is diagnostics.**
Conflating them is the classic CLI bug and, for the MCP server, an outright protocol
corruption.

- **stdout = product output** — a command writes its actual answer (the computed
  result) to stdout **itself**; the `Reporter` does not own the product answer. In
  `jsonl` mode the `NdjsonReporter` adds its machine event frames (findings, `stage_*`,
  `summary`) on stdout too. This is a **stable, versioned contract** with the caller.
  Where determinism is required — notably the pipeline `RunReport` and `--timings-json`
  — the payload is product *data*, not a log, and is **never** routed through the
  logging layer (whose field ordering is non-deterministic).
- **stderr = diagnostics** — the `Reporter`'s *diagnostic* channel: the `HumanReporter`'s
  coloured findings, plus `tracing` spans and events (stage timings, MCP
  request-correlation ids, debug/trace), filtered by `GMEOW_LOG` / `RUST_LOG`, default
  quiet (`warn`) so `gmeow describe` stays silent and fast unless asked.

The `Reporter` therefore owns the **diagnostic** surface, not the product answer:
`HumanReporter` renders diagnostics to stderr, `NdjsonReporter` frames them as NDJSON on
stdout for agents, and the command itself is what emits the product result.

`tracing` is the **stderr** diagnostic log, and only that: `init_tracing()` installs a
single stderr subscriber (a `fmt` layer under a `GMEOW_LOG` / `RUST_LOG` `EnvFilter`),
never a stdout layer. That is what satisfies the stdio-MCP rule — *never write to
stdout* — **structurally**: all MCP logging flows through the stderr `tracing`
subscriber, so it can never corrupt the JSON-RPC stream on stdout. The
`GMEOW_PIPELINE_TIMING` stderr timing dump is ordinary `tracing` output and does not
disturb the deterministic `--timings-json` artifact.

The machine-readable NDJSON stream is a **separate** concern from that log: it is owned
by the `NdjsonReporter` (§4), which writes its frames to stdout in `jsonl` mode — it is
**not** a `tracing` layer. So the distinct channels are: the command's product answer
(stdout), the `NdjsonReporter`'s machine frames (stdout, `jsonl` mode), and the
`tracing` diagnostic log (stderr). There is no third telemetry-export sink (see
[§2.1](#21-deliberate-rejections-recorded-so-they-are-not-re-litigated)).

## 4. Shared core — `crates/cli-core` (`gmeow-cli-core`)

A library crate owning everything both binaries and the MCP launcher share:

- **`ConsoleMode`** — a closed `clap::ValueEnum` `auto | pretty | text | jsonl | silent`,
  mirroring the original Python vocabulary so behavior is continuous across the port.
  Resolution precedence is **flag > env (`GMEOW_*`) > default**; an unrecognized env
  value falls through to the default rather than hard-failing, and the flag stays the
  authoritative override.
- **`Reporter`** trait (`report` / `stage_start` / `stage_end{elapsed}` / `summary`),
  object-safe (`&dyn Reporter`), with two implementations:
  - `HumanReporter` — renders diagnostics to **stderr** as `anstyle`-coloured text
    (via `gmeow_diagnostics::render::to_text`), leaving stdout clear for product
    results; `indicatif::MultiProgress` drives one bar per parallel pipeline *level*
    from the scheduler's `StageTiming` / `LevelTiming`, plus `comfy-table` summaries.
  - `NdjsonReporter` — one JSON object per line to stdout with a stable event schema
    (`stage_start` / `stage_end{elapsed_ms}` / `finding` / `summary`), line-framing the
    canonical `gmeow_diagnostics` findings (framed, never re-serialized in a second
    model).
- **DX rule:** `auto` resolves non-TTY → **`jsonl`** so agents get machine output by
  default (Python's `auto` fell to `text`); human `auto` at a TTY → `pretty`.
- **Exit codes:** the small, stable convention `0` = clean report, `1` = any failure
  (mapped from the diagnostics `Report` by `cli_core::exit_code`); `2` is reserved for
  clap usage errors, which **clap** emits itself. No sysexits-style per-category codes.
- **One-line `tracing` init:** `init_tracing` installs a stderr subscriber with a
  `GMEOW_LOG` (then `RUST_LOG`) `EnvFilter` defaulting to `warn`. It is idempotent
  (`try_init`), so every binary and the MCP launcher may call it once at startup;
  product output on stdout never mixes with logs.
- **Common clap fragments:** the shared `--console` / `--format` / `--lang` /
  `--diagnostics-*` flags and their `GMEOW_*` env vars, so both binaries expose an
  identical output-control surface.

Two bin crates depend on this core: `crates/gmeow-cli` (consumer) and
`crates/gmeow-dev-cli` (developer). Because the consumer binary does **not** depend on
the repo-maintenance code, the wheel-only razor is enforced by the dependency graph,
not by convention.

## 5. Surface (a) — `gmeow` (consumer)

- **Crate:** `crates/gmeow-cli`, producing the `gmeow` binary.
- **Discipline:** wheel-only — reads the bundled `gmeow.gts` snapshot via the existing
  native fold views; never the repo, never a generator input, never a repo-local query
  tree.
- **Subcommands** (ported 1:1 from the Python consumer CLI): `version`, `info`,
  `verify`, `verify-release-bundle`, `describe`, `validate`, `build`, `project`,
  `transpile`, `export`, `convert`, `export-docs`, `docs-on`, `crossref`, `mcp` (starts the
  consumer MCP stdio server, [§7](#7-surface-c--mcp-interface)), `gts` (thin shim to
  the external `gts` binary), and the extension dispatch (`music`, …) via the
  [`cli-extensions.md`](./cli-extensions.md) `gmeow:providesSubcommand` discovery model.
- **Output:** the shared `Reporter`. `describe` prints prose to a human at a TTY and a
  structured `describe` frame under `jsonl`.

### 5.1 Consumer parity (Python → Rust)

The clap subcommand set is the command set of the former Python consumer CLI,
preserving names, exit codes, `GTS_SNAPSHOT_FILE` injection, and `--lang` selection.
`trycmd` snapshots pin the human surface; `assert_cmd` pins exit codes and the `jsonl`
frames. This **retired** the Python consumer CLI-surface tests (the `gmeow` CLI, the
feedback seam, and the public wheel-path `gmeow validate`).

## 6. Surface (b) — `gmeow-dev` (developer, ultra-rich reporting)

- **Crate:** `crates/gmeow-dev-cli`, producing the `gmeow-dev` binary; may read the
  tree.
- **Nested sub-apps:** `box-roles` (`audit`), `logic` (`query`, `compile`), `i18n`
  (`extract`, `sync-english`, `merge`, `export-csv`, `export-xliff`) — modelled with
  clap's nested subcommands.
- **The heavy path:** `sync` and `fanout` call the existing `crates/pipeline`
  `run_full` **directly in Rust** — no PyO3 hop — and stream the `RunReport` through
  the `Reporter`. This is the *ultra-rich status* surface:
  - **Developers** get live `indicatif` `MultiProgress`: one bar per parallel
    topological level, updated from `StageTiming` / `LevelTiming`.
  - **Agents** get NDJSON `stage_start` / `stage_end{elapsed_ms}` / `finding` /
    `summary` frames — the same `TimingRecord` / `Finding` stream, line-framed. A tool
    run thousands of times a session parses one stable schema; the deterministic
    `--timings-json` artifact remains available for profiling.
  - Both come off **one** event source; there is no second reporting path to drift.
- **Diagnostics:** the shared `--diagnostics-console` / `--diagnostics-artifacts` /
  `--diagnostics-dir` flags and `GMEOW_DIAGNOSTICS_*` env, backed directly by
  `gmeow_diagnostics` render functions (no Python re-serialization).

This **retired** the developer CLI-surface and thin PyO3-seam tests — the `logic` CLI,
the logic engine/compile-diagnostics seams, the slice-dependency check, and the
external-tool seam — as their Python consumers were deleted.

## 7. Surface (c) — MCP interface

The MCP server is native Rust (`crates/pipeline/src/mcp.rs`): a **hand-rolled
JSON-RPC 2.0** stdio server (protocol version `2024-11-05`), synchronous — one request
per line on stdin, one response per line on stdout — with **no async runtime** (no
tokio) on the path, so startup stays cheap. `McpServer` owns the stdio loop, startup
language validation, resource routing, and the grounded-memory triad.

- **Native launch.** The server is launched by the Rust CLI — `gmeow mcp` (consumer,
  `McpMode::Consumer`, snapshot-only) and `gmeow-dev mcp` (developer, `McpMode::Dev`,
  repo-anchored) — not by a Python shim. The `#[cfg(feature = "python")]` blocks in
  `mcp.rs` remain only as PyO3 *binding* wrappers, not the launch path.
- **Preserved memory / process semantics:**
  1. the Transaction-Logic memory triad `store_claim` / `recall` / `revise_belief`;
  2. `dry_run` → the hypothetical/sandbox operator (verdict computed, effect
     discarded, strict boolean parsing — no coercion);
  3. compensation-as-rollback (`revise_belief` is `store_claim`'s compensation,
     P10 suppression-not-erasure);
  4. `ToolCall` provenance + audit-segment recording on every committed write;
  5. the consumer-vs-dev tool gating (`validate` / `reason` / `sync` /
     `constitution` are dev-only) via `McpMode`;
  6. snapshot-bound reads for the bundled ontology surfaces (`lookup_term`,
     `llms_txt`, `llms_full`, `doc_card`, `okf_index`).
- **Local-ontology overlay.** Beyond the snapshot, the server answers reads over
  `bundle ∪ overlay`, where `overlay` is a read-only local lower-tier vocab/graph file
  the agent supplies — so an agent can reason about *its own* graphs alongside
  `gmeow.gts` without polluting the canon.

## 8. How the port landed

The port landed as a **single atomic cutover** across all three surfaces, executed
internally as replace-and-delete: each surface's Python predecessor was deleted in the
same change that introduced its Rust replacement, so the tree never carried a parallel
Python + Rust CLI. That one change deleted each retired pytest **and** its
`docs/test-retention/` dossier **and** redirected any `governance/constitution.ttl`
`meta:artifact` citation of a deleted test to the Rust artifact that now proves the
principle (a dossier must not outlive its test). The Python `gmeow` / `gmeow-dev`
console entry points were re-pointed to the Rust binaries and the PyO3 CLI seam retired
in one step — no interim state where both surfaces coexisted.

## 9. Resolved decisions

1. **Execution shape:** a single atomic cutover covering all three surfaces (§8),
   executed as replace-and-delete so the tree never carried a parallel Python + Rust
   CLI.
2. **MCP transport:** the in-repo hand-rolled JSON-RPC 2.0 server was **retained**
   (not migrated onto an external SDK), preserving the Transaction-Logic memory
   semantics verbatim (§7) and keeping the surface async-runtime-free.

## 10. Non-goals

- No parallel Python + Rust CLI at any point (greenfield).
- No external MCP SDK / rmcp — the in-repo hand-rolled JSON-RPC 2.0 server is canonical.
- No OpenTelemetry / optional telemetry exporters — an env-activated-else-no-op sink is
  runtime optionality, which the constitution forbids.
- No full-screen TUI (`ratatui`). <!-- codespell:ignore ratatui -->
- No second diagnostics model (`miette`) — `gmeow_diagnostics` is canonical.
- No change to the PIPELINE_SPINE laws, the `gmeow` / `gmeow-dev` razor, or the
  `cli-extensions.md` subcommand-discovery contract.
