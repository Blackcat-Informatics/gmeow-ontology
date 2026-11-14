VERDICT: BLOCKED

# Independent review of pull request 2755 at dbbd77181 (fix commit 76ce31834)

## Scope

- Head `dbbd77181`, a merge of main `04aa28a7e` into `76ce31834`. Fix commit `76ce31834` sits on the previously reviewed `85cef27fe`. Base main for the whole change: `04aa28a7e`.
- Everything was read from git objects (`git show`, `git diff 85cef27fe 76ce31834`, `git grep dbbd77181`). I read no working-tree file. I ran no build, test, `make`, `cargo` or fetch, so every statement about runtime behaviour is reasoned from source and labelled when inferred.
- Read: the earlier review (`review-2755-at-85cef27fe.md`), the three automated findings, the fix diff in full (24 files, +614 −163), `reads.rs`, `node.rs` (product accessors), `scheduler.rs` (dispatch, completion, `exec_stage`), `loader.rs` (`bind`, `BoundStage`), `run.rs`, the sink, medium-dictionaries, archive-blobs and carrier stage bodies that touch dependency products, the slice's pipeline module and docs, the two canonical documents, and the PR description.
- Line numbers are approximate where I quote from a diff hunk. Quoted code is verbatim.

## State of each earlier finding

| Finding | State at 76ce31834 | Evidence |
|---|---|---|
| N1, an undeclared carrier read on a released edge panics and hangs the coordinator | **Fixed**, with two residual follow-ups (F1) | `scheduler.rs`: the worker body is wrapped in `std::panic::catch_unwind(std::panic::AssertUnwindSafe(...))` and a panic becomes `StageFailed { message: "stage body panicked: …" }`, so the worker always reaches `tx.send((stage_id, resources, result, dispatched_edges))`. `node.rs`: `dataset()` and `dataset_arc()` now call `self.record(Read::Dataset)` before the `assert!`. The `Err(error)` arm now calls `crate::reads::undeclared_reads(&dispatched_edges)` under `Enforce` and pushes into `read_differences`. The end of `run_without` no longer does `scheduling_result?`; it keeps `scheduling_error` and appends `\n  the run also failed: {error}` to the read-difference report. Three tests (below). |
| N3, dead Rust retention overrides and tests that prove nothing | **Fixed** | Removed `GtsSinkStage::carrier_consumes` (and its field), `GtsSinkStage::internal_artifact_reads`, `MediumDictionariesStage::carrier_consumes` (and its field), `ValidateStage::consumes_span_table`. `grep` at head finds no `fn carrier_consumes`, `fn internal_artifact_reads` or `fn consumes_span_table` in `crates/pipeline/src/stages` outside test fakes. The four tests now read the bound stage (`crate::run::bound_production_stage`). |
| N4, false statements | **Partly fixed**; the fix added or left three false statements (R1 to R3) | Every item in the earlier "False statements" list was reworded and the new text is true, except where listed below. |
| N5, no check that the bound implementation is the stage it is bound to | **Fixed** | `loader.rs` `bind`: `if stage.id() != s.id { return Err(InvalidDeclaration …) }`, placed before the capability check. Test `bind_rejects_swapped_implementations`. |
| N2, peak memory | Not changed, as intended. Nothing in the fix raises it (see below). | |
| Automated finding 1, span-table readers counted by the coverage relation | **Fixed** | `loader.rs` `BoundStage::consumes_span_table` now answers `self.spec.reads.get(SPAN_TABLE_PRODUCER).is_some_and(\|reads\| crate::reads::permits(reads, &span_table))`. The scheduler's initial count (`stage.consumes_span_table()` in the `executed` loop) and its decrement (`let consumes_spans = stage.consumes_span_table();`) call the same method, and `strip_rep_blob` uses the shared `SPAN_TABLE_PRODUCER` constant. Strip runs before the carrier release in the same commit, so a counted whole-carrier reader is also a carrier reader and cannot find a stripped table. Test `a_whole_carrier_declaration_counts_as_a_span_table_reader`. |
| Automated finding 2, all-artifacts must not silently deliver a partial lane | **Fixed** with the fail-closed contract chosen; no missed edge found (section C); one wording defect (R3) | `reads.rs` `covers`: `(Self::AllArtifacts, Self::Artifact(path)) => !is_internal_artifact(path)`. `node.rs` `lane_visible` hides an unnamed internal artifact from `artifacts()` and `artifact_refs()` and records the named ones. `internal_artifacts()` still keeps only named paths. `docs/PIPELINE_SPINE.md`, `docs/GATE-AND-PIPELINE.md` and the slice definitions of `laneAllArtifacts` and `flowReadsArtifact` were rewritten to match. |
| Automated finding 3, author new slice terms with `logic:` | **Fixed** | `git diff 05385cf3b dbbd77181 -- slices/core/pipeline/module.ttl` has no added line containing `rdfs:subClassOf`, `rdfs:domain`, `rdfs:range`, `rdfs:subPropertyOf` or `owl:`. `ProductLane` uses `logic:subClassOf`; the four `flowReads*` properties use `logic:domain`; `flowReadsLane` uses `logic:range`. |
| Automated finding 4 (the hang) | **Fixed** by the reviewer's first alternative (convert the unwind to a `StageFailed`) | As N1. |

## A. Is each fix correct in every path, is it tested, would the test fail on the old code

**N1 and the hang** (tests in `tests.rs`):

- `a_panicking_stage_fails_the_run_instead_of_hanging_it`: on the old code the worker closure panics, its cloned `tx` drops, the coordinator's own `tx` keeps the channel open, and `rx.recv()` never returns. The test would hang until the nextest slow-timeout terminates it. That is a failure, though not a clean one.
- `an_undeclared_dataset_read_of_a_released_product_is_reported`: on the old code the assertion fired before `record`, so nothing was recorded and the run hung or panicked. It asserts the exact line `a read the dataset of source but its dataflow edge does not declare it; declare \`gmeow:flowReadsLane gmeow:laneDataset\``, which matches the `undeclared_reads` format string and `Read::Dataset`'s `Display` ("the dataset"). The stage under test is bound through `bind`, so `BoundStage` derives no carrier input from an `Identity` edge and the product is released at dispatch (`whole` is false). The test does exercise the released path.
- `a_failing_stage_still_reports_its_undeclared_reads`: on the old code the `Err` arm dropped the edges, so the handle IRI is absent from the text. It fails on old code. It asserts both the read and the stage's own message.
- `MisbehavingStage` binds: capabilities, resources, stability and cache disposition equal the trait defaults and the `spec()` fixture.

**N5**: `bind_rejects_swapped_implementations` registers `fake("b")` under `impl:a`. Without the check `bind` succeeds and `expect_err` panics, so the test fails on the old code.

**Span table**: `a_whole_carrier_declaration_counts_as_a_span_table_reader` asserts `counted(Read::Carrier)`. The old `reads.contains(&span_table)` returns false for `Carrier`, so it fails on the old code. It tests the predicate, not a scheduler run; I consider that enough because both scheduler call sites use the predicate.

**All-artifacts**: `an_unnamed_internal_artifact_is_undeclared_under_the_whole_lane` fails on the old code (`permits` would be true; `artifact_refs()` would return three paths). `a_named_internal_artifact_is_kept_and_readable_beside_the_whole_lane` would also pass on the old code. It protects named-artifact retention and recording, not the fix. That is acceptable.

**Path-by-path residuals for N1**: see section B.

## B. N1 in depth: every way a stage body can end

All rows assume `read_policy == Enforce` unless stated. `active` is decremented, and `active_resources` released, immediately after `rx.recv()` returns and before the result is inspected, so every row below decrements it.

| Ending | Coordinator receives a message | Resources released | Recorded reads reported | Kept out of persistent cache | Hang or false success |
|---|---|---|---|---|---|
| `Ok`, reads equal declarations | Yes | Yes (guards dropped by `drop(_guards)`) | Nothing to report | Cached normally | No |
| `Ok`, reads differ | Yes | Yes | Yes (`mismatches`, both directions) | Yes (`!reads_differ` guards `cache.put`) | Run fails at the end |
| `Err` | Yes | Yes (`?` drops guards without poisoning) | Undeclared reads only, with the stage's own error appended | No product exists | Run fails |
| Panic in the body (no lock held) | Yes, as an `Err(StageFailed)` through `catch_unwind` | Yes | Undeclared reads recorded before the panic, as for `Err` | No product exists | Run fails; cannot report success because `first_error` is set |
| Panic while holding a resource lock | Yes | The guard drops during unwinding, so the `Mutex<()>` is poisoned. Later `lock()` calls return an `Err` that `exec_stage` maps to `StageFailed("resource lock … poisoned")`. A waiting sibling is woken with that error, so no deadlock. | As above | As above | No hang. The poisoned lock is a process-wide `static`; it stays poisoned for later runs in the same process (F1). Before the fix the panic poisoned it too. |
| Panic inside a nested rayon scope or `par_iter` in the body | Yes. Rayon re-raises it in the thread that opened the scope, which is inside `catch_unwind`. | Yes | As above | As above | No |
| Panic in a thread the body spawned with `std::thread::spawn` | Whatever the body does with `join()`; the body's own panic or error then follows a row above | Yes | As above | As above | No |
| Panic in a detached `rayon::spawn` inside the body | No `StageFailed`. With no `panic_handler` rayon aborts the process. | n/a | n/a | n/a | Loud abort, not a hang. I found no such call in `crates/pipeline/src/stages` by reading, but did not search for it (inferred, unchecked). |
| Cache hit | Yes, with `edges: Vec::new()` and `cached: true`, so nothing is compared. The `Err` arm for a failing hit (`verify_attach_drift`, `validate_hit_receipt`) finds no recorded reads. | Not acquired | Nothing recorded | Hit is already cached | No |
| Panic in `progress.stage_end` or `Duration::from_millis` after `catch_unwind` | No: this code is outside the catch. | Never released | No | n/a | **Still hangs** (F1). Pre-existing; not a stage body. |
| Cancellation | There is no cancellation mechanism in the scheduler. | | | | |
| `commit_result` fails in the coordinator | n/a | Already released | The stage's reads were already pushed | The `cache.put` failure is itself the error | `first_error` set; run fails |

**`AssertUnwindSafe`.** What the closure captures is `&dyn Stage` (immutable), `root`, `&upstream` (consumer views whose only interior mutability is `EdgeAccess::observed`, a `Mutex`), `cache: &PipelineCache` and a bool. `PipelineCache` has no `Mutex`, `RwLock`, `static`, `thread_local` or `OnceLock` (`grep` of `cache.rs`), and `cache.get` only reads; `cache.put` runs on the coordinator thread, outside the catch. `products`, `read_differences`, `stage_receipts` and the cache writes are coordinator state that a worker never touches. A panic inside `EdgeAccess::record` could poison `observed`, but every access uses `.unwrap_or_else(PoisonError::into_inner)`. A panic mid-way through a stage leaves no product to publish, so no half-updated map or cache entry can result. The one shared state that stays damaged is process-global state a stage body or a dependency owns, for example a poisoned global `Mutex`. Any later user of it panics, and that panic is caught the same way, so the run still terminates with an error. I judge the `AssertUnwindSafe` sound for this scheduler.

**The final error.** When `read_differences` is non-empty and a stage also failed, `run_without` returns one `StageFailed` whose message is `"{n} stage(s) read differently from their declared dataflow ({total} difference(s)):\n  {listing}\n  the run also failed: {error}"`. Both parts are present. Two caveats: only `first_error` is kept, and a typed inner error becomes text inside a `StageFailed` (F7). I did not check that `Diag`'s `Display` prints the message; the new test `a_failing_stage_still_reports_its_undeclared_reads` assumes it does.

**Ordering.** `read_differences` is sorted by `(stage, lines)` before printing, so the new `Err`-arm pushes do not change the order of reports.

## C. The all-artifacts contract in depth

I traced `reads.rs` (`covers`, `permits`, `internal_artifacts`), `node.rs` (`artifacts()`, `artifact_refs()`, `lane_visible`), `loader.rs` (`BoundStage::internal_artifact_reads`, `BoundStage::new`), `scheduler.rs` (`kept_internal`, `kept_for`, dispatch-time `into_carrier_released_keeping`, completion-time release) and the slice. They agree:

- `covers`: `AllArtifacts` permits an `Artifact(path)` only when `path` is not internal. An internal artifact is permitted only by a declaration naming that exact path (`declared == access`).
- `internal_artifacts` returns only named `pipeline/` paths. `BoundStage::internal_artifact_reads` flattens them; the scheduler's `kept_internal` unions them over every executed stage and applies the union at every release of that producer.
- `lane_visible` applies the same test (`edge.declared().contains(&Read::Artifact(path))`) and records the exact read.

**Search for a missed edge.** I enumerated every producer of a `pipeline/`-prefixed artifact (constants and strings listed by `git grep`) and every consumer of one:

| Producer | Internal artifacts that another stage reads | Edge | Declared? |
|---|---|---|---|
| compile-logic | `pipeline/canon-proof-admission.json` | sink | Yes |
| conformance | `pipeline/prepared-operator-rules.json`, `pipeline/mcp-action-policy.json` | sink | Yes |
| conformance | `pipeline/agreement-tallies.json` | export-agreement | Yes |
| verify-attestation | `pipeline/prepared-verify-gates.json` | sink | Yes |
| statements | `pipeline/statements/gmeow.rdf12.jsonld`, `…yamlld` | archive-blobs | Yes (`.artifact()` and the whole lane) |
| medium-dictionaries | the six `pipeline/medium/*.zdict` | sink | **Yes (the fix)** |

- The six names equal the six `gmeow:dictionaryId` values in `slices/core/gts/module.ttl` (core, lang-ast, logic, memory-compact, memory-hot, prooftrace).
- `medium_plan_under` refuses a declared dictionary with no trained bytes. A seventh dictionary that the edge does not name therefore fails the sink closed, not silently (F4).
- No other stage reads any other internal path through a dependency: the remaining `pipeline/…` constants (`base-graph.nq`, `slice-quality-report.html`, `documentation.nq`, `conformance-divergence.nq`, every conformance `CHANNEL`, the `*.purrpack` channels, `metadata-*.purrpack`) are read by no stage body. Their readers are `fixture.rs` and `run.rs`, which read owner views after the run, or the producer itself.
- Every non-test caller of `.artifacts()` or `.artifact_refs()` on a dependency (`archive_blobs.rs` 937 to 1026, `carrier.rs` 1286 and 2354, `medium_dictionaries.rs` 294, `medium/corpus.rs` 201, 344, 630) filters by committed prefix, by a fixed list (`AXIOM_FILES`), by `is_rdf_member`/`is_internal_lane_path`, or by a `gmeow:corpusSelectsPathPrefix` under `generated/` or `slices/`. None selects a `pipeline/` path. No shipped corpus uses `corpusSelectsStageProduct`, the one selector that takes a whole product lane.
- Every computed-path `.artifact(path)` call (carrier, docs-render, mappings, pydantic, research-objects, shape-union-fresh, verify) takes committed paths.

I found no edge the fix missed. I could not run the stage bodies, and I did not search test-support files (`*_test_support.rs`).

**Output identity.** Hiding unnamed internal artifacts from whole-lane readers changes nothing the readers use. At main, an artifact-only consumer already received a release that dropped internals. A carrier-held consumer received them, but none of the readers above uses them. I infer an unchanged output; it is unproven without a run.

## D. Everything the fix commit removed or weakened

| Removed or changed | Replaced by | At least as strong? |
|---|---|---|
| `GtsSinkStage::carrier_consumes` and `internal_artifact_reads`, `MediumDictionariesStage::carrier_consumes`, `ValidateStage::consumes_span_table` | Derivation from the slice in `BoundStage`; the four tests now read the bound stage | Stronger: a slice edit now fails a test, which the Rust twin could not do |
| `AllArtifacts` covering internal artifacts | Fail-closed `covers`, `lane_visible`, new tests | Stronger |
| `scheduling_result?` early return | Report differences with the run's own error | Stronger; the error kind changes (F7) |
| "closed set" in `ProductLane`, and "fail the stage before its product is published" in `flowReadsLane` | True wording | Truthful, and nothing was enforced by the old text |
| `sink_declares_only_the_lanes_it_reads_as_carrier_inputs`, `the_sink_declares_every_internal_artifact_its_reasoning_archive_reads`, `the_consumes_set_is_sorted_and_bound`, `term_panels_follow_the_actual_documentation_stage_dependencies` (re-pointed) | The same assertions on `bound_production_stage(..)` | See below |

**The re-pointed tests and the helper.** `bound_production_stage` is `#[cfg(test)] pub(crate)` in `run.rs`. It runs `full_spec()`, `validate()`, and `bind(&spec, &graph, &default_registry())`, then finds the stage by id. Production binds through `registry_for(profile)` and `spec_for_scope(full_spec(), scope)`. Retention depends only on the spec's reads, entities and consumes, which neither the profile nor the scope changes, so the helper exercises the `BoundStage::new` derivation that production runs. The difference between production and the helper is the registry profile and the scope pruning, and neither affects the derived sets. `the_sink_declares_every_internal_artifact…` takes the declared set from the sink only, while the scheduler unions every consumer's declarations, so the test is stricter than production. Removing one `gmeow:flowReadsArtifact` of the sink from the slice makes `build_reasoning_blob` fail in that test. I count all four as protecting named production behaviour.

## E. New defects introduced by the fix commit

No logic error found. Behaviour changes that the commit does not state in its own text, all minor:

1. The returned error is now a `StageFailed` wrapper whenever a read difference exists, so a typed inner error is stringified (F7).
2. `REASONING_INTERNAL_READS` became `#[cfg(test)]` (correct; its only user is a test).
3. `undeclared_reads` and `is_internal_artifact` are `pub` in a `pub mod reads`; `pub(crate)` would suffice for the former (F6).
4. The three false statements R1 to R3 below.

## F. The merge commit dbbd77181

A plain merge of main with no content of its own. Main's change since the merge base `05385cf3b` is `git diff 05385cf3b 04aa28a7e`: two files (`crates/gmeow-cli/src/commands/prove.rs`, four lines; `helpers-census-debt.json`, one line removed). `git diff 76ce31834 dbbd77181` is byte-identical to that diff. The branch touches neither file (`git diff --name-only 05385cf3b 76ce31834` on both paths is empty), so a conflict-free merge produces exactly this tree.

## G. False or overstated statements that remain or were added

**Required (R1 to R3):**

**R1. `crates/pipeline/src/stages/archive_blobs.rs`, about line 709, added by the fix commit, names the wrong declaration.**

```rust
"the {STAGE_ID} carrier was released before `{rep}` was read; the consumer's \
 dataflow edge from {STAGE_ID} in the pipeline slice must declare a carrier read \
 (`gmeow:flowReadsBlobRep \"{rep}\"`)"
```

The next line reads the product with `archive_bytes(product.bundle(), rep)`, and `bundle()` records `Read::Carrier`. `covers(BlobRep(rep), Carrier)` is false (`declared == access`), so a consumer that follows the advice and declares `gmeow:flowReadsBlobRep "<rep>"` still has its carrier read reported undeclared, and its declaration reported as never exercised. The correct declaration, and the one the slice uses on `medium-dictionaries←archive-blobs`, is `gmeow:flowReadsLane gmeow:laneCarrier`. This message is the only guidance in this path, because it returns before `bundle()` records anything. Failing case: delete `gmeow:flowReadsLane gmeow:laneCarrier` from `gmeow:pipeline-dataflow-medium-dictionaries-archive-blobs`; the run fails with this message and the advice does not fix it. The old text ("declare in carrier_consumes()") was also false, so the commit replaced one false instruction with another.

**R2. The recording policy no longer "withholds nothing", but three comments and the maintainer docs still say it does.**

- `scheduler.rs`, dispatch: "Recording learns what a stage body reads, so it withholds nothing: every consumed product is handed over whole."
- `reads.rs`, `ReadPolicy::Record`: "Reads are recorded and reported, and nothing is refused."
- `reads.rs`, `record_run`: "nothing is refused, every consumed product is handed over whole, and the output is the declaration the stage bodies need."
- `docs/GATE-AND-PIPELINE.md` item 4: "`gmeow-dev pipeline-reads --out <file>` writes the declarations a tree's stage bodies need", and the slice's `flowReadsLane` `gmeow:howToUse`: "writes the exact declarations".

`lane_visible` does not consult the policy, so under `Record` the whole-lane accessors still drop every unnamed internal artifact. A stage that reaches an internal artifact through the whole lane then gets a lane without it and fails with its own error, and the tool never records the `flowReadsArtifact` it would have emitted. Failing case (inferred): delete the six `flowReadsArtifact` lines from `gmeow:pipeline-dataflow-gts-sink-medium-dictionaries` and run `gmeow-dev pipeline-reads --out f`. The sink fails with "declared but no trained bytes were supplied" and writes nothing. The functional gap is F3; the false statements are required.

**R3. The definition of `gmeow:laneAllArtifacts` and the doc of `StageProduct::artifacts` say retention does not keep an unnamed internal artifact.**

- Slice: "an unnamed internal artifact is neither kept through the release of the producer's carrier nor returned by the whole-lane accessors".
- `node.rs`: "an unnamed internal artifact is not part of the lane the consumer is permitted to read, and retention does not keep it".

Retention is per run (`kept_for(producer)` is the union over all consumers), not per edge. Production case: compile-logic→archive-blobs declares only `laneAllArtifacts`, while compile-logic→sink names `pipeline/canon-proof-admission.json`. The artifact is therefore kept through the dispatch-time release that builds archive-blobs' view; only the view hides it. The `flowReadsArtifact` definition, `reads.rs` and `PIPELINE_SPINE.md` state it correctly ("kept … until the run ends, whether or not the declaring consumer has run"), so the same slice now holds two statements that disagree. Fix: say "is not returned by the whole-lane accessors for that consumer, and is kept only when some other consumer in the run names it".

**Optional or follow-up wording (not blocking):**

- `Stage::carrier_consumes` doc: "A read of a released carrier fails at the read, through `StageProduct::carrier_released`". Only `dataset()` and `dataset_arc()` assert. `handle()` and `rep_blob()` return `None`, and `bundle()` returns the released (empty) bundle. The run still fails later, from the recorded undeclared read. The same imprecision was in the main-line text.
- `flowReadsBlobRep`: "once the last stage that declares it has run". The counted readers are those whose declaration permits the read, including `laneCarrier`.
- `CarrierRetention::DropAfterLastConsumer` and `bundle.rs` `release_carrier` docs describe internal artifacts as freed "as soon as no stage can still read them"; declared ones are kept to the end of the run. This predates the change.
- The new properties are not described in `slices/core/pipeline/docs.md`.

## Required findings, most serious first

All three are false statements, and none changes execution on its own. I found no regression in behaviour or output, no refusal removed without a stronger one, no hang that remains for a stage body, and no test that cannot fail.

1. **R1** `crates/pipeline/src/stages/archive_blobs.rs` ≈709: the released-carrier message tells the reader to declare `gmeow:flowReadsBlobRep`, which does not permit the `bundle()` read that follows. Correct: `gmeow:flowReadsLane gmeow:laneCarrier`.
2. **R2** `scheduler.rs` (dispatch comment "it withholds nothing"), `reads.rs` (`ReadPolicy::Record`, `record_run`), `docs/GATE-AND-PIPELINE.md` and the `flowReadsLane` `howToUse`: they claim the recorder hands over whole products and writes the exact declarations; the whole-lane accessors still hide unnamed internal artifacts under `Record`.
3. **R3** slice `gmeow:laneAllArtifacts` definition and `node.rs` `StageProduct::artifacts` doc: "retention does not keep" an unnamed internal artifact is false when another consumer names it (compile-logic→sink and compile-logic→archive-blobs).

## Follow-ups (file as issues; none blocks)

- **F1. Two residual stall or poison paths.**
  - A panic in `progress.stage_end` is outside the catch, so the worker never sends and the coordinator (which holds its own sender) waits for ever. Wrapping the whole worker closure, or sending from a drop guard, closes it.
  - After a caught panic that held a resource lock, the process-wide `RESOURCE_LOCKS` entry stays poisoned. Its payload is `()`, so recovering with `PoisonError::into_inner` would be safe. It only matters to a process that runs more than one pipeline.
- **F2. Dispatch-time release copies more than the consumer can see.** `into_carrier_released_keeping(&kept_for(producer))` deep-copies every internal artifact any consumer names into each artifact-only consumer's view, although `lane_visible` now hides all but that edge's own names. Passing the edge's own named set would shrink the per-consumer copy (statements' two JSON-LD-star renderings are the large case). This bears on the open peak-memory finding; measure first.
- **F3. The recorder cannot discover an internal artifact reached through the whole lane** (R2's functional gap). `pipeline-reads` would need `lane_visible` to show everything under `Record`, and to record each internal path it returns.
- **F4. Dictionary names are authored by hand in the slice.** Nothing ties them to `gmeow:dictionaryId` values. A seventh dictionary fails closed at the sink (`medium_plan_under`), but the message does not point at the slice edge, and no read difference is reported because `AllArtifacts` is declared. A test pairing the six `flowReadsArtifact` strings with the registry would close it.
- **F5. Test gaps.** No scheduler-level test that the span table survives until a whole-carrier reader runs. No test that the `parse_edge_reads` refusals fire (the N8 item of the earlier review, unchanged). The positive half of the all-artifacts pair also passes on old code.
- **F6. Public surface.** `reads::undeclared_reads` can be `pub(crate)`.
- **F7. Error kind.** When a read difference exists, the run's typed error is wrapped as text. A caller that matches on the error kind of a failing stage sees `StageFailed` instead.
- **F8. Earlier lower-priority findings.** N6 (partial lane) is fixed by the all-artifacts contract and N7 (differences lost when a stage fails) by the deferred `scheduling_error`; N8 (untested refusals) and N9 (inaccurate fake comments) are unchanged. D1 to D12 of the earlier review are unaffected by this commit; D2, D3 and D12 still gate the merge by the PR's own conditions.

## Peak memory (N2): could anything in the fix commit change it against 85cef27fe

I find nothing that raises it, and nothing that lowers it by an amount worth stating.

- `lane_visible` filters after the existing clone (`artifacts()`) or on the borrowed map (`artifact_refs()`): no extra allocation, and a slightly smaller returned map.
- `dispatched_edges` is a `Vec<Arc<EdgeAccess>>`, a few hundred bytes per dispatch.
- `catch_unwind` allocates nothing on the success path.
- The six dictionary names added to `kept_internal["stage-medium-dictionaries"]` keep the six dictionary byte strings in medium-dictionaries' released product until the run ends. Before the sink completes that product was already whole (the edge reads the dataset); afterwards it adds the dictionaries' size once. I have not measured it, and each is the size of one trained dictionary.
- The span-table count only changes for a consumer of `stage-source-load` that declares `laneCarrier`; the slice has none (`validate` names the blob, the other four source-load consumers declare `laneDataset` or `laneIdentity`), so the strip point is unchanged.
- The dispatch-time deep copies and the statements renderings kept to the end of the run, which the earlier review identified, are untouched.

## Tests

Added by the fix (all can fail on the named behaviour, section A): `bind_rejects_swapped_implementations`, `a_whole_carrier_declaration_counts_as_a_span_table_reader`, `an_undeclared_dataset_read_of_a_released_product_is_reported`, `a_failing_stage_still_reports_its_undeclared_reads`, `a_panicking_stage_fails_the_run_instead_of_hanging_it`, `a_named_internal_artifact_is_kept_and_readable_beside_the_whole_lane`, `an_unnamed_internal_artifact_is_undeclared_under_the_whole_lane`. The four re-pointed tests protect what production runs (section D). The three N1 tests reach the real scheduler path through `run()` with `open_uncached`; two would hang rather than fail cleanly on the old code. No volume padding.

## Stale statements in the pull request description

1. "The first commit on this branch after `main` is the exact inverse of the revert … the diff against `main` is that change and nothing else. Any later commit on this branch is a fix … listed under 'Changes made after the restore'". The branch now also carries `76ce31834` (26 files of fixes: identity check, panic handling, retention derivation, the all-artifacts contract, the six dictionary reads, text corrections) and the merge `dbbd77181`. "Changes made after the restore" lists only the `empty_tiers()` change.
2. "Verification … The restored tree was compared with the tree merged for #2752: identical." True of the restore commit only; the head differs by design.
3. "Under the enforcing policy a difference fails the run and the stage's product is not written to the stage cache." Still true, but incomplete: a stage that fails or panics is now also reported for its undeclared reads, and the run's own error is kept beside the differences. The list of what the change does not show should say that a stage served from the stage cache, and a read made through a whole-lane accessor of an unnamed internal artifact, are still outside the comparison.
4. "The change removes two bind-time tests … and replaces the test that compared the slice with an independent stage list by one that compares the slice file with its own embedded copy." Still true, but the commit also removes Rust retention overrides and re-points four tests, and adds an identity check; none of this is stated.
5. "Known failure this change introduces … `helpers` … It is fixed on this branch", and "At the new head `make helpers-hygiene` no longer prints the `6cd0ef3c0fdc0d8d` line; it still exits non-zero on the three lines that are on `main`, which #2762 fixes." Main moved: `04aa28a7e` ("Rename the prove world authority and shrink the Default shim debt") is now merged and removes one `helpers-census-debt.json` row. Whether those "three lines" still exist on the merged head is not shown; the sentence must be re-measured or removed.
6. "Not yet run … Strict clippy and the focused tests of `gmeow-pipeline` and `gmeow-dev-cli`." If they have since been run for `dbbd77181`, the list is stale; if not, it is still accurate and still blocks the merge by the PR's own conditions.
7. "A declared whole-carrier read is satisfied by any single carrier read, so an over-declared edge passes" is still true. "One product accessor for diagnostic nodes records no read" is still true.
8. The body does not mention the whole-lane contract (committed artifacts only; an internal artifact only when named), which is a user-visible rule for anyone authoring an edge.

## Not determined

- Whether the crate compiles, whether any test passes, and strict clippy at this head: I ran nothing. In particular I could not confirm that `Diag`'s `Display` prints the stage message that the new tests assert on.
- Whether every existing test registers its implementation under its own stage id. `bind_rejects_swapped_implementations` shows the check works; it would make any test that registers an implementation under a different id fail. The helpers I read (`fake`, `retention_registry`, `CarrierStage`) comply. I did not read every test fake.
- Whether the new `logic:domain`/`logic:range` rows and the new definitions pass the projection-vocabulary ratchet, the annotation gates and the i18n coverage gates (`fr.po` and `zh.po` have no rows for the new terms). `make check` was not run.
- `prose-check` on the changed definitions, comments and docs: I cannot run it without writing a file.
- Output identity against main (D3 of the earlier review, the 43,407 against 43,357 findings), the full-scope enforcing pass from an empty cache (D2), and peak memory of that pass (N2): all still unmeasured.
- Whether a stage body anywhere in `crates/pipeline/src/stages` calls a detached `rayon::spawn` (the one panic row that aborts); I did not grep for it.
- Test-support files (`*_test_support.rs`) were not searched for internal reads through dependencies.
- Whether the "#2762" named in the description is the commit now on main.
