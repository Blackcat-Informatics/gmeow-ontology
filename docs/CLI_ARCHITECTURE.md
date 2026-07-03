<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# CLI & MCP Architecture — porting `gmeow` / `gmeow-dev` to Rust

> **Status:** specification (design locked 2026-07-03). Blueprint for the CLI→Rust
> port epic. This document is the crate-selection decision record and the
> three-surface architecture; the code port and Python-test retirement happen in
> the follow-on phases enumerated in [§8](#8-migration-epic).
>
> **Genre.** A normative architecture spec for the two user-facing command-line
> surfaces (`gmeow`, `gmeow-dev`) and the MCP interface. Peer to
> [`PIPELINE_SPINE.md`](./PIPELINE_SPINE.md) (the regeneration spine this CLI
> drives) and [`cli-extensions.md`](./cli-extensions.md) (the subcommand-discovery
> model this CLI must respect). Governed by [`CONSTITUTION.md`](../CONSTITUTION.md)
> and [`AGENTS.md`](../AGENTS.md).

## 0. Why this exists

The `gmeow` (consumer) and `gmeow-dev` (ontology-developer) command surfaces are
today **Python/[Typer](https://typer.tiangolo.com/) + [Rich](https://rich.readthedocs.io/)**
apps (`src/gmeow_tools/cli.py`, `src/gmeow_tools/cli_dev.py`) that call the Rust
core through the unified `gmeow_native` PyO3 cdylib. Their behavior is verified
only via Python `CliRunner`/subprocess tests, which cannot be retired until the CLI
itself is Rust. Porting the command surface to Rust (rust-first, greenfield) both
retires those tests and unlocks a **next-generation command-line experience**: one
output engine that serves a *human* developer at a TTY and an *agent* running the
tool thousands of times a session, off the same structured event stream.

Three things make this port smaller and safer than it first appears — and they
shape every decision below:

1. **The reporting backbone is already Rust and canonical.** `crates/diagnostics`
   (`gmeow_diagnostics`) is a serde model that already renders to JSON, SARIF, RDF,
   HTML, text, and **NDJSON**, with an eight-way `FindingCategory` taxonomy, GTS
   wire-coordinate `Location`s, and slice attributions. The pipeline emits a
   structured `RunReport` (`crates/pipeline/src/run.rs`) with per-stage/per-level
   `TimingRecord` critical-path timings. The port is mostly about moving the
   *presentation seam* from Python/Rich onto this existing Rust model — not building
   a diagnostics engine.
2. **The MCP layer is already Rust** (`crates/pipeline/src/mcp.rs`) — a hand-rolled
   JSON-RPC 2.0 server with sophisticated Transaction-Logic memory semantics already
   in place. Surface (c) is a *modernization*, not a from-scratch build.
3. **Greenfield forbids a stub.** [`CONSTITUTION.md`](../CONSTITUTION.md) §6 requires
   removing an inferior element rather than retaining it for compatibility. A
   half-parity Rust CLI living beside the Python one would *be* that violation, so
   the port proceeds **surface by surface as replace-and-delete**, never as a
   parallel implementation.

## 1. Constraints that bind the design

- **Greenfield / no backwards-compat** ([`CONSTITUTION.md`](../CONSTITUTION.md) §6):
  each surface is replaced and its Python predecessor deleted in the same change.
- **Rust-first, no new Python** ([`AGENTS.md`](../AGENTS.md)): the CLIs become Rust
  binaries; the only Python touched is the code being *deleted*.
- **No optionality / hard-fail**: output modes are a closed enum; a missing required
  input stops and reports. No degraded fallbacks, no optional-dep feature gates that
  change core behavior.
- **The `gmeow` vs `gmeow-dev` razor** ([`AGENTS.md`](../AGENTS.md) §CLI): `gmeow`
  must work **from the installed wheel alone** — the bundled `gmeow.gts` snapshot,
  no source checkout, no repo-local query trees. `gmeow-dev` is repository
  maintenance and may read anything in the tree. This razor is enforced
  *structurally* here (see [§4](#4-shared-core--cratescli-core-gmeow-cli-core)).
- **Gate-latency budget culture** ([`docs/rust-test-budget.md`](./rust-test-budget.md)):
  the tool is run thousands of times a session, so startup latency and per-invocation
  cost are first-class. Heavy dependencies (async runtimes, telemetry exporters) stay
  off the hot path.
- **PIPELINE_SPINE laws** ([`PIPELINE_SPINE.md`](./PIPELINE_SPINE.md)): the CLI
  *drives* the spine but does not violate it — one terminal, superset law, fanout is
  pure projection.
- **Subcommand-discovery model** ([`cli-extensions.md`](./cli-extensions.md)):
  extension subcommands are discovered via `gmeow:providesSubcommand` manifests /
  entry points; the Rust `gmeow` dispatcher must keep that seam intact.

## 2. Crate decision record

Versions are the latest stable on crates.io as of 2026-07-03. Pin these at
implementation time; bump deliberately, never float.

| Concern | Crate | Version | Rationale |
|---|---|---|---|
| Argument parsing | **clap** (derive API) | 4.6.1 | The de-facto standard; the derive API is type-safe and its nested-subcommand support cleanly models `gmeow-dev`'s `box-roles` / `logic` / `i18n` sub-apps. |
| Completions & man pages | **clap_complete**, **clap_mangen** | track clap | bash/zsh/fish/nu completions and man pages — real DX wins for a heavily-run tool. |
| Colour / styling | **anstyle** + **anstream** | clap-native | `NO_COLOR`-aware, terminal- and Windows-aware; already the transitive styling layer clap uses, so no additional colour crate is pulled in. |
| Live progress | **indicatif** | 0.18.6 | `MultiProgress` bars keyed to the scheduler's parallel topological *levels*; TTY-gated so agents never receive progress bytes. |
| Tables | **comfy-table** | 7.2.2 | Replaces Rich `Table` for `info` / `coverage` / `audit` tabular views. |
| Diagnostics model | **`gmeow_diagnostics`** (reuse) | in-repo | Already the canonical serde model with JSON/SARIF/RDF/HTML/text/NDJSON renderers, category taxonomy, and GTS-coordinate locations. Reused, not re-invented. |
| Internal error type | **thiserror** | 1.x | Already the house style; a typed CLI error enum maps to stable exit codes. |
| Logging / telemetry | **tracing** + **tracing-subscriber** | 0.1.44 / 0.3.23 | Instrument-once / many-sinks; **stderr-bound**; `EnvFilter` via `GMEOW_LOG` / `RUST_LOG`; near-zero cost when a level is disabled (hot-path safe). Mandatory for the stdio MCP server (see [§3](#3-two-output-channels)). |
| MCP protocol | **rmcp** (official SDK) | 2.1.0 | The official `modelcontextprotocol/rust-sdk`; `#[tool_router]` / `#[tool]` macros, schemars-generated tool schemas, resources/prompts, current protocol version, and a path to HTTP/SSE. Replaces the hand-rolled JSON-RPC. |
| Async runtime (MCP only) | **tokio** | 1.x | rmcp's stdio transport needs it; kept off the synchronous `gmeow describe` hot path for fast startup. |
| CLI testing | **assert_cmd** + **trycmd** / **snapbox** + **predicates** | latest | Exactly the parity-coverage toolset the issue names; `trycmd` snapshots the human surface, `assert_cmd`/`predicates` assert exit codes and machine output. |

### 2.1 Deliberate rejections (recorded so they are not re-litigated)

- **ratatui** (0.30.2) — *not* for v1. Agents do not consume a full-screen TUI, and <!-- codespell:ignore ratatui -->
  developers are well served by `indicatif` + `comfy-table`; a TUI would fight the
  NDJSON contract and inflate startup. Documented as a *future* option for an
  interactive `gmeow-dev` dashboard only.
- **miette** — *not* adopted as the diagnostics model. `gmeow_diagnostics` is the
  canonical model with SARIF/RDF projections; a second diagnostic model would
  violate no-optionality. (miette could be revisited *solely* to prettify internal
  Rust panics, but it is not required and is not part of v1.)
- **A busybox-style single multi-call binary** — rejected in favour of two bin
  crates sharing a `gmeow-cli-core` library, so the wheel-only razor is
  *structurally* enforceable: the `gmeow` binary never links repo-maintenance code.
- **OpenTelemetry / OTLP export** (`tracing-opentelemetry`) — *not* pre-committed.
  A telemetry exporter is a documented future extension, held back so a build-time
  observability add-on does not become a runtime degraded-fallback and trip the
  no-optionality doctrine. The baseline `tracing` stderr subscriber ships regardless.
  Carried as an open decision in [§9](#9-open-decisions).

## 3. Two output channels

Every surface obeys one rule: **stdout is product output; stderr is diagnostics.**
Conflating them is the classic CLI bug and, for the MCP server, an outright protocol
corruption.

- **stdout = product output** — the `Reporter` (results, findings, NDJSON event
  stream, tables, progress). This is a **stable, versioned contract** with the
  caller. Where determinism is required — notably the pipeline `RunReport` and
  `--timings-json` — the payload is product *data*, not a log, and is **never**
  routed through the logging layer (whose field ordering is non-deterministic).
- **stderr = diagnostics / telemetry** — `tracing` spans and events (stage timings,
  MCP request-correlation ids, debug/trace), filtered by `GMEOW_LOG` / `RUST_LOG`,
  default quiet (`warn`) so `gmeow describe` stays silent and fast unless asked.

`tracing`'s layer model is the diagnostic-side analogue of the `Reporter`:
instrument once, then attach a human `fmt` layer or a JSON layer as the mode
dictates. The stdio-MCP rule — *never write to stdout* — is satisfied
**structurally**, because all MCP logging flows through the stderr `tracing`
subscriber. The existing opt-in `GMEOW_PIPELINE_TIMING` stderr dump is reframed as
ordinary `tracing` spans without disturbing the deterministic `--timings-json`
artifact.

## 4. Shared core — `crates/cli-core` (`gmeow-cli-core`)

A library crate owning everything both binaries and the MCP server share:

- **`ConsoleMode`** — a closed enum `auto | pretty | text | jsonl | silent`,
  mirroring the *existing* Python vocabulary (`src/gmeow_tools/diagnostics_config.py`)
  so behavior is continuous across the port. Resolution precedence is
  **flag > env > default**, matching the Python resolver.
- **`Reporter`** trait with two implementations:
  - `HumanReporter` — `anstyle` colour + `indicatif::MultiProgress` (one bar per
    parallel pipeline *level*, driven by `scheduler.rs` `StageTiming` / `LevelTiming`)
    plus `comfy-table` summaries.
  - `NdjsonReporter` — one JSON object per line to stdout with a **stable event
    schema** (`stage_start` / `stage_end{elapsed_ms}` / `finding` / `summary`),
    line-framing the canonical `gmeow_diagnostics` findings (framed, never
    re-serialized in a second model).
- **DX upgrade over Python:** `auto` resolves non-TTY → **`jsonl`** so agents get
  machine output by default (Python's `auto` falls to `text`). Human `auto` at a
  TTY → `pretty`.
- **Exit codes:** stable, sysexits-style (`0` ok, `2` usage, `64` data error,
  `65` internal, `74` I/O), mapped deterministically from `Severity`.
- **One-line `tracing` init:** a stderr subscriber with a `GMEOW_LOG` / `RUST_LOG`
  `EnvFilter` (default `warn`). Every binary and the MCP server call it once at
  startup; product output on stdout never mixes with logs.
- **Common clap fragments:** the shared `--console` / `--format` / `--lang` /
  `--diagnostics-*` flags and their `GMEOW_*` env vars, so both binaries expose an
  identical output-control surface.

Two bin crates depend on this core: `crates/gmeow-cli` (consumer) and
`crates/gmeow-dev-cli` (developer). Because the consumer binary does **not** depend
on the repo-maintenance code, the wheel-only razor is enforced by the dependency
graph, not by convention.

## 5. Surface (a) — `gmeow` (consumer)

- **Crate:** `crates/gmeow-cli`, producing the `gmeow` binary.
- **Discipline:** wheel-only — reads the bundled `gmeow.gts` snapshot via the
  existing native fold views; never the repo, never a generator input, never a
  repo-local query tree.
- **Subcommands** (ported 1:1 from `src/gmeow_tools/cli.py`): `version`, `info`,
  `verify`, `verify-release-bundle`, `describe`, `validate`, `build`, `project`,
  `transpile`, `export`, `convert`, `extract-docs`, `crossref`, `mcp` (starts the
  consumer MCP stdio server, [§8](#8-migration-epic) Phase C), `gts` (thin shim to
  the external `gts` binary), and the extension dispatch (`music`, …) via the
  [`cli-extensions.md`](./cli-extensions.md) `gmeow:providesSubcommand` discovery
  model.
- **Output:** the shared `Reporter`. `describe` prints prose to a human at a TTY and
  a structured `describe` frame under `jsonl`.

### 5.1 Consumer parity table (Python → Rust)

The clap subcommand set is the `@app.command` set of `cli.py`, preserving names,
exit codes, `GTS_SNAPSHOT_FILE` injection, and `--lang` selection. `trycmd`
snapshots pin the human surface; `assert_cmd` pins exit codes and the `jsonl`
frames. This retires `tests/test_cli.py`, `tests/test_cli_feedback.py`,
`tests/test_validate_rdf.py` (the public wheel-path `gmeow validate`).

## 6. Surface (b) — `gmeow-dev` (developer, ultra-rich reporting)

- **Crate:** `crates/gmeow-dev-cli`, producing the `gmeow-dev` binary; may read the
  tree.
- **Nested sub-apps:** `box-roles` (`audit`), `logic` (`query`, `compile`), `i18n`
  (`extract`, `sync-english`, `merge`, `export-csv`, `export-xliff`) — modelled with
  clap's nested subcommands.
- **The heavy path:** `regenerate` and `fanout` call the existing `crates/pipeline`
  `run_full` **directly in Rust** — no PyO3 hop — and stream the `RunReport` through
  the `Reporter`. This is the *ultra-rich status* surface:
  - **Developers** get live `indicatif` `MultiProgress`: one bar per parallel
    topological level, updated from `StageTiming` / `LevelTiming`, with the
    critical-stage-per-level highlighted (the critical-path floor).
  - **Agents** get NDJSON `stage_start` / `stage_end{elapsed_ms}` / `finding` /
    `summary` frames — the same `TimingRecord` / `Finding` stream, line-framed. A
    tool run thousands of times a session parses one stable schema; the deterministic
    `--timings-json` artifact remains available for profiling.
  - Both come off **one** event source; there is no second reporting path to drift.
- **Diagnostics:** the shared `--diagnostics-console` / `--diagnostics-artifacts` /
  `--diagnostics-dir` flags and `GMEOW_DIAGNOSTICS_*` env, backed directly by
  `gmeow_diagnostics` render functions (no Python re-serialization).

This retires the CLI-surface and thin PyO3-seam tests the issue enumerates —
`tests/test_logic_cli.py`, `tests/test_logic_engine.py`,
`tests/test_logic_compile_diagnostics.py`, `tests/test_slice_fix_deps.py`,
`tests/test_external_tool.py` — as their Python consumers are deleted.

## 7. Surface (c) — MCP interface

The MCP server is already Rust (`crates/pipeline/src/mcp.rs`) but hand-rolls
JSON-RPC 2.0, pins protocol `2024-11-05`, is gated behind the `python` cargo
feature, and is launched by thin Python shims. The port:

- **Migrate to rmcp 2.1.0:** express tools with `#[tool_router]` / `#[tool]`,
  request structs deriving `schemars::JsonSchema`, over the stdio transport. Gains:
  current protocol version, resources/prompts via the SDK, schemars-generated tool
  schemas, and a path to HTTP/SSE.
- **Lift out of the `python` feature** and launch via a native `gmeow mcp` clap
  subcommand — the Python shims are deleted.
- **Preserve exactly** (a hard checklist for the porting PR):
  1. the Transaction-Logic memory triad `store_claim` / `recall` / `revise_belief`;
  2. `dry_run` → the hypothetical/sandbox operator (verdict computed, effect
     discarded, strict boolean parsing — no coercion);
  3. compensation-as-rollback (`revise_belief` is `store_claim`'s compensation,
     P10 suppression-not-erasure);
  4. `ToolCall` provenance + audit-segment recording on every committed write;
  5. the consumer-vs-dev tool gating (`validate` / `reason` / `regenerate` /
     `constitution` are dev-only);
  6. snapshot-bound reads for the bundled ontology surfaces.
- **Close the capability gap:** add tools/resources to load and query the user's
  **local lower-tier ontologies, vocabs, and graph files** against the bundle. The
  current server is snapshot-bound only; the next-generation agent surface must let
  an agent reason about *its own* graphs alongside `gmeow.gts`.

## 8. Migration epic

The port lands surface by surface (replace-and-delete). Each phase deletes the
named pytest **and** its `docs/test-retention/` dossier **and** redirects any
`governance/constitution.ttl` `meta:artifact` citation of a deleted test to the
Rust artifact that now proves the principle (a dossier must not outlive its test).

- **Phase A — `cli-core` + `gmeow` consumer bin.** Retires `tests/test_cli.py`,
  `tests/test_cli_feedback.py`, `tests/test_validate_rdf.py` (+ dossiers).
- **Phase B — `gmeow-dev` bin + live reporting.** Retires `tests/test_logic_cli.py`,
  `tests/test_logic_engine.py`, `tests/test_logic_compile_diagnostics.py`,
  `tests/test_slice_fix_deps.py`, `tests/test_external_tool.py` (+ dossiers).
- **Phase C — MCP → rmcp + native subcommand + local-file read tools.** Removes the
  Python MCP shims and the `gmeow_logic` / Typer PyO3 consumers once no Python
  imports them.

## 9. Open decisions

1. **Execution shape:** one large epic PR vs. surface-by-surface (Phase A/B/C) PRs.
   This document assumes surface-by-surface — safer, and it respects greenfield
   replace-and-delete.
2. **rmcp adoption vs. keeping hand-rolled JSON-RPC:** recommended is rmcp for
   protocol currency, resources/prompts, and the HTTP/SSE path; the counter-argument
   is that the hand-rolled server is small and already carries bespoke TR semantics
   that must not weaken.
3. **OpenTelemetry export:** whether to pre-commit `tracing-opentelemetry` + an OTLP
   exporter for agent-fleet observability, or keep it a documented future extension.
   Deferred here to respect no-optionality; the baseline `tracing` stderr subscriber
   ships regardless.

## 10. Non-goals

- No parallel Python + Rust CLI at any point (greenfield).
- No full-screen TUI (`ratatui`) in v1. <!-- codespell:ignore ratatui -->
- No second diagnostics model (`miette`) — `gmeow_diagnostics` is canonical.
- No change to the PIPELINE_SPINE laws, the `gmeow` / `gmeow-dev` razor, or the
  `cli-extensions.md` subcommand-discovery contract.
