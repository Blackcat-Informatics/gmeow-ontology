15 false or misleading statements found at 68272e7e4 (2 already reported, 13 new). Line numbers are at 68272e7e4. Read-only, git objects and the worktree checkout at that revision, nothing built or run.

Method: every added or changed comment, doc comment, human-read string, test name, test comment, document sentence and slice annotation in `git diff 04aa28a7e 68272e7e4` was checked against the function it describes, not against other comments. The 6 `flowReasonAuthority` edges, the 6 `laneIdentity` edges, the `pipeline-build` edge list and the per-edge `skos:definition` texts were checked against the declared reads one by one.

## The two findings already reported

### 1. `crates/pipeline/src/reads.rs:237-238` (`ReadPolicy::Enforce`)
- Quoted: "A stage that reads what its edges do not declare, or does not read what they declare, fails and publishes nothing. Every production run."
- Code: `scheduler.rs` `commit_result` (about 850-960) still runs `products.insert(run.id, run.product)` and readies dependents. Only `cache.put` is skipped (`!reads_differ`), and the run fails at the end. The slice `flowReadsLane` definition (module.ttl:~263) already says this correctly.
- Corrected: "A stage that reads what its edges do not declare, or does not read what they declare, fails the run at its end; its product is still handed to later stages but is not written to the stage cache. Every production run."

### 2. `crates/pipeline/src/run.bound_stage_tests.rs:6-7`
- Quoted: "the same instance the scheduler runs"
- Code: `bound_production_stage` calls `bind(&full_spec(), ..., &default_registry())` and creates a fresh binding. The run binds under `registry_for(profile)` and `spec_for_scope`.
- Corrected: "The implementation the authored pipeline binds to stage `id` under the default registry, so a test reads the declarations the production stage reports."

## Further findings

### 3. `crates/pipeline/src/reads.rs:240` (`ReadPolicy::Record`)
- Quoted: "Reads are recorded and reported, and nothing is refused."
- Code: `scheduler.rs` computes mismatches only `if !mismatches.is_empty() && read_policy == ReadPolicy::Enforce`, and the "declares no read" pre-check is also `Enforce`-only. Under `Record` nothing is reported; the reads are only kept in `RunResult::recorded_reads`.
- Corrected: "Reads are recorded and returned in `RunResult::recorded_reads`; no difference is reported and nothing is refused."

### 4. `crates/pipeline/src/reads.tests.rs:14`
- Quoted: "the whole artifact lane permits any artifact and no carrier lane."
- Code: `Read::covers`, `(AllArtifacts, Artifact(path)) => !is_internal_artifact(path)`. The lane permits committed artifacts only. The next test in the same file asserts this.
- Corrected: "the whole artifact lane permits any committed artifact and no carrier lane."

### 5. `crates/pipeline/src/node.rs:716` (`Stage::carrier_consumes`)
- Quoted: "A read of a released carrier fails at the read, through [`StageProduct::carrier_released`]."
- Code: only `dataset()` and `dataset_arc()` assert on `carrier_released`. `handle()` and `rep_blob()` on a released product return `None`, and `artifact()` of an internal artifact returns `None`. The PR's own test doc (`tests.rs`, `a_failing_stage_still_reports_its_undeclared_reads`) says "A handle read of a released product returns nothing."
- Corrected: "A dataset read of a released carrier panics at the read, through [`StageProduct::carrier_released`]; a handle or blob read returns nothing."

### 6. `crates/pipeline/src/node.rs:721-722` (`Stage::internal_artifact_reads`)
- Quoted: "The internal `pipeline/` artifacts this stage reads through dependencies whose carrier is released for it".
- Code: `BoundStage::internal_artifact_reads` (`loader.rs:764`) returns every `gmeow:flowReadsArtifact` under `pipeline/` on any edge. `gts-sink` from `medium-dictionaries` declares six internal artifacts and the carrier lane `laneDataset`, so its carrier is kept and `carrier_consumes()` includes that producer (asserted in `gts_sink.tests.rs`).
- Corrected: "The internal `pipeline/` artifacts this stage reads, as `(producer, artifact)` pairs, whether or not the dependency's carrier is released for it."
- Made false by the same change, not edited: the `scheduler.rs:620-621` comment (see the list below).

### 7. `slices/core/pipeline/module.ttl:240` (`ProductLane` `gmeow:avoidWhen`)
- Quoted: "Do not declare gmeow:laneCarrier where the consumer reads one handle or one representation blob: the wider declaration keeps the whole carrier resident for that consumer."
- Code: `Read::needs_carrier` is true for `Handle` and `BlobRep` as well. A handle or blob declaration keeps the whole carrier resident exactly as `laneCarrier` does. The slice's own `flowReadsHandle` definition says "A handle lives on the producer's carrier, so the carrier stays resident until this consumer has run." The two statements contradict each other.
- Corrected: "Do not declare gmeow:laneCarrier where the consumer reads one handle or one representation blob: name it with gmeow:flowReadsHandle or gmeow:flowReadsBlobRep, so the run fails the stage if it reads anything wider."

### 8. "every lane access is recorded"
- Places:
  - `node.rs:179`: "whose every lane access is recorded against the edge's declared read set"
  - `node.rs:323`: "with every lane access recorded against `access`"
  - `reads.rs:9`: "checks every access against the declaration and records it"
  - `slices/core/pipeline/module.ttl` `flowReadsLane` definition: "records every lane access"
  - `scheduler.rs` comment at about 755: "records every lane access"
- Code: `StageProduct::diag_nodes()` (`node.rs:466`) reads the `diagnostics:nodes` blob lane with no `record`, and `bundle_rep_blob` is called directly. The owning accessors also record without checking: `EdgeAccess::record` "never refused here", and the comparison happens when the stage returns.
- Corrected, one wording for all: "records each access made through the lane accessors (`bundle`, `handle`, `rep_blob`, `dataset`, `dataset_arc`, `artifact`, `artifacts`, `artifact_refs`, `span_index`) and compares them with the declaration when the stage returns."

### 9. `crates/pipeline/src/node.rs:155` (`bundle` field)
- Quoted: "No stage does today."
- Code: the test stage `CarrierStage::run` in `crates/pipeline/src/tests.rs` (about 2653 and 2672) calls `up.raw_bundle()` to inspect a product without a record. It is a `Stage` in this crate.
- Corrected: "No production stage does today."

### 10. `crates/gmeow-dev-cli/src/dev_pipeline_reads.rs:10`
- Quoted: "writes the Turtle that declares exactly what each stage body read."
- Code: `reads.rs:314-322` states the exception (an internal artifact reached only through the whole artifact lane is not discovered and must be named by hand), and the PR's `GATE-AND-PIPELINE.md` item 4 and the `flowReadsLane` `howToUse` say the same. This module doc contradicts them.
- Corrected: "writes the Turtle that declares what each stage body read, except an internal artifact it reaches only through the whole artifact lane, which must be named by hand."

### 11. `crates/pipeline/src/scheduler.rs:162-163` (`RunContext::read_policy`)
- Quoted: "only the maintainer command that prints observed read sets selects `Record`."
- Code: `dev_pipeline_reads::run` writes the Turtle to `--out` and prints only a "wrote {path}" line.
- Corrected: "only the maintainer command that writes observed read sets to a file selects `Record`."

### 12. "every byte artifact" for `laneAllArtifacts`
- Places:
  - `reads.rs:39` (`Read::AllArtifacts`): "The whole byte-artifact lane."
  - `reads.rs:107`: the `Display` text "every byte artifact"
  - 28 per-edge `skos:definition`s in `module.ttl`, the first at 662 ("archive-blobs reads every byte artifact of the producer's product"). Another one at 241 says "every committed byte artifact".
- Code: the lane covers committed artifacts only (`covers`, `lane_visible`), and the slice's own `laneAllArtifacts` definition (module.ttl:262) says "COMMITTED artifacts only". An edge text saying "every byte artifact" claims more than the declaration can read.
- Corrected: "every committed byte artifact" in all of them, and `reads.rs:39` "The whole committed byte-artifact lane."

### 13. `slices/core/pipeline/module.ttl:301` (`flowReadsBlobRep`)
- Quoted: "once the last stage that declares it has run".
- Code: `BoundStage::consumes_span_table` counts a stage as a reader when its edge `permits` the span-table read, and the `Carrier` lane permits it. A stage that declares only `laneCarrier` and calls `span_index()` holds the strip back. `node.rs` states this correctly.
- Corrected: "once the last stage whose edge permits it (by naming it or by declaring the whole carrier) has run".

### 14. `crates/pipeline/src/stages/carrier_rooting_tests.rs:668`
- Quoted: "Removing a `gmeow:flowReadsArtifact` of the sink from the slice fails here."
- Code: the test builds only the four `REASONING_INTERNAL_MEMBERS` artifacts and three `stage-reason` committed paths. Removing any of the sink's six `pipeline/medium/*.zdict` declarations leaves it green.
- Corrected: "Removing a `gmeow:flowReadsArtifact` of the sink that the reasoning archive folds fails here."

### 15. `crates/pipeline/src/loader.rs:75-76` (`StageSpec::reads`)
- Quoted: "Retention of a product's lanes is derived from the declarations of the consumers still to run."
- Code: `kept_internal` in `scheduler.rs` (about 622-627) is the union over every executed stage, so an internal artifact is kept "whether or not that consumer has run" (as `node.rs:733` and the slice say). Only carrier retention counts the consumers still to run (`remaining_carrier_readers`).
- Corrected: "Retention is derived from the declarations: a carrier is kept until its last declared carrier reader has run, and a declared internal artifact is kept through every release until the run ends."

## Statements already on main, not changed here, that are also false (or made false by this PR)
- `crates/pipeline/src/scheduler.rs:620-621`: "The internal artifacts each producer must keep through every release, because an artifact-only consumer declared it reads them." The adjacent code is changed by this PR. The set now covers every declaring consumer, including carrier readers (`kept_internal` is built from every stage's `internal_artifact_reads()`).
- `crates/pipeline/src/bundle.rs:161-162`: "`kept_internal` is the set of internal artifacts that artifact-only consumers declare they read". Same cause, same correction ("that consumers declare they read").

## Files covered (statements checked per file; 40 files in the diff)
- `crates/gmeow-dev-cli/src/dev_pipeline_reads.rs` 9
- `crates/gmeow-dev-cli/src/dev_producer.rs` 0 (no text)
- `crates/gmeow-dev-cli/src/lib.rs` 4 (the `PipelineReads` doc comments)
- `crates/pipeline/src/cache.rs` 2
- `crates/pipeline/src/fixture.rs` 0 (code only)
- `crates/pipeline/src/lib.rs` 0
- `crates/pipeline/src/loader.rs` 21 (docs, comments, 4 error strings, `BoundStage` docs)
- `crates/pipeline/src/node.rs` 30
- `crates/pipeline/src/reads.rs` 38 (including all error and line formats)
- `crates/pipeline/src/reads.tests.rs` 12
- `crates/pipeline/src/run.bound_stage_tests.rs` 3
- `crates/pipeline/src/run.diagnostics_ingest_gate.rs` 0 (code only)
- `crates/pipeline/src/run.output_scope_tests.rs` 0
- `crates/pipeline/src/run.rs` 8
- `crates/pipeline/src/scheduler.rs` 22 (3 error strings, comments, docs)
- `crates/pipeline/src/stages/archive_blobs.rs` 2
- `crates/pipeline/src/stages/attach.rs` 1
- `crates/pipeline/src/stages/carrier.rs` 4 (docs plus the reasoning-archive error)
- `crates/pipeline/src/stages/carrier.term_entailments_tests.rs` 1
- `crates/pipeline/src/stages/carrier_rooting_tests.rs` 2
- `crates/pipeline/src/stages/conformance/native_scene.rs` 0
- `crates/pipeline/src/stages/gmn_training_corpus.rs` 3
- `crates/pipeline/src/stages/goal_directed.rs` 0
- `crates/pipeline/src/stages/gts_sink.rs` 1
- `crates/pipeline/src/stages/gts_sink.tests.rs` 1
- `crates/pipeline/src/stages/mappings.rs` 0
- `crates/pipeline/src/stages/medium_dictionaries.rs` 1
- `crates/pipeline/src/stages/medium_dictionaries.tests.rs` 2
- `crates/pipeline/src/stages/parse_sources.rs` 0
- `crates/pipeline/src/stages/research_objects.rs` 0
- `crates/pipeline/src/stages/shape_union_fresh.rs` 1
- `crates/pipeline/src/stages/validate.rs` 0
- `crates/pipeline/src/stages/verify_attestation.rs` 0
- `crates/pipeline/src/tests.rs` 22 (test names and docs, 6 new tests, 4 helper docs)
- `crates/pipeline/tests/dag_dogfood.rs` 4
- `crates/slice-brief/tests/packet.rs` 0 (code only)
- `docs/GATE-AND-PIPELINE.md` 9
- `docs/PIPELINE_SPINE.md` 7
- `slices/core/pipeline/docs.md` 1
- `slices/core/pipeline/module.ttl` 66 annotations on the new classes, properties and lanes, the changed `flowReasonDataflow` definition, and every per-edge definition (about 118). The edge definitions were checked against the declared reads one by one.

## Not determined
- The claim in `run.rs` `full_spec` that "the producer's own authentication covers the file" (the compiled-in description): not checked against the producer receipt code.
- Compilation: nothing was run, so error and `expect` texts were compared to the code paths by reading only. For example `bind_rejects_swapped_implementations` quotes `stage a: gmeow:stageImpl "impl:a" ... own id is "b"`, which I checked against the format string but not against `registry_for` key assignment.
- `consumer_view_of` (`reads.tests.rs`) docs its product as "released the way the scheduler releases it" while a `released=false` mode exists. I judged this a description of the `true` case, not a false statement.
- `tests.rs` `attach_drift_honors_typed_entity_inputs` inserts the same `reads` entry twice. It is a duplicated line, not a statement, so it is not counted.
- Unedited slice text for the 30 or so edges that already existed on main (their definitions are outside the diff).
- The `lane_visible` doc says an edge "names" an internal artifact. Only an exact `flowReadsArtifact` counts; I did not find a declaration form that would contradict this.
