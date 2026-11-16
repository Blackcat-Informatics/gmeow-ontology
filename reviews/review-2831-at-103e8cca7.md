BLOCKED at 103e8cca7fe9bdfe6da562b4556d172ea35cf6e6

# Delivery review: constraints over reified statements on the validation paths

Branch `paudley/2831-constraints-over-reified-statements`. The worktree head, `origin/paudley/2831-…` and the expected head are the same: `103e8cca7`, a single commit on merge base `bc4560473`. `origin/main` is at `bae740871`, 12 commits ahead of the merge base. The branch has not integrated those commits. Reviewed range: `git diff origin/main...HEAD`, 30 files, +999 / -217.

## Scope

This review grades the narrow acceptance the caller gave against the verbatim issue:
(1) one lowering of RDF 1.2 reifiers into the `logic:reifiedStatement*` component edges is applied in every validation view, with no second implementation;
(2) RDF 1.2 controls for the two standpoint constraints: the attribution constraint does not flag a correctly attributed ranking, and the ranker-order constraint catches a reversed pair.

The issue's other acceptance items are not graded here. The caller says those items are tracked separately, but I did not see where. They are: per-constraint controls for the other 23 constraints, and replacing fixtures that hand-write the lowered edges.

## REQUIRED findings

### R1. The new finding code is not registered in the validator's code authority

- `crates/validate/src/store.rs:30` declares `NESTED_TRIPLE_TERM_CODE = "validate.nested-triple-term-not-lowered"` locally.
- `crates/validate/src/codes.rs:4-14` states that every finding code the validator emits is declared there as a `pub const`. It also says every emit site references that const, and that `ALL_CODES`, `STATIC_RULES` and the families are built from those consts, so an unclassified code fails the build.
- The new code is in neither `ALL_CODES` nor `STATIC_RULES`, and no family matches it: `validate.deep.` is the only `validate.` prefix.

Why this blocks:
- `rule_catalog::populate_rules` runs on the dev gate and through `enrich_findings` on stage-validate. For this code it falls through to `catalog_anchor_uri`'s "cannot happen for an emitted code" branch (`rule_catalog.rs:881-885`). Every such warning therefore carries a help URI to a catalog anchor that does not exist, and the constraint catalog has no rule for it.
- This is not hypothetical. The shipped example `slices/core/refinement-readings/examples/link-and-time-readings.ttl` contains 5 nested reifications (lines 243, 248, 253, 373, 378). The dev gate's per-example pass, Phase 10, will now emit 5 of these warnings, each with a broken link.
- The totality-by-construction invariant is silently bypassed.

Smallest correction:
1. Move the const into `codes.rs` and add it to `ALL_CODES`.
2. Add a `STATIC_RULES` row for it: `Severity::Warning` plus an enforcement class, with remediation if the catalog requires one.
3. Reference `codes::…` from `store.rs`, `validation_fixtures.rs` and the tests.

### R2. The standpoint controls exercise a copy of the constraints, not the repository's constraints

- The acceptance controls (`crates/validate/src/data_validate.tests.rs:716` `STANDPOINT_RANKING_CONSTRAINTS`, used by the test at `:832`) embed a hand-copied text of `gmeow:moreEntrenchedThanAttributionConstraint` and `gmeow:moreEntrenchedThanRankerOrderConstraint`. Today the copy matches `slices/core/standpoint/module.ttl:1134-1221` formula for formula; I compared them.
- Nothing ties the copy to the source. If the slice's constraints are edited, the controls keep passing against the stale copy and say nothing about the shipped constraints.
- The shapes are projected per constraint by `project_procedural_constraint`, not taken from the producer's output for the standpoint module.
- The standpoint slice has no `gmeow:ExampleConformance` cell and no example that uses `moreEntrenchedThan`. So nothing on this branch runs the real constraints over RDF 1.2 data. Seventeen other slices do use conformance cells, so the mechanism exists.

Acceptance (2) is about these two constraints, so a control on a copy does not meet it.

Smallest correction: add two RDF 1.2 example files and two `gmeow:ExampleConformance` cells under `slices/core/standpoint/` (`tests/` and `examples/`):
- a correctly attributed ranking, expecting Conforms;
- one ranker holding both directions, expecting Violates, pinned to the ranker-order constraint's procedural source shape.

The slice-spec producer then runs the real module constraints through `run_conformance_cell`, which this branch now lowers. Keep or delete the embedded-copy test; once the cells exist it is a duplicate.

### R3. Not every production SHACL view gets the lowering, and the change says it does

My search found these production calls into the SHACL engine that bypass `validation_view`. The full search is in the table below.

- `crates/pipeline/src/scoreboards.rs:1135` (`validate_claim_shapes`, the claim audit). This validates **user-supplied claim files** against production shapes.
- `crates/pipeline/src/stages/conformance/inference_validation.rs:111`. This validates authored control inputs against the authored shapes plus **the whole derived constraint-shape union**, which includes the 25 component-reading constraints.
- `crates/pipeline/src/stages/conformance/advice_wing.rs:80` and `crates/pipeline/src/stages/conformance/flagship_unwired.rs:65`. These validate inputs against the production shape profile.

None of these misbehaves today:
- None of their inputs contains a reification. I searched for `rdf:reifies`, `{|`, `<<` and `accordingTo` and found none.
- The claim audit's three shapes (`ClaimNeedsEvidence…`, `GroundingSpanShape`, `StaleSource…`) read no component edge.

Why this still blocks:
- Acceptance (1) requires the lowering in *every* validation view.
- The commit message says "validation_view is the one constructor a SHACL data view passes through".
- `statement_lowering.rs:30-37` presents its list as "the SHACL validations of authored and user data".
- Each bypass is a second door that will silently mis-evaluate the first time its input or shape set gains a reified statement. The repository's one-path rule and P11 (a false claim in a comment or rationale is a defect) both apply.

Smallest correction:
- Route the claim audit through `gmeow_validate::store::shacl_validate_dataset`, collecting `nested_findings`.
- Build `validation_view(&data)?` before `bind_dataset` / `bind_projected_dataset` in the three producer controls.

Each is a one- or two-line change, and none needs a new test beyond the existing producer observations.

## OPTIONAL findings

- **O1.** `crates/pipeline/src/stages/compile_logic/validation_fixtures.rs:123-128` puts a finding code into the `source_shape` column of a `(focus, source_shape)` observation pair. Any mismatch is still visible, but the column now mixes two kinds of value. A typed observation entry for the residue would be cleaner.
- **O2.** `lower_reifiers` (`statement_lowering.rs:160-178`) reads both the side table and `rdf:reifies` base quads.
  - The docs say a dataset "holds one form or the other for a given declaration"; nothing enforces it.
  - If a merged dataset holds both forms, the rows duplicate. The view's builder deduplicates them through `store_once`, so this is harmless. But `nested` duplicates too, which produces duplicate warnings.
  - Folded input is not read twice: `an_unfolded_rdf_reifies_quad_lowers_exactly_like_its_folded_form` asserts that the folded form yields exactly 3 rows.
- **O3.** Cost is unmeasured, and the repository's optimization guide (`docs/RUST-OPTIMIZATION.md`) says to measure first. When at least one flat reification is present, `validation_view` copies every base quad (as owned terms, with locations), every reifier and every annotation, then re-freezes. Who pays:
  - stage-validate: one full copy of source ∪ substrate per producer miss. When a substrate is present, this comes on top of the existing union copy.
  - `gmeow-dev validate` with a live merged pass: two full corpus copies. One is in Phase 8; the second is the Phase 10 base on any example cache miss. Each example file with a reification adds one more.
  - Slice conformance cells: one copy per cell; the slice module is re-copied every time.
  - Consumer validate, MCP, LSP and wasm: one copy per request.
  - The test harness `validate_dataset_sharded`: the whole repository ontology plus the fixture, per call.
- **O4.** `conformance_harness_sees_the_lowered_components_of_an_rdf12_fixture` (`crates/validate/tests/conformance_suite.rs`) protects test-support code, not a production entry point. The statement-lowering unit test already covers the flat form.
- **O5.** slice-brief (`assemble.rs:596`) drops `view.nested`. Its rationale is that a reifier is never a defined term. But a term that a nested statement is *about* is not checked over that statement, and it can still be picked as an exemplar.
  - The `validate_all` Phase 10 base also drops `base_view.nested`. That one is defensible, because Phase 8 reports the same residue for the same dataset.
- **O6.** Pre-existing, but this change depends on it. `recorded_merged_shacl` (`dev_validate.rs:420-486`) accepts `generated/diagnostics/shacl.json` when its input digest matches, and the digest does not include the producer's implementation identity.
  - A v14 (unlowered) `shacl.json` left on disk therefore passes `make validate` / `gmeow-dev validate` until the producer reruns.
  - The `v15` bump busts only the stage action cache, which already binds the build fingerprint (`pipeline/src/cache.rs:234-238`).
  - `make check` re-syncs first, so the gate itself is not affected.
- **O7.** In the rebuilt view, scoped blank nodes are re-interned from their qualified owned labels. Nobody has shown that a finding whose focus is a blank node renders identically with and without the view. Rendering affects result identity, dedupe and span enrichment.
- **O8.** The test-file lint edits (`compliance.rs`, `exemplar_gate.rs`, `compile_logic.tests.rs`, `shape_union_fresh.tests.rs`) are outside the issue's scope. They are harmless and were justified by `clippy-3.log`.

## Settled questions

### One lowering, one constructor, layering

- `lower_reifiers` is the only lowering. Its callers are `operator.rs:566` (reasoning world), `validation_view` (`statement_lowering.rs:194`) and the test harness's nested-residue refusal.
- `validation_view` is the only view constructor. Its callers are `store::shacl_validate_view` (`store.rs:119`, which the validate crate's store door, `shacl_validate_dataset` / `shacl_validate_view`, delegates to), `validate_all.rs:2000` and `:2125`, and `slice-brief/assemble.rs:596`.
- The old module is fully deleted. No reference to `gmeow_logic::statement_lowering` or `crate::statement_lowering` remains anywhere in the tree.
- No `Cargo.toml` or `Cargo.lock` changed. Every edge used (`logic`, `validate` and `slice-brief` → `logic-compile`; `slicetest` and `pipeline` → `validate`) already existed, so there is no new cycle.
- `logic-compile` gains only in-memory dataset construction plus `gmeow_errors`: no I/O, no orchestration.

### RDF 1.2 handling

- The `rdf:reifies` base-quad read is correct for the flat, unfolded form: `flat_dataset_from_quads` keeps the declaration as a base quad with a triple-term object.
- A non-triple-term object is skipped. That is not a reification.
- Each row keeps its reifier's graph: `reifier.graph` for the side table, `quad.g` for base quads.
- Double lowering is possible only for a dataset holding both forms; see O2.
- The operator's new named-graph branch is behaviour-equivalent. A reifier in a named graph was already refused by the reifier loop above it.

### Nested triple terms

These outcomes are visible:
- a Warning finding in consumer validate, LSP, the DSL pass, the dev gate (Phases 8 and 10) and stage-validate;
- an `Err` from a slice conformance cell that claims Conforms;
- a panic in the test harness;
- an observation entry in fixture validation.

Grade: a Warning is defensible. The residue is a declared `logic:expressivenessBoundary` (`logic:rdf12-nested-triple-term`), the reasoning CLI warns over the same residue, and the outcome is named, not silent. A refusal is reserved for a conformance claim.

The code itself is a validator Finding code, not a `gmeow-errors` Diag kind. The failure-class census in `repo_static.rs` does not apply to it; the validator's `codes.rs` authority does, and it is not met (R1).

### Leakage, digests and caches

- The view is local to the SHACL call.
  - `ShaclValidation` holds only the report and the residue.
  - The advisory split, span enrichment and the stage product use the authored dataset.
  - Nothing serializes the view.
- The projector emits `SELECT $this`, and the logic module gives the three predicates no domain or range axioms, so no derived shape has a lowered edge as its path. Whether any other shape anywhere in the union cites one over the real corpus has not been run.
- The stage version `validate.v15-…` and the example salt `…-lowered-statement-components-v4` are correct, but they are belt-and-braces: stage actions and validate caches already bind the implementation identity. The slice-spec action keys (`slicetest/src/repository.rs`) bind the implementation fingerprint too.
- The only survivor across the change is the on-disk recorded verdict; see O6.

### Error propagation

`shacl_validate_dataset` used to `expect()` engine failure; it now returns a typed `crate::error::Dataset` diagnostic. Every caller propagates the error with `?`, maps it to its own kind, or `expect`s it in tests.

## Production SHACL entry points (my own search)

| Entry point | Code | Lowered view? | Judgment |
|---|---|---|---|
| `gmeow validate` / `run` / `run_with` (CLI, MCP, `--deep`) | `data_validate.rs:560` → `Tier1Shapes::validate_dataset` → `store::shacl_validate_dataset` | yes | tested (`consumer_validation_…`) |
| `Tier1Shapes::validate` / `validate_json` (validate-wasm) | `data_validate.rs:124,230,248` | yes | `validate_json` path not tested; same door |
| LSP `shacl_report_via_ledger` | `data_validate.rs:398` | yes | not tested; same door |
| MCP prepared `Tier1Shapes` | `mcp/src/prepared.rs:105` → same | yes | by reading |
| DSL SHACL (`validate_dsl`) | `dsl_shacl.rs:41` | yes | not tested; same door |
| Dev gate Phase 8 merged (live) | `validate_all.rs:788` | yes | not tested; same door |
| Dev gate Phase 8 (recorded) | `dev_validate.rs:485` | via stage-validate record | O6 |
| Dev gate Phase 10 per example | `validate_all.rs:2000,2125` (`bind_view`) | yes | tested (`example_validation_sees_…`) |
| Shape oracle cross-checks | `shape_oracle.rs:1599` | yes (both sides) | residue dropped; defensible (differential) |
| stage-validate | `pipeline/stages/validate.rs:337` | yes | tested through `validate_source_graph` (not `ValidateStage::run`) |
| Pipeline fixture validation | `compile_logic/validation_fixtures.rs:113` | yes | not tested; O1 |
| Slice conformance cells | `slicetest/src/exec.rs:845` | yes | tested (`conformance_cells_read_…`) |
| Slice-brief exemplar gate | `slice-brief/src/assemble.rs:596-597` | yes | residue dropped (O5); not tested |
| Claim audit | `pipeline/src/scoreboards.rs:1135` | **no** | user data, production shapes: R3 |
| Inference-validation producer control | `pipeline/src/stages/conformance/inference_validation.rs:111` | **no** | full derived constraint union: R3 |
| Advice-wing producer control | `pipeline/src/stages/conformance/advice_wing.rs:80` | **no** | R3 |
| Flagship-unwired producer control | `pipeline/src/stages/conformance/flagship_unwired.rs:65` | **no** | R3 |
| Finance evaluation (`gmeow` passthrough) | `logic/src/finance_evaluation.rs:150` | no | defensible: `Records::dataset` is atomic IRI/literal triples; no reifier can reach it |
| Diagnostics migration differential | `compile_logic/validation_fixtures/migration.rs:90` | no | defensible: a legacy-vs-projected shape comparison over a synthetic population |
| Range-SHACL scoreboard | `pipeline/src/scoreboards.rs:2049` | no | defensible: vendored range shapes over transpile output, no `logic:` constraints |
| Test harness (`conformance_support`) | `validate/tests/conformance_support/mod.rs:290,557` | yes | test code |

Search method:
- `rg` over `crates/` (excluding tests) for `shapes::engine::`, `validate_dataset`, `PreparedShapes`, `bind_dataset`, `bind_shared_dataset`, `bind_view`, `bind_projected`, `validate_complete_dataset` and `validate_graphs`.
- A separate search of the `mcp*`, `lsp`, `*-wasm`, `gmeow-cli`, `conformance`, `cli-core` and `gmeow-dev-cli` crates for the consumer entry points.
- `crates/conformance` and `slice-quality` contain no SHACL engine call. The `engine::` export in `lang-bridge` is an unrelated NLP engine.

## Tests

| Test | Protects | Real entry point | Fails without lowering |
|---|---|---|---|
| `statement_lowering::…an_unfolded_rdf_reifies_quad_lowers_exactly_like_its_folded_form` | base-quad read; folded input not read twice | `lower_reifiers` | not in the teeth run; by construction yes |
| `data_validate::…consumer_validation_reads_rdf12_rankings_…` | consumer validation of the two standpoint constraints plus the nested warning | `Tier1Shapes::validate`, `run_with` | yes (teeth: line 877) — but on a **copy** of the constraints (R2) |
| `validate_all::…example_validation_sees_…` | Phase 10 wiring and the nested warning | `run_example_shacl` | yes (line 1685, components assertion) |
| `stages::validate::…stage_validate_sees_…` | stage-validate wiring and the nested warning | `validate_source_graph` | yes (line 895) |
| `slicetest exec::…conformance_cells_read_rdf12_…` | cell wiring; refusal of a Conforms claim over nested residue | `run_conformance_cell` | yes (line 487) |
| `conformance_suite::…conformance_harness_sees_…` | the test harness lowers flat N-Triples | harness only | yes (line 244); O4 |

Redundancy: the four wiring tests share one synthetic detector shape, but each protects a different entry point, so they are justified. No test covers slice-brief, LSP, `validate_dsl`, the Phase 8 live pass or fixture validation. All except slice-brief share the tested store door.

The teeth run (`teeth-without-lowering.log`, EXIT=2, 6 run, 0 passed) took place at 21:24-21:31. Every component assertion failed while the nested warning was still emitted, which shows the rows were removed while the residue was kept. The patch that removed the lowering is not recorded: `teeth.sh` only sets the filter.

## Evidence logs

Every log records `pwd`, the HEAD `bc4560473` (the base, not the commit), `git status --porcelain` showing the uncommitted modifications, the command and `EXIT`. None records a tree hash.

The last tracked edit (`shape_union_fresh.tests.rs`, 22:11:12) precedes `clippy-4`, `tests-3`, `lint`, `crate-check` and the commit (00:04:01). The worktree is clean at the commit. So the committed content matches what `tests-3` tested; this is inferred from file times and status, not from a hash.

| Log | Command | Exit | Result |
|---|---|---|---|
| `tests-3.log` | `make nextest-synthetic` with a focused filter (excludes gmeow-cli, gmeow-lsp, gmeow-gmn-wasm, gmeow-validate-wasm) | 2 | 225 run, 183 passed, 42 failed: 40 "`GMEOW_TEST_FIXTURE_MANIFEST` is required", 1 operator selector `NotPresent`, 1 "Source graph is empty" |
| `teeth-without-lowering.log` | `make nextest-synthetic` over 6 tests, lowering disabled | 2 | 0 of 6 passed (expected) |
| `clippy-4.log` | direct `cargo clippy --manifest-path … -p logic-compile -p logic -p validate -p pipeline -p slicetest -p slice-brief --all-targets -D warnings` | 0 | Not `make clippy`. gmeow-cli, lsp, mcp and the wasm crates were not linted |
| `crate-check.log` | `make crate-check` | 2 | 6 static violations (see (b)) |
| `lint.log` | `make lint` | 0 | pass, including the issue/PR reference lint |
| `producer-recipe.log` | `make producer-recipe` | 0 | prints a digest |
| `commit.log` | commit through hooks | 0 | |

`prose-check` over the added lines and the commit message: EXIT=0.

### (a) `validation_consumes_contextual_result_literals_and_quoted_evidence`

On this test's path the branch changes only `validate_parsed_source_graph` and the signature of `diagnostics_report`. For the test's empty source, `lower_reifiers` yields nothing, so the authored dataset is validated unchanged.

The failure is a `Frontend` "Source graph is empty" error. It comes from `require_nonempty_source` (`logic-compile/src/frontend.rs:7027`), reached through `PreparedLogicSource::new`, `parse_logic_dataset` or `lift_annotation_axioms`. None of those, nor their pipeline callers (`parse_sources.rs:278`, `meta_findings.rs:259`, `gate_verdict.rs:87`, `enrich`), is in the diff. The same failure appears in the teeth run, with the lowering removed. `validate.rs` and `validate.tests.rs` are identical between `bc4560473` and `origin/main`.

What I can conclude: by reading, the branch neither introduced the failure nor can affect it, and it is reachable at the merge base.

What I cannot conclude without a run: that it fails identically at the `origin/main` tip. Other files on the path may have changed in the 12 intervening commits.

### (b) `make crate-check`

The branch touches none of `Cargo.lock`, `crates/errors/src/model.rs`, `crates/gmeow-dev-cli/src/dev_build.rs`, `crates/pipeline/src/error.rs` or `crates/validate/src/repo_static.rs`. None of them differs between the merge base and `origin/main`, so the six violations are inherited from the base.

The branch adds:
- no `Result<_, String>`;
- no Diag kind: it reuses `validate::error::Dataset`, `slicetest` `ConformanceCell` and `ShapeValidation`;
- no dependency.

Its one new code is a Finding code, which those checks do not cover; its registration gap is R1.

## Read

- the issue and comments (`gh issue view 2831`);
- `.stage/research/2831-established-by-run.md`;
- the full branch diff;
- `statement_lowering.rs` (new);
- `store.rs`;
- the changed hunks of `data_validate.rs`, `validate_all.rs` (the merged and example phases and the cache key), `dsl_shacl.rs`, `shape_oracle.rs`, `pipeline/stages/validate.rs` (`validate_input`, `diagnostics_report`, the digest), `validation_fixtures.rs`, `slicetest/exec.rs`, `slice-brief/assemble.rs` and `operator.rs` (`cli_world_dataset`);
- every new or changed test;
- `docs/GATE-AND-PIPELINE.md` P3, P4 and P12;
- AGENTS.md and CLAUDE.md;
- `codes.rs` and `rule_catalog.rs` (classification and anchors);
- `dev_validate.rs` `recorded_merged_shacl`;
- `validate/cache.rs` and `pipeline/cache.rs` (implementation identity);
- `purrdf-core` `to_owned_term`, `owned_*` and `push_owned_*`;
- the standpoint constraints;
- the logic-module definitions of the three predicates;
- the producer control modules listed above;
- `finance_evaluation` validation and records;
- the claim-audit shape selection;
- every evidence log listed, plus `constraints.md`, `teeth.sh` and `run.sh`.

## Not read

- `docs/PIPELINE_SPINE.md` in full: I relied on the stage code for carrier attachment.
- The rust-work and rdf-work skill files.
- The `purrdf-shapes` engine internals for blank rendering (O7).
- The rest of `operator.rs`.
- Every one of the 25 constraints beyond the two standpoint ones.
- `scoreboards.rs` beyond the claim-audit and range functions.

## What nobody has shown

- `make check` / the slice-spec producer on this head. Now that stage-validate, Phase 10 and the conformance cells lower, the 23 other component-reading constraints are active over real slice data and real examples. Whether any finding newly appears or disappears in the corpus is unknown.
- The 40 corpus-backed tests in `tests-3` that refused for lack of a fixture manifest. Some, such as `no_preferred_claim_flags_crimea`, validate standpoint data that the lowering can change.
- The real standpoint constraints over RDF 1.2 data (R2).
- The produced (not per-constraint-projected) SHACL for those constraints.
- Compilation of gmeow-cli, gmeow-lsp and the wasm targets on this head. The signatures those crates call did not change.
- `ValidateStage::run` on the real corpus.
- The cost of the view copy (O3).
- Identical rendering of blank-node findings with and without the view (O7).
- The branch merged with current `origin/main`.

No Python was added. No hand-authored shape was added: the test detector shapes are synthetic and live in Rust tests. No issue or PR number or process wording appears in the committed files; I searched the diff, and `make lint` passed.

VERDICT: BLOCKED
