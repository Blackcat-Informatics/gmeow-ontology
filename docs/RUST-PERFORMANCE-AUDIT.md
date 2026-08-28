<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Rust and native-gate performance audit

**Historical, dated record.** This audit captures a point-in-time measurement
pass (baseline `2026-07-13`, see below) taken while `make reason-gate` and
`make reason-crosscheck` — the live native-vs-`purrdf::entail` cross-check
oracle lane — still existed. A subsequent removal deleted that oracle
lane entirely (`make reason-gate`, `make reason-crosscheck`,
`gmeow-dev reason-crosscheck`/`reason-gate`, and the
`bench-entail-oracle-alloc` bench are gone); the native `logic:` reasoner is
now the single reasoning authority and `make reason-verify` is the retained
aggregate. The measurements and dispositions below are preserved as dated
history and must not be read as current commands — see the annotated
reproduction-lane list at the end of this document.

This audit records the measured optimization pass over the Rust workspace and
the composition of `make check`. It is evidence for the standing doctrine in
[`RUST-OPTIMIZATION.md`](./RUST-OPTIMIZATION.md), not a second performance
policy.

The changes preserve one canonical generated-output path (Principle 4), keep
the hard gates executable rather than advisory (Principle 7), preserve the
projection/logic separation (Principle 17), and retain reproducible release
inputs and outputs (Principle 18).

## Measurement identity

- Baseline source: `13634fcfac83d827856d2c83e62a22bec0269a62`.
- Date: 2026-07-13.
- Host: AMD Ryzen AI MAX+ 395, 32 logical CPUs, 124 GiB RAM.
- Toolchain: Rust `1.98.0-nightly` (`f28ac764c`), LLVM 22.1.7.
- Kernel: Linux 7.1.1-2-cachyos.
- RDF family: lockstep purrdf 0.5.0.
- Profile: first-party and dependency code at O3; first-party debug assertions
  and overflow checks on; debug symbols off and residual symbols stripped.
- Population: committed `generated/dist/gmeow.gts` and the repository's complete
  discovered slice set. The generated-artifact comparison covered 9,614 files.
- Host state: quiescent except for one unrelated idle filestore service. Timing
  runs recorded process-tree peak RSS, wall/user/system time, load, `perf stat`
  counters, command output, and JSON phase telemetry under a temporary directory.

The fresh-reason and cross-check baselines used three repetitions. The
whole-pipeline and duplicate-chase experiments used one cold repetition each
because each sample is multi-minute; their mechanism is additionally proved by
phase counters, execution-count tests, and byte-exact output checks. Wall time is
report-only evidence, never a correctness threshold.

## Accepted results

| Population | Baseline | Candidate | Change | Semantic proof |
|---|---:|---:|---:|---|
| `reason-crosscheck`, median of 3 | 237.540 s | 210.350 s | -11.45% | 104 worlds; 1,488 native / 281 oracle; 281 agree, 1,207 native-only, 0 oracle-only, 0 DL-gap |
| Fresh `reason-verify`, 1 run | 458.483 s | 214.966 s | -53.11% | 1,112,386 inferred axioms; zero verify errors |
| Combined `reason-gate`, 1 candidate run | — | 217.305 s | one closure instead of composing two producers | same reasoning, verify, and oracle counters as the focused commands |
| Cold artifact drift gate, 1 run | 478.283 s | 192.973 s | -59.65% | all 9,614 artifacts byte-clean |
| Cold source-load critical stage | 342.706 s | 55.747 s | -83.73% | identical assessment and generated-artifact bytes |
| Slice-quality assessment phase | 313.548 s | 27.750 s | -91.15% | indexed result fold preserves slice, diagnostic, finding, and RDF order |
| MCP full-card JSON test, standalone | 12.677 s | 6.711 s | -47.06% | identical JSON and tier fields; pointer identity pins one shared documentation projection |

The cold pipeline's process-tree peak RSS moved from 11,903,924 KiB to
13,879,616 KiB (+16.60%) while average CPU utilization moved from 108% to 485%.
That is the deliberate throughput trade: independent leave-one-out closures are
resident concurrently. The measured 13.24 GiB peak remains below the existing
16 GiB build-memory contract; lower-core CI workers schedule fewer concurrent
closures. No LTO, codegen-unit, debug-symbol, or runtime-check profile change was
used to obtain the speedup.

Fresh `reason-verify` peak RSS fell from 8,494,732 KiB to 6,424,296 KiB
(-24.37%) because the command no longer materializes two complete native results.
The three-run median cross-check peak moved from 6,363,772 KiB to 6,541,688 KiB
(+2.80%) while its independent world oracles became parallel. The combined gate's
217.305 s sample split into 1.626 s snapshot import, 203.560 s native reasoning,
7.413 s verification, and 4.171 s oracle work; peak RSS was 6,702,244 KiB.

### 2026-07-16 follow-on: exact batch proofs and verified task reuse

The earlier indexed-Rayon change still executed one complete reasoner closure per
leave-one-out candidate. The follow-on replaces that repeated work with exact
shared indexes for subclass/subproperty reachability, equivalence and finite-union
support, fixed-head absence, disjointness impossibility, and property-characteristic
load-bearingness. Ambiguous constructs retain the production incremental-session +
finite-DL path; mixed-family tests compare every batch answer with a scratch
`reason_closure_axioms` run. The redundant eager baseline closure in the
slice-quality axis was removed because its value was discarded and each selected
positive/negative proof already owns its error path.

Focused warm observations on the same checkout moved the extension/music slice
from 39.91 s to 1.24 s and the exhaustive logic-slice scoring test from 55.31 s to
1.57 s. The strict pipeline's slice-quality phase moved from 54.639 s in the first
batch implementation to 1.487 s in the final implementation (36.7×); total pipeline
telemetry moved from 300.165 s to 208.276 s. After the canonical update established
the fixed point, the read-only generated check hit the whole-run manifest in 0.80 s.
These are report-only observations taken while the shared host had variable
contention; the acceptance claim is exact scratch parity and unchanged scoring,
not any wall-clock value. The retained ignored exhaustive audit compared every
capped candidate in every real slice with parallel scratch reasoning in 65.3 s.
After integrating the newer main-branch corpus, the same slice-quality phase was
1.858 s inside a 283.735 s cold pipeline. A complete receipt-fallback gate then
passed in 493.616 s, including 6,170 default-lane Rust tests; those corpus-expanded
figures are evidence for the current checkout, not a like-for-like baseline.

The aggregate check runner has ONE execution profile: `make check` physically
executes its entire task DAG. The receipt-backed impact profile that once pruned
unaffected tasks has been removed — it bought a saving only on the docs-only edits
that were already cheap, while every Rust or ontology edit fell closed to the full
registry, and it made the gate's behaviour depend on network reachability of a
GitHub attestation. The successful-check receipt itself is retained: CI still emits
and attests one per green `main` push as a supply-chain artifact
(`cargo xtask receipt create`), it simply no longer feeds scheduling.

Wall time is now bought by scheduling instead. `sync` is a prerequisite of a task
only when that task genuinely reads a `generated/` artifact, and the monolithic
`rust-gate` node is split into three concurrent siblings (`clippy`, `nextest`,
`doctests`) under one `rust-build`. Carrier-purity and coherence-teeth proofs execute
inside the single nextest inventory/archive instead of compiling another test workspace.
The breadth-dominated lanes
(`wasm-parity`, `acceptance`, `bench-soak`, and `medium-consumer-surface`) moved
to the CI-only `make heavy`, which still runs on
every PR.

The local `make check` entry point also owns update-mode synchronization. This
removes the developer-visible regenerate-then-gate boundary without adding
another cold pipeline: a clean manifest returns immediately, while a miss pays the
same regeneration that previously had to be run as a separate command. The
internal `make check-sync` target defaults to read-only mode for CI drift proof.

On a cold full profile, `rust-build` is an explicit aggregate DAG node. Rust
consumers wait for that one owner instead of racing separate Make subprocesses
to create the same readiness stamp behind Cargo's build lock. The dedicated
`gts-frame-profile-gate` target remains independently runnable, while the
aggregate relies on the identical `gts_profile` unit tests already present in
the complete `rust-gate` nextest population.

## Ranked findings and dispositions

| Rank | Hot symbol or composition seam | Measured share / smell | Mechanism | Disposition and proof |
|---:|---|---|---|---|
| 1 | `slice_quality::reasoner_axis` leave-one-out probes | 313.548 s, 91.5% of source-load and 65.9% of the cold pipeline | Up to 64 candidates rebuilt equivalent closure state | Superseded by exact batch relation/support indexes with conservative incremental + finite-DL fallback. Mixed-family tests compare every result with scratch reasoning; authored order is retained. |
| 2 | `gmeow-dev reason-verify --fresh` | Two full native chases for one command | The first result checked consistency; verification called a second reasoning entry point | Accepted: a `FnOnce` producer returns one `ReasoningResult`, and verification consumes that exact value. A unit test pins one producer invocation. |
| 3 | Aggregate reasoning targets | Focused verify and oracle targets independently acquired equivalent native state | Make composed command surfaces instead of composing their shared product | Accepted: `reason-gate` imports once, chases once, then feeds the same result to verification and the entailment oracle. Focused targets remain runnable. |
| 4 | `entail_crosscheck::oracle_subsumptions` | 104 isolated worlds evaluated serially | Each projected named graph is immutable and independent | Accepted: indexed per-world Rayon buffers, followed by explicit stable sort/dedup. Repeated fixed-pool serial/parallel parity tests pin order and values. |
| 5 | `entail_oracle::owlrl_subsumptions` scan | Predicate rejection allocated owned IRI strings | Terms were owned before knowing whether a row survived | Accepted: compare borrowed `TermRef::Iri` values and own only output pairs. Five exact samples moved from 8,260,960 bytes / 37,145 allocations to 8,116,809 bytes / 33,030 allocations (-1.75% bytes, -11.08% count) with the same 3,212,206-byte peak-live value. |
| 6 | `McpView::run_docs_query` documentation panels | A full card re-projected the whole documentation graph for every SPARQL panel; the test reached 30.675 s under full-suite contention | Each panel copied the same immutable named graph before querying it | Accepted: one `OnceLock<Arc<RdfDataset>>` projection per server is shared by every documentation query. Exact-output tests and pointer identity pin the contract; the standalone full-card test moved from 12.677 s to 6.711 s (-47.06%) without an off-gate exception. |
| 7 | Whole-bundle coherence teeth | Clean bundle and poisoned bundle were each chased in the teeth test, after the aggregate reasoning lane already established cleanliness | The test mixed the hold proof with the mutation-sensitivity proof, and CI compiled a second workspace just to execute it | Accepted: the test owns only the poisoned-bundle witness and runs once inside the authenticated nextest archive. `reason-gate` still proves the clean hold independently, while the default test inventory and Make/CI contract tests prevent a second coherence-only build or job. |
| 8 | Aggregate linting | `lint-issue-refs` ran as an explicit prerequisite and again as an always-run pre-commit hook; workspace-wide clippy ran during ordinary commits and again in the required Rust gate | A source-hygiene hook owned a compiled whole-workspace gate, creating cold duplicate builds | Accepted: pre-commit is source-only and file-triggered; clippy remains mandatory exactly once in `make check` and runs on CI's existing Rust-static runner. No `SKIP` path is needed, and a Rust structural test pins the ownership. |
| 9 | Test fixture and focused-test build lineages | The former Rust prebuild was followed by a second four-example `cargo build`, measured at 2m15s after a 3m53s nextest no-run build. A later `cargo nextest -p gmeow-dev-cli` rebuilt a narrower feature graph for 44.64s after the workspace inventory was warm | Standalone primer/verifier examples and package-scoped focused commands each asked Cargo for a different graph | Accepted: hidden `gmeow-dev test-fixtures` producer/verify verbs reuse the binary already emitted by the one nextest build, removing 36% of the measured cold compile sequence. `NEXTEST_FILTER` applies a nextest expression while retaining `--workspace`; the same focused contract then needed 0.67–1.21s of Cargo checking and 0.01s of execution with 374 binaries skipped. |
| 10 | Stage-fixture cache lookup | Persistent test leaves can depend on nonpersistent carrier stages, so reconstructing their keys from currently persistent dependency receipts misses and invites a second DAG traversal | The producer already has the exact `StageKeyContext` and receipt for every selected leaf | Accepted: the producer writes one SHA-256-selected fixture manifest and consumers address those actions directly. Miss, stale build identity, wrong selector digest, receipt drift, and blob corruption all fail closed; no loader can schedule a stage. |
| 11 | Test-only stage-cache invalidation | The pipeline build fingerprint hashed external integration-test sources; fixing one Make-contract assertion changed every stage key and forced a new whole-corpus fixture production | External `tests/`, `examples/`, and `benches/` Rust targets cannot enter the producer executable | Accepted: exclude only Rust sources under those Cargo target roots while retaining every non-Rust asset there. After a retained integration-test comment edit, the workspace no-run compile rebuilt only `gmeow-dev-cli` in 2.60s and the producer fingerprint remained exactly `c5a45fb2…3850b281`; no pipeline action was invalidated. |
| 12 | Aggregate mapping and engine-golden targets | The artifact drift gate already runs the registered mapping producer; the first `bench-soak --soak 3` iteration is the golden check | Duplicate facade targets repeated the same authority | Accepted: remove only the duplicate aggregate entries. `mappings` and `bench-golden-gate` remain standalone commands. The Make contract test prevents accidental reintroduction. |
| 13 | Warm pipeline cache hydration | The prior warm artifact check was 287.173 s; cached reason hydration was 119.698 s and cached snapshot hydration 129.369 s | Large cache products must be deserialized even when computation is skipped | Superseded by the unified sync boundary: full-run cumulative stage snapshots are no longer persisted; the clean whole-run manifest skips the pipeline instead. |
| 14 | Source ingestion | Hypothesis: repeated authored RDF parsing dominated source-load | Phase telemetry measured authored dataset load at 90 ms and base N-Quads at 126 ms | Rejected: an immutable authored-corpus abstraction would add API surface without touching the measured bottleneck. |
| 15 | Self-description carrier construction | 28.602 s baseline, 27.445 s candidate | Real but secondary whole-repo work | No change: output assembly is already a single canonical carrier path; speculative caching or a parallel representation would threaten Principle 4 for less than one tenth of the former slice-quality cost. |

An intermediate change that parallelized slices without changing the inner
closure probe reduced the cold pipeline only to 394.040 s (-17.61%) while
raising user CPU time. It was not accepted as the explanation for the final
gain. The inner algorithm and ownership change is load-bearing.

## Main-source crate sweep

The sweep below makes the disposition of each main Rust performance surface
independently reviewable. Pipeline-stage figures come from the same cold
9,614-artifact candidate run reported above. A rejected mechanism means that it
did not meet the measure-first burden; it is not an implied second work list.

| Scope and symbol | Measured evidence | Candidate mechanism | Semantic risk | Minimum acceptance proof | Disposition |
|---|---|---|---|---|---|
| `gmeow-logic`: `reason_all`, `oracle_subsumptions`, `owlrl_subsumptions` | The combined gate spent 203.560 s in native reasoning and 4.171 s in oracle work; the focused allocation counter recorded 37,145 allocations before the ownership change | Reuse one closure, parallelize independent immutable worlds, and reject rows through borrowed `TermRef` values before owning output | Closure completeness, world isolation, and deterministic pair ordering | Exact inferred-axiom/consistency/oracle counters, serial/parallel parity under a fixed pool, producer-execution count, and allocation counts | Accepted; this is the dominant measured Rust gain |
| `gmeow-logic-compile`: `compile_program` and `CompileLogicStage::run` | `stage-compile-logic` took 12.448 s, 6.5% of the 192.973 s cold command | The compiler retains both named output strings and cloned `ProjectionResult` values; the stage also clones the projection vector for its JSON channel | Projection-report, preservation-ledger, typed-handle, and fanout consumers intentionally share one semantic product; an ownership rewrite can silently separate those authorities | A focused compile allocation counter plus byte identity for every projection, report, ledger, channel, and typed-handle backing graph | Rejected: no allocation profile attributed enough of the stage to those clones, and the stage is not on the dominant serialized path |
| `gmeow-pipeline`: `SourceLoadStage::run` and the DAG scheduler | The scheduler took 175.215 s; source-load fell from 342.706 s to 55.747 s after its 313.548 s slice-quality subphase was isolated | Add nested phase evidence, then parallelize only the independent leave-one-out work; preserve the scheduler's indexed products and cache keys | Artifact bytes, graph attachment order, stage-digest identity, and concurrent peak memory | All 9,614 generated artifacts byte-clean, identical assessment RDF, stage-order tests, and peak RSS below the 16 GiB contract | Accepted; coarse carrier/cache-format rewrites were rejected because the measured inner algorithm explained the bottleneck |
| `gmeow-validate`: `ValidationRun::run` and per-example SHACL | `stage-validate` took 18.609 s in the cold pipeline | The current path already parses each source once, builds one shared dataset, content-addresses merged SHACL results, projects the base once, and uses indexed Rayon work for cache misses | Finding order, first-error selection, cache invalidation, and SHACL graph semantics | A phase/allocation profile naming a residual producer, followed by identical normalized diagnostics and cached/uncached conformance results | Rejected: static inspection found the expected reuse and deterministic parallel fold, but no measured residual mechanism |
| `gmeow-docs`: `render_site_lang_exec`; pipeline MCP `McpView::run_docs_query` | Whole-site render took 3.957 s; the standalone MCP full-card test fell from 12.677 s to 6.711 s | Keep fixture/model render caches; share one immutable documentation dataset across all card queries with `OnceLock<Arc<_>>` | Language fallback, page order, query result order, and cache cross-contamination | Exact site/JSON bytes, every tier field, and pointer identity for the shared projection | Accepted for MCP query reuse; no render rewrite was justified at a 2.1% pipeline share |
| `gmeow-slice-quality`: `score_slices_with_rubric_timed` and `reasoner_axis` | The phase fell from 313.548 s to 27.750 s (-91.15%) | Use indexed Rayon slice/probe buffers and a borrowed early-exit membership query instead of allocating a full closure set | Authored slice order, diagnostic/finding/RDF order, and concurrent closure memory | Repeated serial/parallel equality under a fixed pool, assessment byte identity, generated-artifact parity, and peak RSS | Accepted; both the scheduling and inner ownership changes are required |
| Targeted SIMD: identifier scanners and escaping loops in `up_projection_gates`, `slice-quality::axes`, `docs::coverage`/`render`, and logic projection writers | No phase or allocation sample attributed measurable command time to these loops; the measured costs are graph closure, SHACL, whole-corpus assembly, and repeated querying | Vectorize a contiguous byte/typed-ID kernel only after a profile isolates it; retain a scalar implementation as the semantic oracle | UTF-8 boundaries, RDF/XML/N-Triples escaping, short-input overhead, target-feature dispatch, and cross-architecture reproducibility | A real-corpus Criterion case with scalar/SIMD byte parity, separate short/long distributions, and supported-target fallback measurements | Rejected: none of the inspected loops is presently a profile-proven SIMD kernel |

The sweep also checked dense IDs, enum layout, boxed iterators, scratch reuse,
hash lookups, lock scope, and representation-aware I/O. The accepted changes
were the cases with a measured end-to-end mechanism: borrowed term inspection,
early-exit membership, deterministic indexed parallelism, one-owner reasoning,
and immutable projection reuse. Changing integer widths, hashers, enum layout,
or cache serialization without a producer-specific profile would trade away
semantic transparency for an ungrounded micro-optimization.

## Sealing audit

Sealing is an API-closure tool, not an automatic devirtualization switch. The
audit therefore changed no trait solely to claim a speedup.

| Trait | Dispatch shape | Decision |
|---|---|---|
| `pipeline::node::Stage` | `Arc<dyn Stage>`, invoked once per long-running DAG stage | Keep open inside the workspace. Sealing would retain the vtable and would block focused synthetic-stage tests without affecting stage runtime. An enum dispatch conversion would need separate measurement. |
| Logic world-fact source seam | Hot dynamic source with boxed iterator variants | Do not merely seal. The possible gain is a measured GAT/RPITIT, enum, or callback walker that removes boxing and dynamic dispatch; sealing alone removes neither. Existing test fakes are useful. |
| Logic source adapter | One implementation, called through generic/static code | Already monomorphized. Sealing or deleting the trait has no runtime mechanism to improve. |
| Provenance and tuple-annotation algebras | Intentional algebraic extension seams | Keep implementable. Their type-level contracts select semantics, not a plugin-shaped hot vtable. |
| Physical cursor lending iterator | Internal storage contract | Already sealed; no action required. |

This follows the doctrine's narrow rule: seal validated storage or carrier
contracts when implementation closure is itself an invariant. For a performance
claim, first remove the dynamic dispatch or allocation shape and measure that
change.

## Determinism and gate-composition contracts

- Parallel work always returns indexed private buffers. Observable mutation,
  diagnostics folding, RDF concatenation, and sort/dedup happen sequentially in
  the pre-existing order.
- Internal phase timings are observational only. They do not enter stage product
  digests, cache keys, generated artifacts, diagnostics, or GTS frames. Cache hits
  emit no fabricated subphase timings.
- The allocation benchmark installs its counting allocator only in its dedicated
  bench binary.
- The immutable MCP documentation projection is initialized once per server and
  shared by `Arc`; query results and their authored order remain unchanged.
- Standalone Make targets retain their full behavior. Only the aggregate target
  uses the non-overlapping composition, and a Rust test parses the Makefile to pin
  that contract. Its explicit Rust-build node owns cold workspace preparation;
  downstream Rust consumers never race the readiness stamp.
- Impact-selected aggregate runs accept reuse only after GitHub provenance
  attestation plus exact commit/tree/registry/toolchain/task-inventory checks.
  Selection uncertainty executes the complete DAG; timing/explanation output is
  observational and unsigned local data.
- GTS payload compression remains exactly zstd-rsyncable level 12.
- The O3/checks-on/no-debug-symbol Cargo profile is unchanged; the doctrine now
  describes the actual checked-in profile accurately.

## Reproduction lanes (historical — commands as they existed at measurement time)

At measurement time, the repository targets used for final evidence were:

```bash
make help
make fmt
make reason-gate       # REMOVED; use make reason-verify
make reason-verify
make reason-crosscheck # REMOVED — the live native-vs-purrdf::entail oracle lane
make check-sync
make gts-frame-profile-gate
make check
make check-full        # REMOVED; `make check` now always runs every task
make heavy             # CI-only breadth lane
```

The focused allocation counter was intentionally a bench-only diagnostic and has since been
removed along with the oracle lane it measured:

```bash
make bench-entail-oracle-alloc   # REMOVED
```

For timing comparisons, build once, run the exact same command and population,
record process-tree RSS and phase JSON, and keep the machine quiescent. Do not
turn the report-only wall times above into CI thresholds. Current reproduction
should use `make reason-verify` and `make check`; `reason-gate`,
`reason-crosscheck`, and `bench-entail-oracle-alloc` no longer exist.
