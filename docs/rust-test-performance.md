<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: CC-BY-4.0
-->

# Rust test-suite performance

Rust test wall time is optimization evidence, not a correctness gate. Neither
`make rust-test`, `make rust-gate`, nor CI fails because an individual test
crosses an elapsed-time threshold. Wall-clock timings vary with host load,
filesystem state, cache warmth, and concurrent worktrees, so a single contended
sample is not a stable policy signal.

Nextest still applies a 60-second slow-timeout with two consecutive periods before
termination. That is only a runaway or hang backstop; it is not a performance
budget. Architecture-scoped overrides extend the backstop for known exhaustive
paths without turning their elapsed time into a target.

## Scheduling

Nextest uses the CPUs available to the process. There is no fixed global or
test-group concurrency cap. Tests that start their own full-width worker pools may
use `threads-required = "num-cpus"` to prevent nested oversubscription; this
reservation scales with the machine instead of throttling a 32+ CPU development
server to a small constant.

The default profile keeps a small architectural carve-out for exhaustive
whole-repository checks whose contracts are already covered by focused tests and
dedicated drift gates. `make maint-rust-heavy` runs that complete lane. A test
must never move off the default profile merely because one wall-clock sample was
slow.

## Optimization evidence

Use evidence that identifies actual work:

- Criterion comparisons through `make bench` and `make bench-compare`.
- Allocation counts, retained bytes, rows scanned, candidate rows, and other
  deterministic cost measures.
- Repeated before/after samples of the same production path, with cache state and
  host contention recorded.
- Profiles that locate redundant parsing, indexing, serialization, joins, or I/O.
- Semantic parity tests and byte-identical generated artifacts after optimization.

JUnit timing remains available as contextual CI output, but it does not decide
whether the suite passes. Correctness failures, snapshot drift, lints, and genuine
runaway termination remain hard failures.

## Paired acceptance protocol

A repository-wide optimization claim uses paired production-path samples; a fast
mechanism test is not closure evidence. Record five baseline and five candidate
samples when the runner population is available, and never report a conclusion from
fewer than three complete pairs. Each pair must use the same node class, command,
nextest inventory, job graph, build profile, toolchain, dependency lock, and selected
feature/output profile. Alternate baseline and candidate runs where the platform
allows it so a time-of-day load shift does not all land on one variant.

The three cache protocols answer different questions and must not be pooled:

- **Cold:** a fresh runner/job with no restored pipeline, bundle-import, nextest
  archive, fixture, or Cargo target cache. Use fresh jobs rather than deleting a live
  shared cache tree.
- **Warm:** the identical command immediately after a successful fixed-point run,
  with identical source and selected DAG. A cache hit must carry its verified receipt;
  absence recomputes and corruption fails.
- **Partial:** a committed, named canonical-source edit applied equivalently to the
  baseline and candidate histories, starting from each variant's matching warm fixed
  point. Record the changed path and digest. This measures invalidation precision,
  not a dirty-worktree shortcut.

Use the median critical path for the headline comparison and retain every raw sample.
For issue-driven 2x work, acceptance means the candidate median is at most half the
baseline median on the specified slow-node population, with the same deterministic
work/inventory and all correctness evidence green. CPU, peak RSS, faults, filesystem
block I/O, cache transfer, and host load explain the result but are not silently
substituted for critical-path time. An outlier may be excluded only for a recorded
external event; exclude and replace both members of that pair.

`make perf-sample` records one schema-versioned sample and refuses a dirty worktree.
It identifies the commit/tree, resolved external dependency set, Cargo.lock, pipeline
build fingerprint, Rust/Cargo/nextest versions, runner image, host, explicit node class,
pair/index, exact command, generated-tree identity, named immutable receipts, and a
separate census for the Cargo, sync-manifest, pipeline, and fixture cache classes. A
partial sample additionally requires `KIND:PATH:SHA256`, so "partial" cannot hide an
unspecified dirty-tree perturbation. Observations carry queue/wall time, aggregate and
host-normalized CPU utilization, RSS, faults, block I/O, context switches, load averages,
and optional command telemetry. For example:

```bash
make perf-sample PERF_SAMPLE_ARGS="\
  --pair-id issue-1700-warm --variant candidate --sample-index 1 \
  --cache-state warm --node-class gh-ubuntu-x64-16 \
  --cargo-cache-state warm --sync-cache-state warm \
  --pipeline-cache-state warm --fixture-cache-state warm \
  --output dist/perf/candidate-warm-1.json \
  --work-telemetry dist/perf/gate-timings.json \
  --identity-receipt nextest=dist/nextest/receipt.json \
  --cache-root pipeline=.cache/gmeow-sync/pipeline \
  --cache-root fixture=.cache/docs-fixture \
  -- make perf-gate"
```

`make perf-accept` is the report-only outcome grader. It pairs samples by node class,
cache protocol, pair ID, and one-based index; rejects missing or duplicate variants;
requires three to five complete pairs for cold, warm, and partial protocols on both node
classes; and verifies the declared semantic/inventory JSON pointers before comparing
time. Callers must name at least one causal-work counter rather than letting the tool
guess which semantic row or output-size count should fall. The emitted JSON retains every
pair and median. Acceptance requires the slow-node headline median to reach 2.0x, the
comparison-node median to stay within 5%, no declared work counter to rise, and at least
one to fall. This tool is intentionally not a `make check` prerequisite.

For hosted critical-path samples, `scripts/ci-run-receipt.sh` reads one completed,
successful Actions run through the GitHub CLI. It binds the exact head/workflow digest,
extracts the authored `needs` graph, groups matrix instances by job, computes the longest
dependency path from actual execution durations, and keeps workflow queue/wall time,
runner identities, step timings, and artifact transfer sizes as observations. It refuses
unknown job names, multiline/unknown dependencies, incomplete jobs, failed runs, or an
API response that exceeds its explicit pagination bound. The raw run URL and every job
and artifact row remain in the receipt; the calculated critical path is never reconstructed
from a hand-written diagram.

The embedded command telemetry keeps deterministic work counters separate from
observations. `sync` schema v2 carries the exact immutable receipt for every stage,
per-entity rows/bytes, executed-versus-hydrated stages, transfer bytes, internal
phase work, and scheduler-level critical path. `validate`, `reason`, `verify`, and
`reason-verify` similarly record input rows, GTS imports, closure constructions,
query/inference counts, and budget use. `perf-gate` merges those records into a
versioned envelope without flattening the two evidence classes.

## Reuse architecture

Reuse follows the production DAG; tests do not maintain a shadow producer.

- A stage is persistently reusable only when the bound RDF DAG and its Rust
  implementation both declare `StablePrefix` plus `Persistent`. Its typed action key
  covers build/tool identity, implementation version, exact upstream entities, raw
  inputs, codec, and the explicit dimensions the action consumes (such as language or
  output profile). Unstable or recompute-declared stages always execute.
- Each admitted result has an atomic immutable receipt and a content-addressed product
  blob. Receipts census graphs, blob representations, logical artifacts, typed handles,
  rows, decoded bytes, and the semantic product digest. Reads are bounded; missing
  entries recompute, while malformed, oversized, truncated, structurally incomplete,
  or same-key-different bytes fail closed. Quota GC deletes only blobs unreachable from
  retained receipts and cannot race an active reader.
- Expensive test fixtures call the real partial-DAG scheduler and use the same action
  key/cache. A blocking per-action OS election lets exactly one process build; every
  waiter validates and hydrates that product. The CI primer only elects those actions
  early—it does not generate a second fixture format.
- Snapshot consumers share a graph-preserving, content-keyed GTS-to-indexed-dataset
  import. Raw frame/profile audits still inspect the original GTS bytes independently.
- CI builds one pinned nextest archive and inventory receipt, then runs exact disjoint
  `slice:m/n` partitions from that archive. The archive ships authenticated report-only
  samplers; each shard records archive replay CPU/RSS/I/O and turns its JUnit XML into a
  canonical identity digest plus separate duration observations. Static Rust siblings
  run independently, and breadth-only heavy DAG nodes stay explicit. Producer artifacts
  move only with their source/build/manifest receipt.

These mechanisms reduce duplicate parsing, indexing, fixture construction, compilation,
serialization, and archive work. They do not remove tests, weaken selected outputs, or
turn an elapsed-time threshold into a correctness policy.
