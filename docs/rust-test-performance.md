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

The required profile does not retry failures. A retry previously let one
contended whole-bundle test perform the same dominant work three times and hid
the first causal failure. Full-width consumers instead reserve `num-cpus`, and a
failure remains a single terminal result.

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

Focused execution filters the already-built workspace inventory rather than asking
Cargo for a package-scoped graph. Use, for example, `make nextest
NEXTEST_FILTER='package(gmeow-dev-cli) & binary(make_gate_contract)'`. This retains
the same feature resolution and binaries as the authenticated archive while nextest
skips everything outside the expression. The test-fixture producer remains a
separate prerequisite; a filter never primes, regenerates, or repairs corpus state.

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
required logical proof inventory, build profile, toolchain, dependency lock, and selected
feature/output profile. The authored CI dependency graph is retained in every receipt;
scheduler optimization may change that graph, but the candidate proof inventory must be
a superset of the baseline inventory. Alternate baseline and candidate runs where the
platform allows it so a time-of-day load shift does not all land on one variant.

Every receipt names exactly the Cargo, sync-manifest, pipeline, fixture, bundle-import,
and nextest-archive cache classes. A class that does not exist on one history is recorded
as `absent` or `not-applicable`; it is never relabelled as warm or cold to make the pair
look symmetric. Shared applicable mechanisms must have identical states within a pair.
This permits an optimization to introduce a cache while keeping the comparison honest,
without allowing a cold baseline to be paired with a warm candidate for a cache both
histories possess. Each cold, warm, or partial receipt must contain at least one class
that witnesses that aggregate protocol.

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

The partial protocol is predeclared, rather than chosen after seeing a fast result:

| Witness | Exact authority | Expected effect |
|---|---|---|
| End-to-end paired sample | On a retained measurement branch for each variant, add exactly `# perf-protocol: raw-source-only-change-v1` after the SPDX header of `slices/core/contacts/module.ttl`, commit it, and record that file's SHA-256 as `raw-source:<path>:<digest>` | `stage-source-load` observes a changed raw-file key; its semantic product stays equal, so only consumers whose declared input identity changed may execute |
| Irrelevant versus consumed entity | `artifact_level_invalidation_reruns_only_the_changed_graphs_consumer` | An entity consumer reruns only for its declared graph; an unrelated graph change remains reusable |
| Raw-file invalidation | `input_files_content_busts_the_cache` | Changing declared raw bytes always changes the owning action key |
| Code, dependency, toolchain, target, features, and profile | `stage_key_is_deterministic_and_structurally_sensitive`, the transitive build-fingerprint tests, and bundle-import build-input tests | Every executable identity input changes the key; feature ordering alone is canonicalized |
| Language | `language_is_a_first_class_render_action_dimension` | Only the selected docs render action changes; another language cannot share its receipt |
| Output scope/profile | `RunOutputScope` remains outside stage keys while it controls only reconciliation; render artifact/profile identity is part of the consuming action name/codec | A selector invalidates at its actual consumer and never creates undeclared stage-key noise |

The ontology comment is intentionally semantic-neutral but not byte-neutral: it measures
raw-source invalidation without changing the required output inventory. The other rows
are the complete focused invalidation matrix and run on both histories; they are not
substitutes for the end-to-end partial sample.

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
separate census for the Cargo, sync-manifest, pipeline, fixture, bundle-import, and
nextest-archive cache classes. A
partial sample additionally requires `KIND:PATH:SHA256`, so "partial" cannot hide an
unspecified dirty-tree perturbation. Observations carry queue/wall time, aggregate and
host-normalized CPU utilization, RSS, faults, block I/O, context switches, load averages,
and optional command telemetry. For example:

```bash
make perf-sample PERF_SAMPLE_ARGS="\
  --pair-id required-lane-warm --variant candidate --sample-index 1 \
  --cache-state warm --node-class gh-ubuntu-x64-16 \
  --cargo-cache-state warm --sync-cache-state warm \
  --pipeline-cache-state warm --fixture-cache-state warm \
  --bundle-import-cache-state warm --nextest-archive-cache-state warm \
  --output dist/perf/candidate-warm-1.json \
  --work-telemetry dist/perf/gate-timings.json \
  --identity-receipt nextest=dist/nextest/receipt.json \
  --cache-root actions=.cache/gmeow-sync/actions \
  -- make perf-gate"
```

For a cold baseline whose history predates the producer-receipt envelope, pass its
retained exact sync manifest as `--identity-receipt producer=PATH`. The sampler accepts
the manifest's recorded build fingerprint without placing it at the live
`.cache/gmeow-sync/manifests/` path, so identity evidence cannot accidentally turn the
cold sample into a manifest hit. When the measured command is `perf-gate`, the sampler
also normalizes `pipeline_stage_executions`: current telemetry supplies the explicit
executed-stage count, while legacy telemetry contributes exactly its unique top-level
`stage:stage-*` execution rows. Nested phase timings and elapsed time never enter this
causal counter.

`make perf-accept` is the report-only outcome grader. It pairs samples by node class,
cache protocol, pair ID, and one-based index; rejects missing or duplicate variants;
requires three to five complete pairs for cold, warm, and partial protocols on both node
classes; and verifies the declared semantic/inventory JSON pointers before comparing
time. Equality identities use `--semantic-identity`; proof inventories use
`--proof-inventory` and permit additions while hard-failing a baseline member dropped by
the candidate. Callers must name at least one causal-work counter rather than letting the
tool guess which semantic row or output-size count should fall. The emitted JSON retains
every pair and median. Acceptance requires the slow-node headline median to reach 2.0x,
the comparison-node median to stay within 5%, no declared work counter to rise, and at
least one to fall. This tool is intentionally not a `make check` prerequisite.

For hosted critical-path samples, `scripts/ci-run-receipt.sh` reads one completed,
successful Actions run through the GitHub CLI. It binds the exact head/workflow digest,
extracts the authored `needs` graph, groups matrix instances by job, computes the longest
dependency path from actual execution durations, and keeps workflow queue/wall time,
runner identities, step timings, and artifact transfer sizes as observations. The
collector also authenticates the exact resolved Rust identity and hosted image from the
producer-build log, normalizes matrix instances into a logical proof inventory, and
counts required job groups that would fall back to rebuilding the source producer. Its
schema-v3 output additionally downloads only the compact, authenticated generation,
fixture, archive, reasoning, and JUnit receipts; it aggregates actual stage executions
and hydrations, cache bytes, fixture builders, imports, closures, indexed RDF rows,
archive bytes, Cargo compilation units, and test-build authorities while preserving every
source receipt below the aggregate. The large generated tree and nextest archive are
never downloaded by the collector because their immutable receipts already bind them.
The output is consumed directly by `make perf-accept` alongside local
`perf-sample` receipts. It refuses
unknown job names, multiline/unknown dependencies, incomplete jobs, failed runs, or an
API response that exceeds its explicit pagination bound. The raw run URL and every job
and artifact row remain in the receipt; the calculated critical path is never reconstructed
from a hand-written diagram.

```bash
make perf-ci-receipt CI_RUN_RECEIPT_ARGS="\
  --run-id 123456 --variant candidate --sample-index 1 \
  --node-class github-ubuntu-latest-x64 --pair-id required-lane-cold \
  --cache-state cold --cargo-cache-state cold --sync-cache-state cold \
  --pipeline-cache-state cold --fixture-cache-state cold \
  --bundle-import-cache-state cold --nextest-archive-cache-state cold \
  --output dist/perf/ci-candidate-cold-1.json"
```

The embedded command telemetry keeps deterministic work counters separate from
observations. `sync` schema v2 carries the exact immutable receipt for every stage,
per-entity rows/bytes, executed-versus-hydrated stages, transfer bytes, internal
phase work, and scheduler-level critical path. A cache hit also records its signed
process-RSS delta. Run the admission census at `--jobs 1` to isolate each hydrated
stage; parallel-wave deltas remain observations and can include sibling allocation, so
they never enter an immutable receipt or deterministic counter. `validate`, `reason`, `verify`, and
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
- An explicit pre-test producer runs the selected production DAG once and publishes an
  immutable manifest of exact action receipts. Test processes receive that manifest's
  SHA-256 and load only its named products read-only; an absent, stale, corrupt, or
  wrong-identity product is a hard failure and never causes a dependency walk or build.
- Snapshot consumers share a graph-preserving, content-keyed GTS-to-indexed-dataset
  import. Raw frame/profile audits still inspect the original GTS bytes independently.
- CI builds one pinned nextest archive and inventory receipt, then runs exact disjoint
  `slice:m/n` partitions from that archive. The archive ships authenticated report-only
  samplers; each shard records archive replay CPU/RSS/I/O and turns its JUnit XML into a
  canonical identity digest plus separate duration observations. Static Rust siblings
  run independently, and breadth-only heavy DAG nodes stay explicit. Producer artifacts
  move only with their source/build/manifest receipt.

### Share expensive consumer state across contracts

Nextest's unit of process isolation is a libtest identity. An in-process `OnceLock`
therefore cannot share a restored bundle with a second ordinary test: each process pays
the import, indexes, parsed shape union, and documentation projections again. A
corpus-backed module with many independent identities must expose one required runner
and, where applicable, one maintained exhaustive runner. The runner sorts the registered
contract names, catches and reports each panic independently, and emits per-contract
timings; assertion identity and failure attribution survive while immutable setup is paid
once.

The native MCP module follows that composition. Its resident view additionally caches the
bundle-derived fixture/entailment joins, GMN dictionary, and slice-quality standard. On the
same authenticated selector and host, the required MCP contracts fell from 421.655 s to
188.785 s (55.2%) while 113 required contracts remained. The focused verifier contracts
stay required; the one whole-bundle overlay proof, whose cost is set by exhaustive corpus
breadth, remains registered in the maintained runner. This is a same-host hot-cache
measurement of that module, not a substitute for the hosted critical-path receipts.

These mechanisms reduce duplicate parsing, indexing, fixture construction, compilation,
serialization, and archive work. They do not remove tests, weaken selected outputs, or
turn an elapsed-time threshold into a correctness policy.
