VERDICT: BLOCKED

# Independent review of pull request 2755 at 85cef27fe

## Scope

- Base `05385cf3b`, head `85cef27fe9d0dd87c6cb29656f7e21c4520b5e21`, branch `paudley/pipeline-declared-graph`.
- Commits read: `c4671a954` (the restore of the reverted change) and `85cef27fe` (removal of `empty_tiers()` in `crates/slice-brief/tests/packet.rs`).
- Everything was read from git objects (`git show`, `git diff 05385cf3b 85cef27fe`, `git grep 85cef27fe`). I did not read the working tree, and I ran no builds or tests.
- Requirements read: the PR body; issues 2697, 2724, 2753, 2758 and 2696 with their comments (2697 and 2724 carry only a bot comment); `AGENTS.md`, `CLAUDE.md`, `.goals`, `.baseline`; the relevant passages of `docs/PIPELINE_SPINE.md` and `docs/GATE-AND-PIPELINE.md` at head.
- Line numbers refer to files at `85cef27fe`. A "≈" marks a line located from a diff hunk rather than counted directly.

## Equivalence of the old and new stage graph

**Result: the stage graph that executes has not changed in any field the old `full_spec()` set. What changed is how the graph is executed. Three things are now derived from the slice where the Rust stage objects used to supply them: retention, the kept internal artifacts and stage identity.**

How I established it:

1. The diff to `slices/core/pipeline/module.ttl` removes exactly 10 lines (`git diff --numstat`: 1545 added, 10 deleted). Nine of the removed lines are the closing `.` of the nine existing `gmeow:BuildDataFlow` edges, each turned into `;` so read declarations can follow. The tenth is the last member of the edge list on `gmeow:pipeline-build`, which gains the 115 new edges. No stage individual's `gmeow:stageImpl`, `gmeow:dataflowConsumes`, `gmeow:hasCapability`, `gmeow:requiresResource`, `gmeow:stageStability`, `gmeow:stageCacheDisposition`, `gmeow:attachesGraph`/`gmeow:attachesBlobRep` or `gmeow:flowEntity` line is touched.
2. I compared, by hand, the removed `full_spec()` text (run.rs at base, the `st_*` builders) with the stage blocks of the head slice (module.ttl 2186–2232 and 2236–2901).
   - **Stage ids:** 47 on both sides, the same set (`hasStage` list, module.ttl 2135–2181).
   - **Implementation keys:** all 47 identical: `parse_sources`, `source_load`, `statements`, `compile_logic`, `native_codebook`, `mappings`, `math_producers`, `gmn-training-corpus`, `slice-brief`, `reason`, `verify_attestation`, `goal_directed`, `validate`, `conformance`, `agreement`, `docs_render`, `constraint_catalog`, `term_manifest`, `snapshot`, `lpg`, `metadata`, `okf`, `yaml_ld`, `export`, the 15 source leaves, `json_schema`, `pydantic`, `schemas`, `research-objects`, `result_shape_composition`, `archive-blobs`, `medium-dictionaries`, `gts_sink`.
   - **Consumes edges:** 124 on both sides, stage by stage: snapshot 21, sink 24, archive-blobs 11, validate 7, pydantic 6, docs-render 6, verify 5, and so on. I summed the old builders and the slice lists separately.
   - **Capabilities:** `sourceOrigin` on `stage-source-load` and `sinkCapability` on `stage-gts-sink` on both sides.
   - **Resources:** `engineResource` on `stage-reason`; `serializationBufferResource` on `stage-export-export`, `stage-export-yaml-ld` and `stage-medium-dictionaries`, on both sides.
   - **Entity narrowing:** the nine edges are unchanged lines: compile-logic←parse-sources, reason←compile-logic, verify←compile-logic, verify←reason, validate←compile-logic, validate←parse-sources, validate←reason, goal-directed←reason, json-schema←compile-logic.
   - **Stability, cache disposition, attach sets and output family:** at base these came from the Rust implementation (`full_spec()` copied stability, cache and attaches from `registry.get(..)`). At head `bind` still requires the slice to equal the same unchanged Rust implementation (loader.rs 597–680). No stage implementation's declarations change in the diff, so these are equal by transitivity.
   - **Pipeline id:** `pipeline-build` on both sides. `formats` is now filled from the slice (it was empty before), but no execution path reads it (`git grep '\.formats\b'`: only docs measurement and tests).
3. Behaviour that differs even though the graph is equal:
   - **Retention.** `BoundStage::carrier_consumes` (loader.rs ≈697–725) is derived from the declared reads. At base, every stage except the sink and medium-dictionaries kept the carrier of every producer it consumes, through the trait default `carrier_consumes() = consumes()`. At head, 71 of the 124 edges declare no carrier lane and no entity narrowing, so the producer reaches the consumer released. At base that was true of 23 edges (21 sink edges and 2 medium-dictionaries edges). For the sink and medium-dictionaries the derived sets equal the old Rust overrides (sink: archive-blobs, medium-dictionaries, snapshot; medium-dictionaries: archive-blobs, snapshot).
   - **Kept internal artifacts.** Derived from every `flowReadsArtifact "pipeline/…"`. The set is the sink's four (matching `REASONING_INTERNAL_READS` by path name; constants not resolved, see Not determined) plus three new ones: `pipeline/statements/gmeow.rdf12.jsonld` and `pipeline/statements/gmeow.rdf12.yamlld` for archive-blobs, and `pipeline/agreement-tallies.json` for export-agreement.
   - **The span table.** Derived from any read set holding `BlobRep("spans:source-table")`. Only validate←source-load declares it, which equals the old `ValidateStage::consumes_span_table`.
   - **Stage identity.** `BoundStage::id()` returns the slice id (loader.rs ≈729–731), not the implementation's own `id()`. See defect N5.

No stage, edge, capability, resource, stability, cache disposition, attach set or narrowing was added, dropped or changed.

## Claims and findings

### 1. Equivalence

As above. One protection is gone. Nothing in the repository now compares the slice with an independent statement of the graph or with the implementation's own identity. Defect N5 and the "Already disclosed" item on the parity test cover this.

### 2. The declared reads

- **Every edge declares a read.** There are 124 `gmeow:BuildDataFlow` individuals. Each names a distinct (consumer, producer) pair and each carries at least one `gmeow:flowReads*` value.
  - 53 edges keep the producer's carrier (a carrier lane, a handle, a blob or an entity narrowing); 71 do not.
  - 6 are identity-only: constraint-catalog←reason, term-manifest←reason, gmn←compile-logic, gmn←mappings, sink←export-glossary and sink←source-load.
  - 7 declare `laneCarrier`: docs-render←compile-logic, docs-render←reason, docs-render←validate, sink←archive-blobs, medium←archive-blobs, snapshot←compile-logic and snapshot←validate. This is the count 2758 item 1 left open.
- **Spot checks against the stage bodies.** All 19 below were true. None was too narrow.
  1. archive-blobs←compile-logic `laneAllArtifacts`: archive_blobs.rs 933–936 calls `.artifacts()`.
  2. archive-blobs←export-json-schema, four artifacts: archive_blobs.rs 906–930.
  3. archive-blobs←result-, frame- and constraint-shapes, one artifact each: 987–1008.
  4. archive-blobs←mappings `laneAllArtifacts`: 942–945. It is wide in what it reads but costs nothing, because it keeps no carrier; only path prefixes are used (archive_blobs.rs 326–344).
  5. archive-blobs←math-producers, four named queries: 954–968. That `COMPETENCY_QUERIES` has exactly four entries was not checked.
  6. archive-blobs←glossary, ←pydantic and ←native-codebook `laneAllArtifacts`: 974–981 and 1013–1016.
  7. archive-blobs←statements, two internal artifacts plus `laneAllArtifacts`: 1022–1045.
  8. goal-directed←compile-logic `Handle(graph/logic)`: goal_directed.rs ≈78.
  9. goal-directed←reason `Handle(graph/reasoning)`: goal_directed.rs ≈118.
  10. validate←reason `Handle(graph/reasoning)`: validate.rs ≈723.
  11. validate←source-load `laneDataset`: validate.rs ≈381, `substrate::project(product.dataset())`.
  12. verify←reason `Handle(graph/reasoning)`: verify_attestation.rs ≈141.
  13. mappings←compile-logic `Handle(graph/logic)`: mappings.rs ≈911.
  14. research-objects←mappings `Handle(graph/correspondence-laws)`: research_objects.rs ≈1646.
  15. snapshot←reason `Handle(graph/reasoning)` plus `laneDataset`: carrier.rs ≈1436 and ≈3583 (the handle), ≈973 (`dataset_arc`).
  16. snapshot←compile-logic, three handles plus the dataset: carrier.rs ≈3524, ≈3558, ≈3569 and ≈1740.
  17. snapshot←source-load `laneDataset`: carrier.rs ≈830.
  18. snapshot←statements `laneDataset`: carrier.rs ≈1664.
  19. All eight parse-sources edges, `Handle(graph/source-catalog)` plus `laneDataset`: parse_sources.rs ≈359–363. Here the dataset read is `let _ = product.dataset();`, a discarded call that predates the change. Declaring it keeps parse-sources' carrier for every consumer, but the handle read keeps the carrier anyway.

### 3. The mechanism

**Recording surface (node.rs).** `bundle()` records `Carrier` (350–353). `handle`, `rep_blob`, `dataset`, `dataset_arc`, `artifact`, `artifacts`, `artifact_refs` and `span_index` record (356–414, 461–465).

**Reads that are not recorded.**

- The `bundle` field (154) and `raw_bundle()` (332) are `pub(crate)`. Every stage lives in the same crate, so any stage body can read the whole carrier without a record. No stage does so today: `git grep` over `crates/pipeline/src/stages` finds only three uses in `docs_render/reasoning_tests.rs`.
- `diag_nodes()` (424–447) records nothing (disclosed).
- `consumer_view` (323) and `EdgeAccess::new` (reads.rs) are `pub`. A stage could re-wrap a product with an edge of its own; that would be deliberate evasion.
- The `digest`, `stage_id` and `carrier_released` fields are public identity data.
- File reads go through the Rust `input_files()` and are not part of the RDF graph at all. That falls short of the owner's "full graph" intent, and the PR body does not mention it.

**The comparison.** `covers` and `unread` in reads.rs are consistent in both directions, except that a declared `Carrier` hides over-declaration (disclosed).

**What happens on a difference.**

- Under `Enforce`, the stage is recorded in `read_differences` and its product is not written to the stage cache: scheduler.rs 826–850. `cache.put` there is the only production write site; `git grep '\.put(&'` finds no other.
- The product is still published to every downstream stage (scheduler.rs 908) and the run continues, then fails at 989–1005.
- Under `Record`, nothing is flagged, and if a context enabled the stage cache, products would be cached. The only caller, `record_run`, uses `open_uncached`.

**Ways a difference is lost.**

- **A failing stage.** Its `Err` result drops its edges (801–808), so its reads are never compared.
- **Any other stage's failure.** `scheduling_result?` (988) returns before `read_differences` is reported, so every recorded difference disappears from the error.
- **An undeclared carrier read on a released edge.** It never reaches the report; see defect N1.

Parallel stages are safe: each dispatch gets its own `EdgeAccess`, and the comparison runs on the coordinator thread.

### 4. The cache

- **Cached stages are not compared.** Confirmed: the hit path returns `edges: Vec::new()` (scheduler.rs ≈1197).
- **The key does not cover the read declarations.** `StageKeyContext` (cache.rs 192–226) holds the schema version, stage id, `impl_version`, codec, `BuildIdentity`, upstream digests or entities and raw-input digests. Neither the reads nor the derived retention are in it.
  - **Consequence for test and debug builds.** There the build identity is `digest(manifest)` (build_script.rs, `None` arm), so a changed read declaration with unchanged inputs is a cache hit and is never checked.
  - **For the authenticated producer**, the fingerprint is the input-inventory digest, and that inventory follows `include_str!` (build-inputs `modules.rs` ≈724–735). An edit to module.ttl therefore most likely changes every key and causes a full miss (inferred, not run). If so, the PR's statement that "a changed declaration is ... not checked until something else causes a cache miss" is too pessimistic for the authenticated producer and accurate only for non-producer builds.
- **The `cache.rs` diff alters nothing keyed, stored or retained.** It only replaces `product.bundle()` and `product.dataset()` with `raw_bundle()` so that the scheduler's own inspection does not record a read.
  - Side effect: `default_graph_keys` and `provenance_keys` (cache.rs 687–690 and 721–726) no longer pass through the `carrier_released` assertion inside `dataset()`. On a released product they now return an empty set silently instead of panicking.
  - Today's callers guard this (`verify_attach_drift` reads these lanes only for carrier inputs, at scheduler.rs 1414–1420), so it is a loosened invariant, not a live defect.

### 5. Retention and memory

**Derivation.**

- The carrier-reader sets are the old sets or subsets of them. Every non-sink, non-dictionary stage used to keep every producer's carrier, so the change can only release sooner.
- The span table is equivalent.
- The internal-artifact set grows by three (see Equivalence).

**Partial products.**

- I found no case where a consumer now receives a released or partial product it needs.
- The only shape that could produce one is `laneAllArtifacts` on a released edge whose consumer needs an internal artifact it does not name. The current consumers of that shape filter internal paths: archive-blobs makes named, fail-closed reads, and the snapshot presenter's `take_opaque` skips the `pipeline/` prefix (carrier.rs 2376–2398). Latent hazard: defect N6.

**Peak memory can rise, at two places.**

1. **Dispatch-time release copies.** For each of the 71 non-carrier edges (48 more than at base), the dispatch calls `into_carrier_released_keeping` (scheduler.rs 733–741). `release_carrier` deep-copies every committed artifact and every kept internal artifact into a new content store with `bytes.clone()` (bundle.rs 179–217, copy at 205).
   - The original product stays live in `products` until its last carrier reader runs, so the copy is pure addition for the duration of the consumer.
   - At base, these 48 consumers received an `Arc` clone.
   - Concrete case: archive-blobs is dispatched while compile-logic, mappings and statements are still carrier-held for snapshot. It now receives fresh copies of all three committed artifact lanes. snapshot receives copies of five leaves' lanes (evals, glossary, which includes the TBX file of about 18 MB, profiles, research-objects and skos-surface).
2. **Longer retention.** `kept_internal` is computed once per run (scheduler.rs 622–633), and the two statements JSON-LD-star and YAML-LD-star renderings are now in it. At base they were dropped when statements' last carrier reader (snapshot) released it. Now they survive in statements' released residue until the run ends, through the sink and export stages, where 2696 places the memory climb.

Releasing earlier saves memory elsewhere. The net effect on the peak is not determined, and nothing measured it.

### 6. What was removed or weakened

See the section "Everything weakened or removed".

### 7. Tests

See the section "Tests".

### 8. The slice text

- **Well-formedness.** Well-formed by reading. Every new term has `rdfs:label`, `skos:definition` and `rdfs:isDefinedBy`. There are no issue or PR numbers and no hand-authored shapes. Literals use `@x-gmeow-english`, as the rest of the slice does.
- **Authoring rules.**
  - `gmeow:ProductLane rdfs:subClassOf logic:QualityValue` and `rdfs:domain` on the four new properties copy the slice's existing idiom (`gmeow:StageCacheDisposition`, `gmeow:flowEntity`). They still add net-new `rdfs:` subsumption, domain and range rows, which the projection-vocabulary ratchet budgets. Whether the ratchet passes is not determined (`make check` was not run).
  - `ProductLane` is called a "CLOSED set", but no `logic:oneOf` closes it. The `StageCacheDisposition` values are closed that way (module.ttl 4427). Only the Rust parser enforces closure.
  - The new terms have no `fr.po` or `zh.po` entries. Whether a gate requires them is not determined.
- **Truthfulness.** Several definitions are false; see "False statements".

### 9. False or overstated statements

See the section "False statements".

### 10. Public-repository hygiene

No private names, host paths or issue numbers appear in added lines. The only `paudley` matches are the standard SPDX copyright headers.

## Defects, most serious first (new)

### N1. HIGH: an undeclared carrier read on an artifact-only or identity edge is not reported; it panics, and probably hangs the scheduler. Fix before merge.

- **Where:**
  - node.rs 368–377 and 380–389: `dataset()` and `dataset_arc()` assert `!carrier_released` before calling `record`.
  - node.rs 356–365: `handle()` and `rep_blob()` record, then return `None` on a released product.
  - scheduler.rs 733–741: the dispatch releases every edge without a carrier lane.
  - scheduler.rs 684–686, 755–772 and 789: the coordinator keeps its own `tx` alive and blocks on `rx.recv()`.
- **What is wrong.** The purpose of the change is that a stage reading what it never declared is reported with the Turtle to fix it. For 71 of the 124 edges the producer is released before dispatch.
  - A stage on such an edge that starts reading the dataset panics inside a rayon worker before anything is recorded.
  - The worker never sends on the channel, and the coordinator holds its own sender, so `rx.recv()` cannot return (inferred hang; no `catch_unwind` exists in the scheduler).
  - A `handle()` or `rep_blob()` read returns `None`, and the stage fails with its own message, for example "stage-reason product carries no Reasoning handle". The `Err` path then drops the recorded edges, so no read difference is reported.
- **Concrete failing case.** Add `let _ = input.upstream["stage-export-constraint-shapes"].dataset();` to `stage-validate`. Its edge declares only `flowReadsArtifact "generated/shapes/constraint-shapes.ttl"` (module.ttl 1856–1865). The result is a panic and, by inference, a hung producer, not the message "declare `gmeow:flowReadsLane gmeow:laneDataset`".
- **Exposure.** At base the same assertion was reachable from 23 sink and dictionary edges. Now it is reachable from 71.
- **Why before merge.** This is the main error the change claims to report, and the failure mode is a hang in a lock-holding producer (`make check` holds a host-global lock).

### N2. HIGH: peak memory can rise; nothing measured it. Fix or measure before merge.

- **Where:** scheduler.rs 733–741 together with bundle.rs 205 (dispatch-time deep copies on 48 more edges); scheduler.rs 622–633 (the statements renderings kept to the end of the run).
- **Failing case:** the archive-blobs and snapshot dispatches described in Claims §5.
- **Why before merge.** The producer already exceeds hosted-runner memory (2696). The PR's own "required before the merge" list includes the peak memory of an enforcing pass, and that pass has not been run.
- **Cheap fix.** Have `release_carrier` share the existing `ContentStore` entries by `Arc` instead of copying them. Release at dispatch only when the original in `products` has itself been released.

### N3. MEDIUM-HIGH: the Rust retention declarations are now dead, and the tests that guard them prove nothing about production. Fix before merge.

- **Where:**
  - Dead overrides that `BoundStage` ignores (loader.rs ≈738–775) and nothing compares with the slice any more: `GtsSinkStage::carrier_consumes` and `internal_artifact_reads` (gts_sink.rs 131–135 and 157–165), `MediumDictionariesStage::carrier_consumes` (medium_dictionaries.rs 475–478 and 500), `ValidateStage::consumes_span_table` (validate.rs 467).
  - Tests that build the Rust object, not the bound stage: `sink_declares_only_the_lanes_it_reads_as_carrier_inputs` (gts_sink.tests.rs 45–75), `the_sink_declares_every_internal_artifact_its_reasoning_archive_reads` (carrier_rooting_tests.rs 669), `the_consumes_set_is_sorted_and_bound` (medium_dictionaries.tests.rs 65) and `term_panels_follow_the_actual_documentation_stage_dependencies` (carrier.term_entailments_tests.rs 204–211).
- **Concrete failing case.** Delete `gmeow:flowReadsArtifact "pipeline/prepared-verify-gates.json"` from `gmeow:pipeline-dataflow-gts-sink-verify-attestation` (module.ttl 1394–1404). Production drops that artifact at release, and the sink fails at the end of a full producer run. All four tests above still pass.
- **Rules broken.** `.goals` and `CLAUDE.md` ("when replacing an element, remove the inferior one"), the engineering policy's one-path rule, and run.rs's own claim that "Nothing in Rust repeats it".

### N4. MEDIUM: false statements in doc comments, canonical documentation, help text and slice definitions. Fix before merge.

The full list is under "False statements". `docs/GATE-AND-PIPELINE.md` treats a false claim in a comment or in help text as a defect in its own right. The two canonical documents (`docs/GATE-AND-PIPELINE.md` 967–970 and `docs/PIPELINE_SPINE.md` 311–315) now instruct contributors wrongly. A contributor who follows the "declare the edge in all three places" checklist and adds no `gmeow:BuildDataFlow` read gets `InvalidDag` from the scheduler at run time.

### N5. MEDIUM: the check that a stage's implementation is the stage it is bound to is gone. Fix before merge (one line).

- **Where:** loader.rs ≈729–731 (`BoundStage::id()` returns `spec.id`); `bind` (536–685) never compares `stage.id()` with `s.id`.
- **How base caught a mis-binding.** The removed parity test compared `impl_key` with the independent Rust list. And because the bound object was the implementation itself, `by_id` was keyed by the implementation's id, so a mis-keyed stage would fail at `by_id[stage_id]` (inferred).
- **Failing case (inferred).** Swap `gmeow:stageImpl` between two leaves that both consume nothing and have equal capabilities, resources, stability, cache disposition and attach sets, such as two source-reading export leaves whose attach sets are empty. Both bind, and each runs the other's body under the other's id. The sink reads both through `laneAllArtifacts`, so the read comparison passes too. Which leaf pairs have equal attach sets was not established.

### N6. MEDIUM: `laneAllArtifacts` on a released edge silently delivers a partial lane, and the recording tool cannot reveal it. May be fixed after merge.

- **Where:** scheduler.rs 733–737. `Record` hands every product over whole, while `Enforce` releases it and drops undeclared internal artifacts (bundle.rs 191–193).
- **The risk.** `gmeow-dev pipeline-reads` can therefore emit `laneAllArtifacts` for a consumer that needs an internal artifact, and the enforcing run then omits it without an error.
- **Why after merge is acceptable.** No current consumer depends on an unnamed internal artifact (Claims §5).

### N7. LOW-MEDIUM: read differences vanish whenever any stage fails. May be fixed after merge.

- **Where:** scheduler.rs 801–808 drops a failing stage's edges, and 988 returns `first_error` before 989–1005 reports `read_differences`.
- **Concrete case.** One stage fails and three others are misdeclared: the error names only the failure.
- **Why after merge is acceptable.** The run still fails, so nothing is accepted wrongly. Only diagnosis is lost.

### N8. LOW: new refusals without tests. May be fixed after merge, but should precede closing 2753.

- `parse_edge_reads` refuses a lane outside the closed vocabulary or a value of the wrong term kind (loader.rs ≈496–531). No test.
- "The product of a stage whose reads differ is not cached" has no test. The only run-level test uses `RunContext::open_uncached` (tests.rs ≈401).

### N9. LOW: test fakes edited to satisfy the new check, with inaccurate comments. Optional.

`EntityConsumer::run` and `AttachTestStage::run` (tests.rs ≈1785 and ≈2068) now call `product.dataset()` and discard the result, under comments such as "The consumer reads the graphs its typed dataflow narrows it to". The behaviours those tests name (artifact-level invalidation, attach drift) are unaffected, so the edits are acceptable, but the comments describe reads the fakes do not make.

## Already disclosed (PR body, 2753, 2758), and whether merging with each open is safe

- **D1. Cached stages are not compared, and the key does not cover the reads (PR body; 2753 items 1–2).** Safe to merge for the authenticated producer only if module.ttl and the pipeline sources are in the producer fingerprint (inferred yes; Claims §4). Not safe to rely on in non-producer contexts. Acceptable to merge with it open.
- **D2. The enforcing runs were not cold; there was no full-scope enforcing pass from an empty cache (PR body; 2753 item 6).** Not safe to merge without it. Nothing else shows that the 124 declarations match the bodies of stages that did not execute. The PR itself lists this as required before merge.
- **D3. Unexplained 43,407 against 43,357 description-logic gap findings (PR body).** Not safe to merge. An unexplained output difference contradicts the claim that only the description's location changed. By my comparison the graph is equal, so the cause must lie in execution (retention, release copies, attach-drift inputs or something else) and must be found.
- **D4. Whole-carrier over-declaration passes (2758 item 1).** Seven edges declare it (Claims §2). Safe to merge: over-declaration only keeps a carrier as long as base did.
- **D5. `diag_nodes()` records nothing (PR body; 2758 item 2).** Safe: it is called only on a stage's own product on a cache hit.
- **D6. A consumer view may outlive its edge (2758 item 3).** Not determined whether any stage does this. Safe to merge as a known gap.
- **D7. The loader accepts an edge with only an entity, and the empty-read refusal exists only under `Enforce` (2758 item 4; 2753 item 4).** Safe, because every production context is `Enforce` (scheduler.rs 194, 234, 256).
- **D8. Removed bind checks and tests not accounted for (PR body; 2758 item 5).** See "Everything weakened or removed". The part of it that is unsafe is N3, which is new: the dead Rust twin and its tests.
- **D9. Debug escaping in `Read::turtle` and `Display`; `include_str!` reaching outside the crate; a swallowed poisoned lock; a duplicate insert in a test (2758 items 6–9).** Safe to merge.
- **D10. The parity test now compares the slice with its own embedded copy (PR body; 2753 item 5).** Merging is safe only because this review compared the two graphs by hand. Together with N5, no regression guard remains for implementation-key or identity drift.
- **D11. The `helpers` line.** Fixed at head by `85cef27fe`: `empty_tiers()` is removed and type inference fills in `BTreeMap::new()`. Correct.
- **D12. Hosted checks, strict clippy, focused tests and `make helpers-hygiene` not run; `make check` not run (PR body).** Not safe to merge until the hosted checks have finished and each failure has been compared with main, as the PR's own merge conditions require.

## Everything weakened or removed

1. **`bind` checks of `carrier_consumes()`** (no duplicates, contained in `dataflowConsumes`), removed from loader.rs. Replaced by the derivation in `BoundStage::new`, which is contained and duplicate-free by construction. As strong for production, but the Rust overrides remain and are unchecked (N3).
2. **`bind` checks of `internal_artifact_reads()`** (the producer must be an artifact-only dependency; the artifact must be internal), removed. Replaced by derivation from `flowReadsArtifact "pipeline/…"`.
   - A committed artifact is no longer an error; harmless.
   - An internal artifact declared on an edge that keeps the carrier is now accepted silently, and is then kept through every release of that producer until the run ends: a retention cost with no error.
   - Weaker.
3. **Test `bind_rejects_carrier_dependency_outside_logical_consumes`**, removed. Replaced by `a_read_set_on_a_stage_that_is_not_consumed_is_refused`, a validate-time check that read sets are a subset of consumes. Comparable strength.
4. **Test `binding_refuses_a_misdeclared_internal_read`**, removed. No replacement.
5. **Test `slice_dag_binds_and_matches_full_spec`**, replaced by `the_executed_pipeline_is_the_authored_slice`. The new test compares module.ttl with its `include_str!` copy, which cannot differ within one build. The checks on id, implementation key, consumes, capabilities, resources, narrowing and attaches against an independent statement are lost. Much weaker; together with N5, nothing guards the implementation key against the stage id.
6. **Loader refusal of an edge with no `flowEntity`**, loosened to "neither a read nor an entity" (necessary for the new edges).
7. **The `carrier_released` assertion** on the paths `default_graph_keys` and `provenance_keys` (cache.rs 687–690 and 721–726), removed. A released product now silently gives an empty key set.
8. **Stage identity** now comes from the slice, not from the implementation (N5).
9. **Tests edited so they pass:**
   - the `spec()` helper declares `Identity` on every synthetic edge (tests.rs ≈58–67);
   - `dangling_dependency_is_rejected` clears the reads;
   - `EntityConsumer` and `AttachTestStage` add discarded `dataset()` reads (N9);
   - `attach_drift_honors_typed_entity_inputs` inserts the same read set twice (disclosed);
   - `CarrierStage` switches to `raw_bundle()` so that its released-product inspection is not recorded.
   Each keeps its named behaviour. None of these edits weakens what the test checks.

## Tests

**New tests.**

- `a_run_names_every_stage_whose_reads_differ_from_its_declared_dataflow` (tests.rs ≈375–426). Protects the reporting of undeclared reads and the "name every stage" continuation, and can fail. It covers only an undeclared handle read, which records before it returns `None`. It does not cover the dataset path (N1), the declared-but-unread direction at run level, or cache exclusion; it runs uncached.
- `a_read_set_on_a_stage_that_is_not_consumed_is_refused`. Protects `PipelineSpec::validate`, and can fail.
- The reads.tests.rs unit tests.
  - `a_declaration_permits_exactly_the_lanes_it_covers` and `undeclared_and_unmade_reads_are_both_reported` protect `covers`, `unread` and the format of the difference lines, and can fail.
  - `retention_follows_the_declared_lanes` tests two helper predicates (`needs_carrier`, `internal_artifacts`) and mirrors their implementation. It does not test the scheduler's actual retention.
- `the_executed_pipeline_is_the_authored_slice` cannot fail in a single build, apart from `validate` and `bind` (D10).

**New behaviour with no test:**

- the refusal at `Enforce` of an edge that declares no read (scheduler.rs 642–666);
- the declared-but-never-read direction in a run;
- `ReadPolicy::Record` (no refusal; whole products);
- exclusion from the cache on a difference;
- `BoundStage`'s derivation of carrier inputs, kept internal artifacts and span-table consumers from the production slice;
- `parse_edge_reads` refusals;
- `observed_turtle`, `record_run` and `gmeow-dev pipeline-reads`;
- the dataset-read failure mode on a released edge (N1).

## False statements

**Code comments:**

- node.rs 152–153: "Crate-private: a stage reaches a lane only through the accessors below". Every stage is in the same crate and can use the `bundle` field and `raw_bundle()`.
- node.rs 160–163: release happens once "the LAST stage declaring this one in `carrier_consumes()` has run". The set is now derived from slice reads, and the Rust declaration is ignored.
- node.rs 668–674: "The conservative default is every dependency ... the loader hard-fails if an override names anything outside `consumes()`". The production stages are `BoundStage`, whose set is derived, and the loader check was removed in this diff.
- node.rs 679–683: "The loader hard-fails a pair whose producer is not an artifact-only dependency or whose artifact is not internal". Removed in this diff.
- run.rs ≈304–313: "Nothing in Rust repeats it". Every stage implementation still declares consumes, capabilities, resources, stability, cache disposition, narrowing and attaches, and `bind` requires agreement (loader.rs 551–680). The dead retention overrides repeat it too (N3).
- reads.rs module doc, lines 14–15: "a product keeps a lane exactly as long as a consumer that has not run yet declares it". Internal artifacts are kept for the whole run once any consumer declares them (scheduler.rs 622–633). The same applies to the doc on `internal_artifacts` in reads.rs.
- Stale references to `full_spec` as a place where the graph is authored, and to `carrier_consumes()` or `internal_artifact_reads` as what governs retention:
  - archive_blobs.rs 710 ("declare in carrier_consumes()") and 819–821;
  - stages/attach.rs 11;
  - shape_union_fresh.rs 49;
  - gmn_training_corpus.rs 66 and 777;
  - medium_dictionaries.tests.rs 62;
  - carrier.rs ≈3038, whose error tells the reader to "declare the read in internal_artifact_reads" when the fix is now a `flowReadsArtifact` in the slice.

**Canonical documents:**

- `docs/GATE-AND-PIPELINE.md` 967–970: "Declare the edge in all three places ... `full_spec()`". `full_spec()` is no longer an authoring place, and the now-required `gmeow:BuildDataFlow` read declaration is not mentioned.
- `docs/PIPELINE_SPINE.md` 311–315: "The executable `carrier_consumes()` subset ... The loader requires that subset ...". The subset is now derived from the slice's declared reads.

**Slice definitions:**

- `gmeow:flowReadsLane`: a difference "both fail the stage before its product is published". The product is published to downstream stages and the run continues; only cache admission is withheld (the scheduler.rs 819–825 comment says so). The definition also does not mention that cached stages are exempt.
- `gmeow:flowReadsBlobRep`: "The scheduler strips a representation blob ... once the last consumer that declares it has run". Only `spans:source-table` on `stage-source-load` is stripped (scheduler.rs 911–925).
- `gmeow:flowReadsArtifact`: "unless a consumer that has not run yet declares it". Kept for the whole run (see above). Its `skos:example` names `"pipeline/logic/canon-admission.json"`, but the real sink edge declares `"pipeline/canon-proof-admission.json"` (module.ttl 1152).
- `gmeow:flowReasonDataflow`: "the build DAG's own edges all carry it". This text predates the change but is now false: the identity edges carry `gmeow:flowReasonAuthority` (for example module.ttl 802).
- `gmeow:ProductLane`: "The CLOSED set ...". It is closed only in Rust; no `logic:oneOf` closes it (Claims §8).

**Help text and error messages:**

- `gmeow-dev pipeline-reads` help, gmeow-dev-cli lib.rs ≈289: "print the declarations". The command writes to a required `--out` file.
- The scheduler error at 657–660: "run `gmeow-dev pipeline-reads` to print the declarations". Run as written, the command fails because `--out` is missing.

**PR description:**

- "Under the enforcing policy a difference fails the run and the stage's product is not written to the stage cache." True for differences that are recorded. Not true for an undeclared carrier read on a released edge (N1), and not true when another stage fails (N7).
- Otherwise the description is careful and its "does not show" list is accurate. N1 through N3 and N5 are not mentioned in it.

## Missing proofs

1. A full-scope `Enforce` producer pass from an empty stage cache, recording for each stage whether it executed and any read difference (D2).
2. The peak resident memory of that pass, compared with main (N2; issue 2696).
3. An explanation of the difference of 50 findings (D3), and a byte-identity comparison of the generated outputs with main.
4. `make check` and the hosted checks at this head.
5. Tests for the new refusals and policies listed under "Tests".
6. A guard that the bound implementation is the stage it is bound to (N5).

## Not determined

- Whether `slice_dag_binds_and_matches_full_spec` passed at `05385cf3b`. I established equivalence by reading both descriptions instead of relying on it.
- Whether the producer's `BUILD_FINGERPRINT` covers module.ttl. Inferred yes from build-inputs `modules.rs` ≈724–735; not traced end to end.
- Whether a worker panic actually hangs `run_without`. Inferred from the channel structure; not run.
- The net change in peak memory (N2).
- Whether the three docs-render `laneCarrier` declarations are wider than the stage needs. docs_render was not read.
- Whether any stage clones a consumer view into its own output (2758 item 3).
- Whether the four sink internal-artifact paths in the slice resolve to the same strings as `REASONING_INTERNAL_MEMBERS`. The constants were not resolved.
- Whether snapshot's attach-drift input set changes now that five more upstream products arrive released. `verify_attach_drift` builds its input graphs from the products it receives (scheduler.rs 1388–1397).
- Whether the new `rdfs:` rows pass the projection-vocabulary ratchet, and whether the i18n coverage gates require `fr.po` and `zh.po` entries for the new terms.
- Which pairs of consume-free leaves have identical attach sets (the failing case for N5).
