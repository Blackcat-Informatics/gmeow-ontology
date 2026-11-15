# Review: PR 2794 at b17c4b8e9

- Reviewed: 2026-11-14 17:38 CST (from `date`)
- Worktree: /home/paudley/Active/gmeow-ontology/.worktrees/fit-hosted-runner
- Head: `b17c4b8e927c766b0a2e72ef70fd008728b17f92` (`git rev-parse HEAD`; matches the requested head)
- Merge base with origin/main: `2629810da68352f54e0c87cfc11ca1c61ab673dd`
- Delta under review: `d1ec274f9..b17c4b8e9` (one commit, five files)
- Read-only review: no build, no cargo, no make. The only commands run were git, grep/sed, and `prose-check`.

## Scope

The requirement came from the caller: (1) per-stage memory records that survive a killed producer;
(2) remove the three named wastes without changing any emitted byte; (3) pass the
`make producer-recipe` macro scanner; (4) follow AGENTS.md. I judged the delta in full, plus the
files named in item C, against those four points.

## Coverage

| File | Lines read | Commit |
|---|---|---|
| `git diff d1ec274f9 b17c4b8e9` (all five files) | full | delta |
| crates/pipeline/src/memory_record.rs | 1-299 (full) | b17c4b8e9 |
| crates/pipeline/src/memory_record.tests.rs | 1-81 (full) | b17c4b8e9 |
| crates/pipeline/src/scheduler.rs | full PR diff vs 2629810da; 1385-1400 | b17c4b8e9 |
| crates/pipeline/src/stages/carrier.rs | full PR diff; 943-1070 (CarrierParts); 1290-1400 (assemble_carrier, log_carrier_size) | b17c4b8e9 |
| crates/pipeline/src/stages/reason.rs | full PR diff vs 2629810da; 19-40 (imports); 60-339 in full | b17c4b8e9 |
| crates/pipeline/src/stages/reason.tests.rs | PR hunk 839-922 | b17c4b8e9 |
| crates/pipeline/src/stages/reason_test_support.rs | PR hunk | b17c4b8e9 |
| crates/pipeline/src/stages/gate_verdict.rs | PR diff; 228-300, 335-400 | b17c4b8e9 |
| crates/pipeline/src/stages/gate_verdict.tests.rs | PR hunk 239-324 | b17c4b8e9 |
| crates/pipeline/src/stages/verify_attestation.rs, diag_render.rs, conformance/diagnostics.rs | PR diffs | b17c4b8e9 |
| crates/pipeline/src/node.rs, run.rs, bundle.rs, lib.rs | PR diffs | b17c4b8e9 |
| crates/pipeline/src/tests.rs, run/acceptance_tests.rs | PR hunks | b17c4b8e9 |
| crates/logic/src/result_rdf.rs | PR diff; 183-285, 519-532, 545-560, 688-692, 1370-1400 | b17c4b8e9 |
| crates/logic/src/result_rdf/native.rs | 48-73 (encode), 83-140 (decode) | b17c4b8e9 |
| crates/logic/src/result_rdf/tests.rs | PR hunk | b17c4b8e9 |
| crates/logic/src/math_expression.rs | 408-424 (`alpha_equivalence_edges`) | b17c4b8e9 |
| crates/logic/src/physical/lower.rs | 904-933 (`math_expression_structural_keys`) | b17c4b8e9 |
| crates/cli-core/src/lib.rs | PR diff (750-833), 706-710 | b17c4b8e9 |
| crates/gmeow-cli/src/{main.rs,lib.rs,commands.rs}, Cargo.toml dependency lines | PR diffs | b17c4b8e9 |
| crates/gmeow-dev-cli/src/{main.rs,lib.rs,dev_sync.rs,dev_feedback/acceptance_tests.rs} | PR diffs | b17c4b8e9 |
| .github/scripts/observe-producer.sh | PR diff; 25-45, 195-215 | b17c4b8e9 |
| .github/workflows/ci.yml | PR diff; nightly pin lines | b17c4b8e9, origin/main |
| docs/RUST-OPTIMIZATION.md | PR diff | b17c4b8e9 |
| crates/build-inputs/src/modules.rs | 15-48 (`qualified_macro`), 405-428 (`macro_path`), 709-852 (`macro_input`), 1254-1285 | b17c4b8e9 |
| crates/build-inputs/src/build_script.rs | 20-50 | b17c4b8e9 |
| crates/pipeline/src/stages/compile_logic.tests.rs | 448-460, plus grep for `.to_*()` | b17c4b8e9 |
| crates/pipeline/src/stages/shape_union_fresh.tests.rs | 1-30, plus grep for `.to_*()` | b17c4b8e9 |
| tracing-subscriber 0.3.23 registry source | fmt_layer.rs 1049-1060; writer.rs 694-702 | Cargo.lock version |

**Not read:** the rest of reason.rs outside 19-40 and 60-339 (the stage `run` body was read only as
diff hunks, which cover every changed line); cli-core/src/lib.rs outside the PR hunk; dev_sync.rs
outside the PR hunk; the unchanged parts of the test files; `purrdf` `CompositeDatasetView`,
`query_prepared_view` and `PipelineBundle::blobs` internals; `prepare_reasoning_input` and
`reason_all`; the full producer-recipe scanner run. No `validation.md` was provided, so I saw no
execution evidence for this head.

## Findings

**REQUIRED:** none.

**OPTIONAL-1: the kill-survival file path has no automated test.**
`crates/cli-core/src/lib.rs:762-801` (`init_tracing` memory layer). Requirement (1) depends on
`pipeline_memory` events reaching the `GMEOW_STAGE_MEMORY_LOG` file while `GMEOW_LOG` is `warn`.
Code reading says they do (the per-layer `Targets` filter; one `write_all` per event to an
unbuffered `O_APPEND` file). No test fails if a later change goes back to a global filter, for
example by restoring `.with_env_filter(...)` on the fmt builder. In that case the file stays empty
and nothing reports it. A test could build the same two-layer subscriber and run it under
`tracing::subscriber::with_default`, writing to a temporary file. This is optional because the code
is correct as written. Separately, the claim that (1) holds in practice still needs evidence: a
hosted run at this head whose resource log contains `stage memory` lines. I saw none.

**OPTIONAL-2: one claim in the commit message is slightly too broad.** The b17c4b8e9 message says
"Neither guard changed what is recorded or logged". One thing did change. With the target off,
a phase boundary crossed outside any collection now takes a value from `NEXT_SEQ`
(`memory_record.rs:225`) and then drops the record. Kept records can therefore show gaps in `seq`
that were absent before. `seq` is documented as the order the records were made (`:189-190`), and
gaps still fit that, so no behaviour is wrong. Nothing can be fixed now, because amending is
forbidden. This is noted only so the PR description does not repeat the claim.

## A. The delta d1ec274f9..b17c4b8e9

**What it does.** It removes both uses of `tracing::enabled!`: `target_enabled()` and its guard in
`memory_record::phase` (old 214-216, 268-271), and the guard at the top of `log_carrier_size`
(carrier.rs, old 1390-1392). The other three files change only comments and docs to match.

**Is this the scanner fix?** Yes. In `macro_input` (modules.rs:728-852), the multi-segment path
`tracing::enabled` falls through `qualified_macro` (:20-48, which maps only `info_span`,
`json_record`, `json_string_enum` and `inventory::collect`). The scanner then takes the last
segment, `enabled`. That name is not in the admitted list (:793-833), so the call reaches the
refusal at :848-851. `tracing::info!` resolves to `info`, which is admitted. I listed every macro
invocation on an added line across the whole PR diff against the merge base: `assert!`,
`assert_eq!`, `assert_ne!`, `format!`, `matches!`, `panic!`, `thread_local!`, `tracing::info!`,
`vec!`, `write!`. All of them are admitted. `thread_local!` with a `const { … }` initializer
already appears in production on main (mcp-wasm/src/lib.rs:133, mcp-core-wasm/src/lib.rs:79). It
contains no `extern` and no `${`, so `contains_opaque_source_syntax` does not trip. The scanner
stops at its first error, so a full producer-recipe run is the only proof that nothing else is
refused. From reading alone, I found no other refusal.

**Is it more than the scanner fix needs?** No. The only alternative would have been to admit
`enabled` in the scanner, which is a larger trust change. The delta does not touch the scanner.

**When is a record made after the delta?**
- Scheduler records are made on every dispatch, publication and release (scheduler.rs:786, :997,
  :1065). They were already unconditional, so nothing changed here.
- A phase record is made at every `measure_stage_phase` boundary (node.rs:42, :48) and at every
  snapshot-serializer phase end (carrier.rs:520), whatever the target state.
- It is kept if the thread is inside `collecting_phases`, and it is written to the log if any
  layer is interested in the `pipeline_memory` target (stderr `GMEOW_LOG`, or the file layer). In
  every other case it is dropped.
- `log_carrier_size` always samples. It is called twice per snapshot (carrier.rs:1365, :1507).

**What does an unconsumed record cost?** One `read_to_string("/proc/self/status")` (a few KB), two
small `String` allocations, one relaxed `fetch_add`, a thread-local borrow, and a disabled
`info!` callsite check. tracing evaluates field expressions only inside the enabled branch, so the
field `map`s cost nothing. Call sites are at stage level, not in per-item loops: conformance has 36
`measure_stage_phase` sites and none sits inside a per-item loop. There are 7
`sink_phase_timing` calls. The added cost is therefore a few hundred procfs reads per run.

**Can it change an output byte or an ordering?** No.
- `record` and `phase` write only to the returned `MemoryRecord`, the thread-local scope and
  tracing. None of these feeds a product, cache key, receipt or digest.
  `RunResult::memory` → `RunReport::stage_memory` → dev_sync `observations.stage_memory` only
  (dev_sync.rs, PR hunk at 657-695 and 777).
- The procfs read has no side effect on pipeline data.
- Scheduling and data order do not depend on timing: products are keyed by stage, and receipts
  and timings are sorted by `order_index` (scheduler.rs ~1134-1136).
- The only data that moves are the `seq` values of kept records, which are observations (see
  OPTIONAL-2).
- No tracing level is capped at compile time (no `max_level_*` features in any Cargo.toml), so
  `info!` is live in the release producer.

## B. Requirement (1) at this head: write to durable record

1. Records are made at scheduler.rs:786 (dispatch), :997 (publish) and :1065 (release), and in
   the phase path at node.rs:42/:48 and carrier.rs:520. All of them call
   `memory_record::record` (memory_record.rs:216).
2. `record` emits the record synchronously, before it returns, with `tracing::info!(target:
   TARGET, …)` at memory_record.rs:233-252. `TARGET` = `gmeow_cli_core::STAGE_MEMORY_TARGET` =
   `"pipeline_memory"` (:47; cli-core lib.rs PR hunk).
3. `gmeow-dev` `main` calls `init_tracing()` first and exits 1 if it fails
   (gmeow-dev-cli/src/main.rs:7-10). `gmeow` does the same (gmeow-cli/src/main.rs:7-9).
4. `init_tracing` (cli-core/src/lib.rs:762-801) works as follows when `GMEOW_STAGE_MEMORY_LOG` is
   set and not empty:
   - It opens the file with `create(true).append(true)` (:777-782). If that fails, it returns a
     typed error rather than continuing silently.
   - It adds a second fmt layer with the writer `Arc<File>` and a per-layer
     `Targets::with_target("pipeline_memory", INFO)` filter (:784-791). The stderr layer keeps its
     own per-layer `EnvFilter`.
   - With per-layer filters, the callsite is enabled when either layer is interested. So the file
     receives records even when `GMEOW_LOG=warn`.
5. tracing-subscriber 0.3.23 writes each event this way: `fmt_layer.rs:1049-1050` formats it into
   a buffer and makes one `io::Write::write_all` call. `MakeWriter for Arc<W>` hands out `&File`
   (`writer.rs:694-702`). `File` has no user-space buffer, so each record is in the kernel page
   cache once `write` returns. A SIGKILL or OOM kill of the producer cannot lose it, and an
   `O_APPEND` write of one line does not interleave with the observer's appends.
6. The observer (`.github/scripts/observe-producer.sh:34-39`) resolves the log to an absolute path
   and exports `GMEOW_STAGE_MEMORY_LOG=$memory_log` to the producer only (:214). ci.yml selects
   `pipeline_memory=info` on the three observed producer steps (:215, :440, :774). I found no
   `env_clear` or `env -i` on the producer path. The only `env_clear` is in xtask/src/cli.rs:719,
   which verifies the identity of the `gmeow` binary. Child `gmeow-dev` processes inherit the
   variable and append to the same file, and each line carries its `pid`.

Durability ends at the page cache. Losing the whole runner VM before upload would still lose the
file. The script's own comment covers that case: console samples are the fallback, and the memory
lines also go to stderr because `GMEOW_LOG` selects the target. **(1) holds by code reading. I saw
no execution evidence at this head** (OPTIONAL-1).

## C. Files with less scrutiny

**reason.rs (vs 2629810da).** The diff makes three changes:
- It wraps the existing phases in `measure_stage_phase` and replaces the hand-written start/end
  `info!` pairs with a single size event after each phase.
- It splits `project_reason_artifacts` out of `reason_prepared_input` with the body unchanged.
- It moves `alpha_equivalence_edges(&edb)` from after `reason_all` to before
  `prepare_reasoning_input`, then drops `edb` (reason.rs:174-177).

Can this change the reasoning result or the receipt bytes? No, for these reasons:
- `alpha_equivalence_edges` (math_expression.rs:411-424 → lower.rs:904-933) is a pure function
  of `&RdfDataset`. It builds a `MathGraph`, walks `BTreeSet`/`BTreeMap` in order, and uses no
  global state, counters or interner side effects. Its value does not depend on when it runs.
- The edges are not an input to the engine. `reason_all` still receives the same
  `PreparedReasoningInput` built from the same `canon`. The edges reach only
  `build_inferred_closure_into` (:278), exactly as before.
- `edb` was already immutable (`impl Deref<Target = RdfDataset>`), so reading it earlier observes
  the same data.
- Error behaviour is the same: the edge computation cannot fail, and every `?` keeps its order
  and its context string.
- The receipt is encoded in gmeow-logic (`ResultProjection`) from the `ReasoningResult`. reason.rs
  does not touch that path.
- `StageRunTiming` metadata strings are unchanged except for `closure_elapsed_ms`, which is now
  measured once. That value is an observation.
- The `reason_test_support.rs` change matches the new signature.

**Receipt bytes in verify_attestation (result_rdf.rs).** The old projection was
`project_reasoning_result(parse(project(r)))`, which encoded the receipt twice. The new one is
`transport_reasoning` followed by `ResultProjection::transported`, which encodes it once and reuses
the text. The bytes are identical for these reasons:
- `native::decode` (native.rs:83-140) accepts only text whose decoded value, written again by the
  same `write_envelope`, uses up exactly the input bytes. `encode` (:48-73) is that same
  `write_envelope` followed by hex. Therefore `encode(decode(t)) == t`.
- `transport_reasoning` also checks that the literal it reads back equals the kept text
  (result_rdf.rs PR hunk, `read_reasoning_result_expecting`).
- `Node::Receipt` displays exactly like `Node::Lit { lex, xsd:hexBinary }`, because both call
  `write_typed_literal`. The content address comes from `render_lines` through `Display`
  (:269-283, :1378), so the node IRI is unchanged.
- Every `match` on `Node` is exhaustive. The two `other => other` arms (:525-528, ~:2493) pass
  `Receipt` through unchanged, as they do `Lit`.
- The test `transported_projection_matches_the_reencoding_path_byte_for_byte` asserts byte
  equality and node-IRI equality against the old path.

**Gate verdicts over the composed view (carrier.rs:1318-1365, gate_verdict.rs).**
- The verdicts are a `BTreeSet<String>` of finding IRIs. `grade_tuples` refuses any term that is
  not an IRI, so blank-node labels cannot make the result differ between the view and the owned
  dataset.
- `release_views` (carrier.rs:990-992) clears only the `producers` cache. `sources` is untouched,
  so the final `materialize` composes the same parts in the same order as the old verdict-free
  carrier, or as the old with-verdicts carrier, which was already built after `release_views`.
- The equivalence of view and materialization is assumed of purrdf, and I did not read
  `query_prepared_view`. The new test checks it over selection plus placement.

**dev_sync.** It adds `observations.stage_memory` under `observations` only, not under
`deterministic_work`. It does no I/O and has no failure path. Correct.

**cli-core / gmeow-cli / gmeow-dev-cli.**
- `init_tracing` now returns a typed `StageMemoryLogError` with `source()`.
- Both binaries report the error and exit nonzero. `gmeow` routes it through
  `error_diag` → `emit_and_exit`, which is why `error_diag` became `pub(crate)`.
- `gmeow-cli` depends on `gmeow-pipeline` (Cargo.toml:46), so wiring the variable into both
  binaries is consistent.
- The silent `try_init` failure applies only when a subscriber is already installed (tests). In a
  binary, `init_tracing` is the first thing called.

**Tests.** Each test names what it protects. None produces the corpus or runs the producer: all
use inline fixtures or synthetic scheduler stages.

| Test | Protects | Corpus/producer? |
|---|---|---|
| memory_record.tests `status_text_yields_resident_and_peak_or_absence` | absent vs zero in the procfs parser | no |
| memory_record.tests `phase_records_reach_the_enclosing_collection_and_only_it` | nested-collection isolation that the scheduler relies on | no |
| tests.rs additions to `carrier_retention_is_bounded_by_the_live_frontier` | one dispatch/publish record per stage, a release record with exact freed counts, none for the run output, seq order | no (synthetic stages) |
| tests.rs addition to `releasing_carriers_changes_no_byte_the_run_produces` | RetainAll makes no release record | no |
| gate_verdict.tests `gate_verdicts_over_a_composed_view_equal_those_over_its_materialization` | the waste removed in carrier assembly does not change verdicts | no (inline N-Quads) |
| reason.tests `expression_identity_edges_reach_the_closure_from_an_owned_edb` | EDB dropped before `reason-all`; edges still ship | no (inline TriG) |
| result_rdf/tests `transported_projection_matches_the_reencoding_path_byte_for_byte` | receipt byte identity; wrong literal refused | no |

The struct-literal additions to `acceptance_tests.rs` (`stage_memory: vec![]`) are field-completion
only. No test covers the file layer (OPTIONAL-1).

**Text.** `prose-check docs/RUST-OPTIMIZATION.md crates/pipeline/src/memory_record.rs` returned
EXIT=0, and `prose-check --message-file` on the b17c4b8e9 message returned EXIT=0. The scratch
copy of the message is at `.stage/reviews/.msg-b17c4b8e9.txt`. The added lines across the PR contain no
issue or PR numbers, TODO, or follow-up text. The docs state that a release is followed by no trim,
which matches scheduler.rs: `reclaim_allocator_slack` is called only at the serializer-permit
handoff (:1394-1396).

## D. The eight clippy `unnecessary_to_owned` errors

- **This branch did not change those files.** `git diff 2629810da HEAD` is empty for both. So is
  `git diff 2629810da origin/main` (14 commits on main since the merge base). The files were last
  changed on 2026-11-01 (06ddea3c3).
- **Callees.** The candidate lines call `carrier_entity_list()` and `object_level_entity_list()`
  (compile_logic.rs:129, :141, which return `Vec<String>` and are not in the PR diff) and the local
  helper `target_classes` (shape_union_fresh.tests.rs:16, which returns `Vec<String>`). The branch
  changes none of them, and neither file calls anything the branch changed (I grepped for every
  changed API).
- **Probable sites, exactly eight**, all `Vec<String>::contains(&"…".to_string())`:
  compile_logic.tests.rs:453 and :457; shape_union_fresh.tests.rs:60, :64, :69, :90, :95 and
  :122.
- **Probable cause: the toolchain, not the branch.** rust-toolchain.toml selects floating
  `nightly`, and the host forces `RUSTUP_TOOLCHAIN=nightly`. CI pins `nightly-2026-09-05`
  (ci.yml:87, :282, :592, :617, :664), and the pin is unchanged on both the branch and main. A
  newer local clippy that widened `unnecessary_to_owned` to cover `contains(&x.to_string())` would
  produce exactly these eight.
- **Not determinable without a build:** which clippy version emits them; whether CI's pinned
  clippy emits them (if it did, main would already be red on these unchanged lines); and the exact
  line-to-error mapping. All of these need the clippy output itself.

VERDICT: PASS (head b17c4b8e927c766b0a2e72ef70fd008728b17f92)
