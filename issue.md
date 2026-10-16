# Issue #1757: perf/logic: finish merged pipeline validation and remove the remaining serial critical path

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

## Context

PR #1740 integrates the current #1704 implementation: authenticated O3/full-LTO production, fixture ownership and read-only test consumption, native source/catalog reuse, typed correspondence and deontic DAG work, native RDF 1.2 literal identity, source admission/TPTP integration, old-graph ownership removal, and broad duplicate-test extraction.

The implementation was committed after the normal repository commit hook passed. This issue owns the work deliberately left after merging that concrete implementation; it must not be represented as completed by #1740.

## Required follow-up

- Complete a fresh `make check` over the merged revision and resolve every failure. The final optimized `check-sync` diagnostic run was intentionally stopped after sufficient profiling evidence and did not complete, so no full-gate pass is claimed.
- Re-run both independent cold CI generations and exact output comparison on the merged revision. Preserve the producer/test separation and never let tests construct corpus fixtures.
- Finish authenticated fixture production, then run the focused nextest inventory covering correspondence laws, expression selection, native literal identity, source admission, old-graph removal, and the ordinary/heavy conformance selectors.
- Diagnose and remove the remaining post-action critical path. In the measured run, 368 producer actions published in about 95 seconds, followed by more than 18 minutes with one active in-process analysis. Use the existing source-load per-slice timings plus added recorder timings to identify the exact owner before editing.
- Split independent `stage-conformance` recorder families into deterministic parallel branches where measurement confirms value. Merge isolated artifact maps in declaration order and hard-fail duplicate ownership.
- Do not introduce a narrow producer feature, crate, or separate compilation lineage as a performance measure: `make check` builds the complete required Rust surface, so shifting code between Cargo invocations is not removed work. Retain the authenticated O3/fat-LTO producer contract and optimize only measured work removed from the end-to-end gate, including duplicate execution, repeated lowering/parsing/materialization, avoidable corpus reads, and serial scheduler dependencies.
- Measure conversion removal, prepared-analysis reuse, certified fusion, incremental reuse, and scheduling separately, including elapsed time, peak RSS, and intermediate bytes. A structural gain is sufficient; do not manufacture a comparative speedup claim.
- Reconcile and close the remaining #1704 child issues only after their own acceptance evidence is present. In particular, #1748 still requires the complete program-aware admitted native run; exporter/source-admission code alone is not that certificate. Existing PurRDF #317 remains upstream-owned and GMEOW must not duplicate its generic conformance suite.
- Review current CodeRabbit findings and required CI on the post-merge follow-up PR before its final merge.

## Preserved contracts

Keep standpoint, provenance, complements, loss evidence, logical stage identities, required checks, exact source/context/program identities, and both cold generations. Bounded-corpus evidence never authorizes unrestricted rewrites. Generic RDF 1.2, SPARQL, SHACL, and codec conformance remains exclusively PurRDF-owned.

Principles 1, 2, 3, 4, 5, 6, 7, 9, 12, 15, 17, 18, and 19.



## 2026-09-17 current-head evidence

The native-pipeline integration in `a159730f8` replaced the certified batch leave-one-out path with one complete `NativeReasoningSession::retract` per selected axiom. The slice-quality sweep permits 64 probes for each of 143 slices, so this restored thousands of full native transactions inside `stage-source-load`.

Merged PR #1760 at `b1676af1a` restores one immutable source analysis for exact taxonomy reachability, fixed-head absence, and property-characteristic impossibility. Existing tests compare every certified family with full scratch/native retraction, including canonical `logic:` spellings, finite-union fallback, multiple worlds, and alternative proofs. Unclassified constructs continue through the complete native transaction; no bounded-corpus observation authorizes a broader rewrite.

The merged change also excludes machine-leased Cargo output directories from producer resolution identity. Output placement does not select packages, features, targets, or sources, and must not make a freshly built producer receipt stale on the next invocation.

Validation completed on the commit:

- `make lint` passed.
- `cargo test -p gmeow-logic leave_one_out --no-fail-fast` passed all four focused parity tests.
- `cargo test -p gmeow-build-inputs resolution_environment_tracks_selection_without_cargo_injected_test_noise` passed.
- `gmeow-dev i18n lint` passed with zero errors after the canonical projection-loss labels were disambiguated.
- An authenticated O3/full-LTO producer built successfully; its receipt remained valid on the following invocation.

The cold update pipeline was stopped after `stage-source-load` exceeded ten minutes. This is an incomplete diagnostic observation, not a completed gate or a speedup claim. The next implementation must expose certified/slow probe counts per slice, identify the residual native families, extend only proof-backed shared analyses, and complete the full cold pipeline plus both independent CI generations. Workspace compilation remains required either way, so moving code behind a narrow producer feature or crate is explicitly outside the optimization target.



## Comments (24)

### paudley — 2026-09-17T05:29:42Z

Current-main cold CI exposed a prerequisite defect before Rust compilation:

- run 35184774821, job rust-prebuild 105084374216
- `make rust-prebuild` reached `scripts/test-corpus-producer-scanner.sh` and exited 127 because the job did not provision `rg`
- the lint and archive-shard jobs already treated ripgrep as an explicit hosted-runner dependency

The active #1757 worktree now provisions the pinned ripgrep installer in both the primary `rust-prebuild` job and the `rust-archive` cache-miss fallback, with the workflow contract test requiring both placements. `make lint` passes; the focused synthetic nextest contract is compiling from a cold local cache.

This is an unblocker, not a performance claim. The optimized producer build in the same hosted run remains in progress and will provide the next current-head critical-path evidence. The earlier generation evidence put `stage-conformance` at 144 ms, so no unsupported conformance split is being made. The structural scheduler target remains the level barrier in `crates/pipeline/src/scheduler.rs`: completion-driven publication/readiness with deterministic products and receipts, declared resource exclusion, and carrier release after each actual last reader (Principles 4, 7, 12, 15, 17–19).


### paudley — 2026-09-17T05:50:11Z

The first current-head hosted generation reached the transferred O3/full-LTO producer and exposed a second pre-pipeline defect: runtime admission rejected the receiving job's Cargo-resolution environment as stale. Both independent generations and fixture-prefix failed at the same boundary before corpus work.

PR #1758 now separates the two authorities:
- build/refresh verification remains strict over current Cargo resolution inputs;
- transferred runtime admission still verifies exact executable bytes, embedded recipe and compilation policy, selected Cargo graph binding, and current source inventory, without requiring the execution host to recreate the builder's job-local Cargo environment;
- CI provisions ripgrep before both prebuild scanner paths.

Focused synthetic validation passes (2 tests, 7,146 skipped), and CodeRabbit's scanner-order finding was incorporated. The refreshed hosted run is the direct acceptance check for cross-job producer transfer.


### paudley — 2026-09-17T05:52:13Z

Current-main hosted producer evidence is now available (run 35184774821):

- wall: 1,112,486 ms (18m 32.486s)
- max RSS: 7,813,704 KiB
- aggregate CPU utilization: 215.24%
- host-normalized CPU utilization: 53.81% on 4 logical CPUs
- output tree: 160,626,356 bytes across 3 files
- cache classification: partial Cargo / partial overall
- filesystem output blocks: 6,110,288

This is measured O3/full-LTO producer-build evidence, not a speedup claim. It confirms the optimized producer build/link surface is itself a major critical-path owner and does not saturate the four-core runner. The narrow producer surface remains required follow-on work in #1757; scheduler changes alone cannot remove this 18.5-minute pre-generation cost.


### paudley — 2026-09-17T05:58:04Z

fix(ci): admit transferred optimized producers

Summary
- Admit the exact authenticated O3/full-LTO producer on clean execution hosts without requiring them to recreate the build controller's job-local Cargo-resolution environment.
- Preserve strict Cargo-resolution freshness for producer build and verification.
- Provision ripgrep before both CI paths that can invoke the corpus-purity scanner.
- Strengthen workflow contracts to prove scanner installation precedes compilation.

Requirements satisfied
- Unblocks both independent cold generation jobs and fixture-prefix from using the transferred optimized producer.
- Keeps exact executable bytes, embedded recipe and compilation policy, selected Cargo graph binding, and current source inventory mandatory.
- Keeps missing or stale source evidence fail-closed.
- Incorporates and resolves CodeRabbit's scanner-order review finding.
- Advances #1757 under Principles 4, 7, 12, and 15.

Files
- .github/workflows/ci.yml: install the pinned ripgrep action in primary and fallback prebuild paths.
- crates/action-cache/src/executable.rs: separate strict build-input verification from portable current-source admission.
- crates/action-cache/src/executable.tests.rs: prove portability across Cargo environments and rejection of changed selected sources.
- crates/gmeow-dev-cli/src/dev_producer.rs: use portable source admission after exact producer authentication.
- crates/gmeow-dev-cli/tests/make_gate_contract.rs: require scanner provisioning before either prebuild invocation.

Validation
- make fmt: passed.
- make lint: passed.
- make nextest-synthetic NEXTEST_FILTER='test(runtime_source_admission_is_portable_across_cargo_environments) | test(ci_reuses_one_authenticated_nextest_archive_without_coverage_loss)': 2 passed, 7146 skipped.
- Hosted CI final candidate b6c4441ba: lint passed; producer-build and rust-prebuild were still running at merge finalization. The two cold generations and remaining required jobs were not claimed. Issue #1757 durably owns their completion and every resulting repair.

Repository discipline
- .goals standing constraints were respected.
- .deficiencies has exactly one marker and no entries.
- Deferral scan over origin/main...HEAD returned no findings.
- No merge conflicts.


### paudley — 2026-09-17T07:01:47Z

#1759 carries the completion-driven scheduler slice. Local focused checks pass, including the regression that proved the coordinator had consumed one configured worker slot; the implementation now uses in-place Rayon scope coordination and all configured workers remain available.

The PR deliberately leaves full hosted validation here. The prior main run `35187835885` passed lint, producer-build, and rust-prebuild and admitted the optimized producer into both independent cold-generation jobs; those cold generations were still running when #1759 opened.


### paudley — 2026-09-17T09:06:52Z

perf(logic): restore certified batch slice probes

Summary
- Restore one immutable certified analysis for slice-quality leave-one-out probes after the native-pipeline integration regressed to a complete native retraction per axiom.
- Answer exact subclass/subproperty reachability, fixed-head absence, and property-characteristic impossibility without constructing a native session.
- Preserve the complete native retraction path for every unclassified construct and for mixed batches.
- Exclude machine-leased Cargo output directories from producer resolution identity; output placement does not change resolved packages, features, targets, or sources.
- Disambiguate the ProjectionLoss class, projectionLoss code property, and lossyDrop translations exposed by the i18n gate.

Issue requirements satisfied
- Removes repeated lowering/execution from the measured stage-source-load critical path through shared immutable analysis.
- Keeps optimized O3/full-LTO production and test/debug lineages separate.
- Preserves canonical logic vocabulary lowering, world identity, exact source retraction, loss evidence, deterministic result order, and native fallback authority.
- Records incomplete end-to-end validation and residual probe work durably on #1757, as explicitly requested by the maintainer, without presenting it as completed by this PR.

Files
- crates/logic/src/reason/leave_one_out.rs: certified batch source indexes and exact-family classifiers.
- crates/logic/src/reason/mod.rs: partitions each batch into certified answers and unresolved native transactions.
- crates/build-inputs/src/resolution.rs and resolution_tests.rs: remove Cargo output slots from semantic resolution identity and test selection/output separation.
- slices/grounding/logic/module.ttl plus fr.po/zh.po: distinct canonical projection-loss concepts and translations.

Validation
- make lint: passed.
- cargo test -p gmeow-logic leave_one_out --no-fail-fast: 4 passed, including scratch/native parity for every certified TBox family, canonical logic spellings, finite-union fallback, multiple worlds, and alternative proofs.
- cargo test -p gmeow-build-inputs resolution_environment_tracks_selection_without_cargo_injected_test_noise: 1 passed.
- optimized gmeow-dev i18n lint: zero errors.
- authenticated O3/full-LTO producer build: completed; next producer invocation accepted the receipt across a different leased output slot.
- cold update pipeline: intentionally stopped after stage-source-load exceeded ten minutes. This is not a full-gate pass and no comparative speedup is claimed; #1757 owns the remaining measured work.

Repository contracts
- .deficiencies is clean.
- .goals and the no-optionality doctrine are respected: unsupported probe families use the full native path, never a weaker answer.
- The branch contains no TODO, stub, placeholder, deferred implementation, or future-work marker. The PR's reference to residual work identifies separately owned #1757 acceptance scope per the maintainer's explicit instruction to merge the verified implementation and file all unfinished validation/work durably.
- origin/main merged cleanly; no conflicts.

Implementation notes
- The batch index retains exact authored predicate spellings so retracting logic:subClassOf does not accidentally retract an independently authored rdfs:subClassOf twin.
- Reachability is world-scoped and returns true when any selected world retains a proof, matching complete native source retraction semantics.
- Open-head guards are conservative. Any producer ambiguity yields None and routes the probe through NativeReasoningSession::retract.

Principles 1, 4, 7, 12, 17, 18, and 19.


### paudley — 2026-09-22T09:02:22Z

Current-head execution plan for the remaining #1757 acceptance work

Measured starting point

- The prior cold run published `stage-conformance` in 144 ms, so that stage is not a supported optimization target.
- The 2026-09-22 authenticated run published 368 upstream actions, including `stage-archive-blobs`, then spent more than three hours inside the recompute-only `stage-snapshot` action before the maintainer killed it.
- Current source shows that `stage-snapshot` serially performs carrier projection/union, the authored gate-verdict derivation over the complete composed carrier, canonical serialization of that complete carrier for the coherence hash, per-graph axiom hashing, certificate folding, and typed-handle re-pinning. The exact phase still needs measurement before an edit.

Execution

1. Reproduce from the authenticated upstream action cache under a hard 30-minute command deadline. Record phase wall time, CPU, peak RSS, quad/term counts, and materialized bytes. Use external sampling first; add report-only recorder spans only where the current process boundary cannot distinguish the serial phases.
2. Remove only the measured duplicate materialization, canonicalization, scan, or scheduler dependency. Preserve the exact carrier dataset, graph identities, RDF 1.2 side tables, blank-node scoping, standpoint/provenance/complement/loss evidence, typed handles, O3/full-LTO producer identity, and deterministic bytes.
3. Add a focused synthetic regression that proves the removed operation stays removed and that old/new graph and byte projections are identical. Tests will consume supplied fixtures only; they will not produce corpus data.
4. Run focused formatting, lint, unit/nextest inventory, and a bounded cold pipeline generation. Every heavy or quiet command is killed at 30 minutes; a timeout is a failed measurement and drives another diagnosis, never a pass.
5. Complete a fresh `make check`, authenticated fixture production, and the required focused nextest inventory for correspondence laws, expression selection, native literal identity, source admission, old-graph removal, and ordinary/heavy conformance selectors.
6. Run two independent cold CI generations from empty action caches and compare their generated outputs byte-for-byte. Record elapsed time, peak RSS, CPU utilization, cache classification, intermediate bytes, and exact artifact digests without manufacturing a relative speedup claim.
7. Reconcile every remaining #1704 child only against its own acceptance evidence, including #1748's complete program-aware admitted native run. Review and resolve current CodeRabbit findings and all required CI checks, then merge only through `ghprsq`.

This plan covers the complete issue rather than treating an intermediate profiling or local test result as completion. It is governed by Principles 1, 2, 3, 4, 5, 6, 7, 9, 12, 15, 17, 18, and 19.


### paudley — 2026-09-22T16:45:15Z

2026-09-22 current-main critical-path evidence

The authenticated warm-prefix reproduction now separates the previously conflated owners:

- `stage-source-load`: 4,705 ms on the accumulated action cache
- `stage-compile-logic`: 7,501 ms
- `stage-mappings`: 62,858 ms
- `stage-conformance`: 89,104 ms
- `stage-reason`: still active when the run's 15-minute deadline fired; it had been active for more than 14 minutes
- `stage-snapshot`: never became ready in this run
- observed process high-water RSS: 11,150,580 KiB during the concurrent prefix; steady RSS during the isolated reasoning tail was about 1.15 GiB

This corrects both earlier interpretations. The old 144 ms conformance observation did not represent current cold execution, while the prior three-hour run does not yet prove that snapshot alone owns the whole delay because current reasoning prevents snapshot readiness.

The reason stage already defines three returned timing phases (`assemble-object-edb`, `construct-closure-and-artifacts`, and `assemble-reason-product`), but those records are unavailable when a bounded run terminates before stage completion. The active #1757 branch adds INFO-level live boundaries around those same phases so the next admitted optimized run identifies the exact long phase without changing outputs or cache identities. Formatting passes and the affected optimized crate lineage compiled; the final O3/full-LTO link was stopped by the bounded build deadline, so no completed instrumented producer or performance fix is claimed.

All heavy commands were bounded at 30 minutes or less. Principles 4, 7, 12, 15, 17, 18, and 19 remain preserved.


### paudley — 2026-09-22T17:57:26Z

Bounded follow-up measurement on the optimized producer isolated the current critical path inside `stage-reason`.

- object-level EDB assembly: 1,886 ms for 630,651 quads
- canonical relabeling: 384 ms for 630,651 quads / 191,844 terms
- `prepare_reasoning_input`: still active when the 18-minute process cap fired, for a measured lower bound of about 16.5 minutes in that phase alone
- live state during preparation: sustained CPU work, about 1.1 GiB RSS; no lock wait

Inspection found that the contextual native `SourceView` rebuilt an ordered term dictionary over the complete source and inherited a full 630k-row scan for each contextual pattern lookup. The worktree now preserves first-encounter term IDs and prior row order while using fixed-seed lookup-only hashing, retained native row partitions, and a predicate index for pattern probes. This keeps deterministic semantic output while removing repeated corpus-wide scans (Principles 4, 7, 18).

Current validation: `make fmt` passes, whitespace check passes, and `cargo check --locked -p gmeow-logic` passes in 41.73s. The repository-native nextest plan is currently blocked before execution by the missing generated projection that depends on this producer path; no test was allowed to produce the corpus. Next bounded step is an optimized rebuild and the identical before/after pipeline measurement before any commit or PR.


### paudley — 2026-09-22T18:42:40Z

The indexed native source-view repair has now been measured with the rebuilt O3/full-LTO producer on the same 630,651-quad reason input.

- optimized producer build completed in 21m07s
- object EDB assembly: 2,970 ms
- canonical relabeling: 529 ms
- `prepare_reasoning_input`: **3,933 ms**, down from the prior measured lower bound of about 16.5 minutes (at least 251x faster; the old phase never completed)

The pipeline therefore advanced into `reason_all` for the first time in these bounded probes. That exposed the next critical defect: about 115 seconds after `reason_all` began, the kernel globally OOM-killed `gmeow-dev`. Kernel evidence records 39,807,824 KiB anonymous RSS, 154,735,148 KiB total VM, and exit 137. No producer process survived.

I added deterministic INFO boundaries around modal admission, schema/source preparation, native binding, program planning, world initialization, effect scheduling, per-stratum/per-round row and candidate counts, materialization, and publication. The telemetry adds no budget and does not enter semantic results (Principles 4, 7, 18). Formatting and whitespace checks pass. The offline targeted compile refreshed only the existing `tracing` dependency edge in `Cargo.lock`, then hit its five-minute command cap while rechecking dependencies with no source diagnostic; that compile is incomplete, not a pass.

Next bounded run will finish that warmed compile, rebuild the producer, and identify the exact materialization round responsible for the 38 GiB growth before changing the solver dataflow.


### paudley — 2026-09-22T18:46:23Z

Follow-up validation correction: the warmed `cargo check --locked -p gmeow-logic` completed successfully in 3m11s. The deeper native-materialization telemetry compiles; the earlier five-minute timeout was dependency recompilation only and is superseded by this pass.


### paudley — 2026-09-22T20:00:23Z

Measured root cause update (2026-09-22): the 24 GiB address-space-capped producer run now localizes the allocation failure to `ValueFlow::summarize` while abstracting the 631,136-fact native source.

Phase evidence from the optimized producer:

- `prepare-reasoning-input`: 2,799 ms
- native reasoning template build: 23 ms
- source-operator admission: 832 ms
- allocation failure after `summarize-native-source-flow` started and before it ended
- global effect refinement and native evidence binding were never reached

Static inspection explains the scaling defect. Every conditioned `(predicate, subject)` or `(predicate, object)` support was stored as a full-width `Domain` bitset sized to the complete operator universe, even when that support contained one opposite value. With a corpus-sized operator universe, sparse source edges therefore retained corpus-width allocations.

The worktree now uses an adaptive support set: sorted sparse indices below the bitset memory break-even, with promotion to the existing dense `Domain` when density warrants it. Canonical summary hashing emits the same word stream for both representations, so cache identity and abstract semantics remain representation-independent.

Current focused validation:

- `cargo check --locked -p gmeow-logic`: pass
- existing value-flow unit module: 13 passed
- new sparse/dense identity regression: pass
- `make fmt`: pass
- whitespace diff check: pass

The repository `make nextest` wrapper could not start because the previously aborted producer sync left `generated/projections/lang/gmn-codebook.cbor` absent. No full producer rerun has been claimed yet. The next bounded run will rebuild the optimized producer and re-run the exact 24 GiB capped measurement before this is considered fixed.


### paudley — 2026-09-22T21:12:41Z

Follow-up measurement (2026-09-22): the first adaptive-support build still exceeded the 24 GiB address-space ceiling inside production `ValueFlow::summarize`.

I separated scheduler concurrency from reason-stage memory using the supported `sync --jobs 1` path. The serial run reached the same 631,136-fact input and failed after `summarize-native-source-flow` began, with a 15,512-byte allocation request. This proves the remaining pressure belongs to the summary representation rather than concurrent mappings/archive stages.

The remaining dense path was the relation column pair: every distinct predicate retained two full operator-universe `Domain` bitsets. The source loop also created three full-width singleton domains for each input fact. The worktree now:

- uses the adaptive sparse/dense domain for relation columns as well as conditioned supports;
- promotes at the bitset memory break-even;
- materializes a dense column view only while binding a rule;
- publishes exact source facts directly, eliminating three universe-width temporary allocations per fact;
- preserves the existing canonical word-stream identity for both sparse and dense representations.

Focused validation after this second-stage change:

- `cargo check --locked -p gmeow-logic`: pass
- value-flow unit module: 14 passed
- `make fmt`: pass
- whitespace diff check: pass
- deficiency ledger: empty

A fresh optimized producer rebuild and identical 24 GiB serial measurement remain required before claiming #1757 fixed.


### paudley — 2026-09-22T22:55:33Z

The rebuilt adaptive-column producer advanced substantially farther, but the 24 GiB serial run still failed inside `summarize-native-source-flow` on a 15,512-byte allocation after sustaining roughly 12.5 GiB RSS. The run completed the previously expensive prefix first: cached conformance took 363 ms, mappings 59.8 s, slice-brief 109.5 s, and the 11,706-fact source-load reasoning invocation completed. The main `stage-reason` input then reached 631,136 facts; EDB assembly took 3.04 s, canonicalization 556 ms, and `prepare-reasoning-input` 4.25 s.

The remaining multiplicative allocation is now identified. Contextual producer effects declare exact output predicates with wildcard subject and object positions. `FlowSummary::publish_rectangle` eagerly expanded each Cartesian output envelope into one conditioned support for every value in the 124,096-cell operator universe. Each support retained a full opposite-domain allocation of 15,512 bytes. One wildcard envelope could therefore demand roughly 1.9 GiB per predicate before map and allocator overhead.

The worktree now retains producer envelopes as bounded Cartesian rectangles and keeps exact source-edge supports separately. Binding unions exact supports with matching rectangles directly, without allocating dense intersection temporaries. Subsumed exact supports and narrower rectangles are removed. The summary cache identity is versioned for the new canonical representation; logical source facts and output artifacts are unchanged.

Validation on the exact change:

- `make fmt`: passed
- `cargo check --locked -p gmeow-logic`: passed in 18.46 s
- focused value-flow module: 15 passed, including existing concrete-derivation parity tests
- new 4,096-value structural regression proves a wildcard output remains one rectangle with no per-value support maps
- authenticated O3/full-LTO producer build: passed in 23m30s
- `make producer-verify`: passed
- `.deficiencies`: no entries

The next bounded production run must still prove that the 631,136-fact summary completes under the same 24 GiB address-space ceiling before this defect is considered fixed. Principles 4, 7, 12, 15, 17, and 18 remain preserved.


### paudley — 2026-09-22T23:51:40Z

The compact-rectangle producer has now passed the identical production measurement that previously failed.

Under `sync --jobs 1`, an 18-minute hard timeout, and a 24 GiB address-space ceiling, the 631,136-fact main reasoning input recorded:

- EDB assembly: 3.04 s
- canonicalization: 556 ms
- `prepare-reasoning-input`: 4.25 s
- template construction: 24 ms
- source-operator admission: 930 ms
- `summarize-native-source-flow`: **634 ms**, completed successfully
- observed RSS immediately after summary: roughly 262 MiB

The previous 15,512-byte allocation failure and multi-gigabyte wildcard-envelope expansion are gone. The run did not crash or exceed memory. It reached `refine-native-global-effects`, where the 18-minute process cap expired after an exact lower bound of about 7m05s in that phase.

Inspection identified the new CPU multiplier: `head_domains` and `publish_head_conditions` reran the complete abstract join once for every conditioned head value. Source-operator admission placed roughly 124,000 exact operator candidates in that set, so a 286-effect refinement could perform tens of millions of singleton joins even though the converged binding already soundly over-approximates every concrete firing.

The worktree now publishes each converged producer-head Cartesian envelope once. Exact source-edge correlations and protected-grammar narrowing still participate in the binding fixed point; no input rows or concrete closure are retained. Summary identity also uses a canonical nonzero-word stream, avoiding the measured 17-second scan over zero words while preserving sparse/dense representation equality.

Validation of the new refinement path:

- `make fmt`: passed
- `cargo check --locked -p gmeow-logic`: passed
- full value-flow module: 16 passed
- the new scaled regression admits 4,096 exact operator candidates, retains the one reachable selector, and excludes an unreachable protected predicate; test execution took 0.02 s
- authenticated O3/full-LTO producer build: passed in 22m45s
- `make producer-verify`: passed
- whitespace checks: passed
- `.deficiencies`: no entries

The rebuilt producer is staged. The next bounded production run must measure the batched global refinement and confirm downstream reasoning/output behavior before this optimization is considered complete. Principles 4, 7, 12, 15, 17, and 18 remain preserved.


### paudley — 2026-09-23T00:46:12Z

Correction from the next bounded production run: the broad Cartesian producer-head propagation is rejected.

With the batched-head producer, `stage-conformance` took 855.728 s, versus 390.991 s for the preceding precise producer. Other prefix stages were slower under host contention, but this regression remained material after CPU normalization. A 10-second live `perf` sample recorded 602 samples with no losses and showed allocation/free and libc memory routines dominating; the mandated stripped producer prevented useful Rust symbol attribution. The 22-minute run published conformance, then reached mappings before the hard timeout. It never reached the main 631,136-fact refinement measurement.

The worktree now restores precise conditioned head supports and removes the over-conservative Cartesian propagation. It retains the independently proven improvements:

- compact Cartesian storage for external wildcard producer envelopes, which made the 631,136-fact source summary complete in 634 ms under the 24 GiB ceiling;
- adaptive sparse/dense relation columns and conditioned supports;
- direct exact-fact publication;
- canonical nonzero-word summary hashing, representation-independent across sparse and dense forms.

Large-universe INFO counters now bracket conditioned head-domain refinement and head-support publication only when a role has at least 1,000 candidates. This will identify the exact broad production rule shape without flooding ordinary conformance runs.

Current validation after restoring precision:

- `make fmt`: passed
- `cargo check --locked -p gmeow-logic`: passed
- full value-flow module: 16 passed
- the 4,096-operator unreachable-protected-target regression remains precise and executes in 0.02 s, proving universe size alone is not the cost

The authenticated producer rebuild was hard-stopped at its 25-minute deadline during the final fat-LTO link. It did not stage a new receipt, so no current production measurement is claimed for the counter-instrumented precise build. The next bounded action is to finish that warmed build, verify its receipt, and rerun the serial probe. Principles 4, 7, 12, 15, 17, and 18 remain preserved.


### paudley — 2026-09-23T01:41:29Z

Bounded production measurement of the restored precise algorithm is terminal (24-minute hard cap, 24 GiB address-space cap, `--jobs 1`): exit 124 from the time cap, not OOM. RSS stayed about 65-67 MiB while the process remained ~97% CPU.

Completed stages before the main refinement included conformance at 470.700 s, mappings at 39.224 s, source/operator admission at 0.898 s, input summarization at 0.727 s, and summary-identity hashing at 0.176 s. In the main native value-flow refinement, the first 124,047-candidate rule spent 21.315 s pruning its subject domain and then repeated the same singleton joins for 23.081 s while publishing conditional supports. Nine empty-body 8,071-candidate rules repeated the two passes for another 9.057 s total. A later 4,115-candidate rule used 2.250 s + 2.428 s.

The terminal blocker is a second two-body/four-variable rule with 124,047 conditioned subject candidates. Its first pruning pass began at 01:29:06Z and was still running when the whole command hit the 24-minute cap. The heap is no longer the limiting resource; this rule has a dynamic predicate body position, and the current seeded join searches the entire predicate domain separately for every conditioned subject. I am replacing that quadratic search with an exact derived index from conditioned subject/object values to predicates, while retaining rectangle-backed producer envelopes separately. This preserves the existing tuple-support semantics and does not revive the rejected all-head Cartesian approximation.


### paudley — 2026-09-23T02:26:13Z

Production result for the conditioned-value → predicate reverse index (20-minute hard cap, 24 GiB address-space cap, `--jobs 1`): exit 124 from the cap, not OOM. The producer was rebuilt in 15m18s and its receipt verified before this run.

The previously terminal four-variable/two-body rule now completes exact 124,047-candidate passes:

- first pruning pass: 77.092 s (the prior run spent more than 21 minutes here and never completed)
- first conditional-support publication: 65.565 s
- second pruning pass: 67.313 s
- second publication: 66.079 s

A later five-variable/four-body rule also completed pruning in 93.699 s and publication in 127.123 s. The command reached the start of a seven-variable/eight-body 124,047-candidate rule when the overall 20-minute cap fired. Conformance completed in 337.420 s and mappings in 41.932 s. Memory remained bounded; this was CPU/dataflow work.

The exact reverse index is therefore a large validated improvement: it replaces per-candidate scans across every predicate relation with candidate predicates supported by the conditioned singleton, while still applying the original relation support checks and separately consulting compact producer rectangles. A new 4,096-subject/4,096-predicate variable-predicate regression preserves the reachable protected writer and completes in 3.04 s; the full value-flow unit selection passes (17 existing tests plus the new regression).

The remaining critical path is now explicit: every broad rule computes the same seeded join once to prune a head domain and again to publish its conditional support, then repeats those full scans on subsequent fixed-point iterations. The next change must fuse the exact prune/publication pass and make closure scheduling delta-aware; no all-head Cartesian approximation is involved.


### paudley — 2026-09-23T03:17:15Z

Follow-up bounded production measurement for fused pruning/support publication plus the first dependency worklist (20-minute hard cap, one job, 24 GiB address-space cap): exit 124 from the cap, not OOM.

The fused transfer is effective per pass. For the first 124,047-candidate three-variable/two-body rule, each exact pass took 19.155-21.843 s, replacing the previous separate ~21 s pruning plus ~22 s publication passes. The 2,229-candidate rules completed fused passes in 4-17 ms. The post-closure effect-restriction rescan is also removed by retaining each converged rule's bindings and head domains.

The measurement also exposed a scheduler defect in my first worklist implementation: using an ordered set with minimum extraction lets a low-index dynamic reader jump ahead of peers already pending whenever it reschedules itself. In the bounded run, 16 broad passes consumed 327.160 s and a 17th began at timeout, so the worklist had not improved total convergence despite halving each transfer.

I have corrected the scheduler to a deterministic FIFO queue: a self-rescheduled rule goes behind already-pending peers, while a per-rule queued bit prevents duplicates. The trace now carries `rule_index` and `visit` so the next production sample can distinguish one recursive rule from structurally identical rules. The full focused value-flow selection passes after this correction: 18 tests, including concrete fixed-point coverage, protected-grammar exclusions, compact rectangles, and the 4,096 x 4,096 dynamic-predicate regression (1.38 s execution for the selection). A new production build/measurement is still required before claiming the scheduler improvement.


### paudley — 2026-09-23T04:08:41Z

Bounded production result for the corrected deterministic FIFO scheduler (20-minute hard cap, one job, 24 GiB address-space cap): exit 124 from the cap, not OOM. The FIFO producer built/staged in 15m43s and its receipt verified.

FIFO fixes the measured starvation. Broad 124,047-candidate passes advanced through distinct producers instead of repeatedly selecting the lowest index:

- rule 3, visit 1: 23.006 s
- rule 43, visit 1: 82.735 s
- rule 44, visit 1: 90.945 s
- rule 50, visit 1: 107.395 s
- rule 88, visit 1 began at the overall timeout

The prior ordered-set scheduler completed 16 broad passes (327.160 s total) with the same structural signature and began a 17th before reaching the later rules. The new `rule_index`/`visit` trace proves FIFO advances fairly across pending transfers. Producer names are now also attached to the trace for the next build.

I tested two follow-up allocation ideas locally and rejected both rather than carrying an unmeasured regression: copy-on-write `Arc<[u64]>` domains increased the focused suite from 1.38 s to 3.73 s, and cloning converged upper bounds still took 3.37 s. Both are removed. Fresh domain initialization is restored; the 4,096 x 4,096 dynamic-predicate regression passes again in 1.34 s, and the crate compiles cleanly. The next production build will contain FIFO plus producer-name tracing, without either rejected allocation experiment.


### paudley — 2026-09-23T05:49:01Z

The capped production measurement rejected the first symmetry activation predicate and identified why.

Measured with the authenticated producer, `--jobs 1`, a 24 GiB address-space limit, and a 20-minute timeout:

- `dl:maximum-one-equality` rule 88 slot 0: 124,047 candidates, 167.368 s.
- Slot 2 then started with the same 124,047 candidates, so the intended reuse did not activate.
- The run terminated at the 20-minute cap (exit 124); no process survived.
- Host variance was material in this sample: `dl:range` took 106.286 s versus 73.085 s previously, and `dl:allValuesFrom` took 110.922 s versus 94.229 s.

Cause: the first activation predicate rejected every dynamic-predicate body. That is too strict. Exchanging `?left` and `?right` is still an automorphism of the positive body, and the existing predicate dependency worklist schedules dynamic readers (including the current rule) whenever published head support changes.

The over-restriction is now removed. Focused coverage proves:

- every actual `dl:maximum-one-equality` schema variant selects symmetric refinement;
- a synthetic dynamic-predicate symmetric rule selects it;
- asymmetric heads do not select it;
- exact support is present in both relation directions;
- the complete value-flow selection passes: 21 tests, 0 failures, 1.28 s execution.

The corrected source still needs a fresh authenticated producer build and capped production trace before any performance or completion claim.


### paudley — 2026-09-23T06:38:33Z

The corrected symmetric-head refinement is now verified in the production pipeline.

Authenticated producer, `--jobs 1`, 24 GiB address-space limit, 20-minute timeout:

- `dl:functional-equality` rule 83: slot 0 completed in 1.579 s and execution advanced directly to rule 88. The former slot-2 pass (9.888 s in the prior sample) is gone.
- `dl:maximum-one-equality` rule 88: slot 0 completed over 124,047 conditioned values in 186.645 s and execution advanced directly to rule 89. No rule-88 slot-2 pass occurred; prior samples started a second 124,047-value pass.
- The run reached rule 89 before the same 20-minute cap and terminated with exit 124. No process survived.
- Focused value-flow coverage remains 21/21 passing, including every actual `dl:maximum-one-equality` variant, dynamic-predicate symmetry, asymmetric rejection, and exact support in both relation directions.

This establishes the optimization rather than inferring it from unit timing. It also exposes the next critical path: rule 89 and the remaining maximum-one equality variants each start another 124,047-value singleton refinement.

The next exact narrowing seam is the property-rule guard abstraction. `FlowRule` currently carries the positive `analysis_body` but deliberately drops pure guards. For these rules that drops `CardinalityEqual(?count, 1)`, so value flow evaluates every cardinality row and then every broad head value. Any repair must preserve accepted cardinality lexical forms such as integer `01`; substituting one literal spelling would be unsound. The appropriate direction is to carry a pure cardinality-domain constraint using the same native count parser, then measure whether the rule head domains narrow before adding broader batching machinery.


### paudley — 2026-09-23T07:31:50Z

The next exact narrowing seam is now implemented and focused-validated.

Changes:

- Native cardinality parsing now exposes one pure `parse_cardinality` function; runtime guard evaluation continues to use the same parser and bounded cache.
- Property rules export statically interpretable `CardinalityEqual` guards into `FlowRule`.
- Value-flow registers only source positions that bind those guarded count variables, preserving exact lexical forms such as integer `01` without widening the entire universe.
- A precomputed abstract domain retains every explicit term parsing to the requested count and retains `Other` for unknown producer output, so the analysis remains a sound over-approximation.
- Tuple support is used when the post-intersection row is fully conditioned; a broader requested domain no longer disables exact support after unavailable cells have already been removed.

Evidence:

- Complete value-flow selection: 22 passed, 0 failed, 3.54 s execution.
- New end-to-end abstract regression admits integer `01`, rejects the count-2 branch, and narrows the emitted output range accordingly.
- Every actual `dl:maximum-one-equality` schema variant exports exactly one `?count == 1` flow guard.
- Concrete `maximum_one_equality_requires_both_qualifiers_and_keeps_every_premise` still passes with its `01` source spelling.
- `cargo check --locked -p gmeow-logic` passes.

The authenticated producer was built and verified. A 20-minute production sample was inconclusive for this optimization because upstream host/runtime variance delayed the repository pass: `dl:domain` took 75.570 s, `dl:range` took 80.805 s, and `dl:allValuesFrom` did not begin until 19m10s. The run exited 124 at the fixed cap before reaching equality; no process survived. I am making no production speedup claim from that sample.


### paudley — 2026-09-23T18:32:40Z

Authenticated production replay has isolated and repaired the snapshot reasoner's memory failure, then exposed the remaining value-flow time concentration.

Production evidence under the same 16-thread, 24 GiB address-space cap:

- Before the repair, the authenticated producer aborted after 26.759 s with a sampled 23,968,224 KiB process-tree RSS peak. The allocation failure occurred in `construct-native-reasoning-input` while refining the minimum-qualified-cardinality witness rule.
- Root cause: property-rule lowering exported the minimum rules' generated ordinal head variables to runtime witness construction but left `FlowRule::native_witnesses` empty. Abstract value flow therefore treated each generated witness as an ordinary corpus-width value and retained candidate-specific support.
- Property lowering now classifies every head-only generated variable behind a real `witness_frontier` as a native witness. Focused coverage checks every actual minimum schema rule and exact native-witness rectangle publication.
- With that repair in an authenticated producer, the same snapshot no longer OOMed. It ran to the fixed 8-minute limit with a sampled 7,816,320 KiB process-tree RSS peak. This is memory-bounded evidence, not a completed generation or a speedup claim.

A three-minute detail replay then completed 75 large refinements accounting for 147.390 s. The dominant visit-2 costs were:

- `dl:maximum-one-equality`: 53.726 s for 9,708 conditioned candidates;
- `dl:functional-equality`: 30.570 s for 12,693 candidates;
- `dl:allValuesFrom`: 29.652 s for 10,379 candidates;
- `dl:range`: 21.154 s for 14,499 candidates.

All four use the same seeded abstract-join path. That path restarted constraint propagation from the full universe for every candidate even though the rule's unseeded fixed point was already available as a tighter sound upper bound. The active worktree now reuses that fixed point and applies only the singleton seed plus subsequent narrowing. Recursive head support still reschedules the rule through the existing predicate worklist, so an enlarged state receives a fresh upper bound on the next visit.

Current focused evidence:

- fixed-point reuse matches the original full-universe seeded result for every selected universe cell in the regression corpus;
- the complete value-flow family passes 27/27, including recursive parallel/serial parity, cardinality lexical identity, symmetry rejection, dynamic predicates, and native-witness rectangles;
- formatting and repository whitespace checks pass.

A fresh authenticated producer build and capped production replay are still required before making a runtime claim. They have not started while the host is running the PurRDF SIMD frontier benchmark and unrelated Lillith compilation. No completion, full-gate, CI, or merge claim is made.

Principles 4, 7, 12, 15, 17, 18, and 19.


