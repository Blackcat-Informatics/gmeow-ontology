PASS at c0bc3c10e

# Delivery recheck: restored handle pin from the digest entry, and dataset payload bytes in the memory record

## Scope

This verdict covers the whole diff `9fb7090f5..c0bc3c10e` on branch
`paudley/2696-snapshot-digest-without-text`
(worktree `/home/paudley/Active/gmeow-ontology/.worktrees/2696-snapshot-digest-without-text`).
It changes 8 files, with 136 lines added and 19 removed.

- I reviewed the new delta `a46a5f241..c0bc3c10e` in full.
- For the rest of the diff I carried forward the code reading in
  `review-2696-snapshot-digest-at-a46a5f241.md`, because the delta does not change it.
- The delta touches three files: `crates/gmeow-dev-cli/src/dev_sync.tests.rs` (test only), plus
  one comment each in `crates/pipeline/src/cache.rs` and `crates/pipeline/src/memory_record.rs`.
- The delta changes no production code.

The earlier conclusions still hold for the whole diff:
- The restored pin is byte-identical to the old route. Both PurRDF 4.0.0 entries run the same
  `CanonState` with the Overlay presentation, the whole-dataset scope, SHA-256 and
  `RDFC_V2_CORPUS_POLICY`, and both return a `ContentDigest`.
- The error path and its diagnostic text are unchanged.
- Removing the `Arc` is harmless.
- The doc text on `dataset_payload_bytes` is accurate.
- The diff contains no issue or PR numbers and no Python.

## Evidence

### R1, re-judged: the JSON key now has a test

- **What the test now does.** The test is at `crates/gmeow-dev-cli/src/dev_sync.tests.rs:236-254`.
  It writes a `Complete` record whose product has these values:
  - quads 11
  - terms 7
  - `dataset_payload_bytes` 4 096
  - committed lane bytes 30
  - internal lane bytes 20

  It then asserts `written["product"]["quads"] == 11` and
  `written["product"]["dataset_payload_bytes"] == 4096`.
- **It fails if the key is misspelled or the line is removed.** In purrdf-lex 4.0.0,
  `Index<&str> for Value` (`json/value.rs:1077-1084`) returns `null` for a missing member. So a
  misspelled key, or deleting the `.with("dataset_payload_bytes", …)` line in `dev_sync.rs:779`,
  makes the left side `null`, and `null != 4096` fails the assertion.
- **It also fails if the wrong field is written.** All five product values are distinct, so
  writing another field under the key also fails.
- **It runs and passes at the new head.** `MemoryRecord` derives `Clone`, so
  `..returned.clone()` compiles. `nextest-r2.log` was taken at HEAD c0bc3c10e with a clean tree.
  It ran `make nextest-synthetic NEXTEST_FILTER='package(gmeow-dev-cli) & test(/stage_memory_observation_carries/)'`
  and reports 1 test run, 1 passed, **EXIT=0**.

### R2, re-judged: the corrected commit message

- The c0bc3c10e message says: "Of 189 selected tests 186 passed and 3 failed, none of them in
  code that commit changed."
- This matches `nextest-final.log` at a46a5f241: 189 run, 186 passed, 3 failed.
- The three failures are:
  - `quality_assessment_fanout_path_is_registered_and_folds_as_ntriples`: producer selector not present.
  - `advice_entries_are_projected_for_realized_carriers`: no fixture manifest.
  - `cache_replay_yields_byte_identical_run_ledger`: panics in `run_validate` before it reaches the cache.
- None of them is in code the branch changed. The correction is accurate.

### The two changed comments

- **`cache.rs:1573-1574` is true as written.** It says the pin comes from the live projection and
  the typed payload is rebuilt from its persisted bytes and checked against its own recorded
  digest. The code matches:
  - The pin comes from `restored_handle_graph_pin(&subgraph)`.
  - The payload comes from `rebuild_handle(&h.arm, h.typed_payload…)`.
  - `handle_payload_digest(&payload)` is compared to `h.payload_digest`, and a mismatch is a
    `CacheMismatch`.

  This fixes my earlier O3.
- **`memory_record.rs:133-136` is true under PurRDF's own definition.** It says PurRDF states an
  index capacity only for a query graph, which a stage carrier is not.
  - The only such figure is `RdfDataset::query_retained_bytes`, at purrdf-core
    `dataset.rs:2433-2470`. It is documented as the "retained capacity of a freshly materialized
    query graph", counted with every lazy index already built.
  - Within PurRDF, the only callers I found are its SPARQL result paths.
  - GMEOW never calls it (`git grep` finds nothing).
  - See O3 below for one precision point.

### Clippy at the new head

- `clippy-r2.log` was taken at HEAD c0bc3c10e. The command was
  `cargo clippy --locked -p gmeow-dev-cli -p gmeow-pipeline --all-targets -- -D warnings`, and it
  ended with EXIT=101.
- It reports exactly nine errors:
  - `crates/pipeline/src/stages/compile_logic.tests.rs:453` and `:457`.
  - `crates/pipeline/src/stages/shape_union_fresh.tests.rs:60, 64, 69, 90, 95, 122`.
  - `crates/gmeow-dev-cli/src/lib.rs:149`, from the `large_unopt_stack_frames` lint on the `Subcommand` derive.
- None of those three files is in the branch's diff.
  `git diff --quiet 9fb7090f5..c0bc3c10e -- <the three files>` exited 0, so all nine errors are
  in lines this branch did not change.
- The `lib test` targets of both crates, which contain `dev_sync.tests.rs`, `cache.tests.rs` and
  `tests.rs`, report no other lint. The changed code therefore added no clippy errors.

### Format and commit hooks

- `fmt-r2.log` shows `make fmt` exiting 0 on the dirty tree before the commit.
- `commit-r2.log` shows these hooks passing: `cargo fmt --check`, codespell, the issue/PR
  reference lint, and the check that rejects corpus producers reachable from tests. The commit
  then went through with EXIT=0.

### Effect on the earlier focused run

The wider focused run at a46a5f241 was not repeated, and the delta cannot change any of its
results:
- The delta changes no production code.
- Its only executable change adds assertions inside
  `stage_memory_observation_carries_allocator_totals_and_the_reclaim_figures`, which was re-run
  and passed.
- The two comment edits do not compile into anything.

## Required findings

None.
- R1 is resolved by the extended test, which fails on a missing, misspelled or wrong key.
- R2 is resolved by the accurate correction in the c0bc3c10e message.
- The clippy failures and the 3 failing tests all predate the branch and lie outside the diff.

## Optional findings

- **O1 (carried forward).** `cache.tests.rs:2544` could also assert that the canonical document
  contains `urn:purrdf:rdfc:`. That would keep the test able to tell the Overlay and
  FlatAssertion presentations apart even if the parser stopped folding the reifier and
  annotation rows.
- **O2 (carried forward).** At `cache.rs:1570`, `subgraph` stays alive through `rebuild_handle`
  and `pin_handle`. Scoping it to the pin computation, or taking the pin from
  `try_graph_digest_view` without building the projection, would free a projection per handle
  earlier.
- **O3 (new, precision).** PurRDF decides structurally whether `query_retained_bytes` returns a
  figure. It returns `None` only when the dataset has locations, content ids, a content scheme
  or a derivation predicate; it does not check where the dataset came from. A plain stage
  carrier with none of those would get `Some(n)`. That number would still be the capacity of a
  graph with every lazy index built, not what the carrier actually holds, so the comment's
  conclusion stands. A tighter wording would be: "PurRDF states only the capacity a freshly
  materialized query graph would retain with every lazy index built".
- **O4 (carried forward).** The a46a5f241 message says resident memory falls only with a
  streaming PurRDF entry. That understates the change: the old canonical document and label
  table lived until `pin_handle` ran, and now they are freed earlier. The PR body should say
  this and mark it unmeasured. It should also give the clippy status (exit 101, with every
  error in a file outside the diff) next to the corrected test counts. The new message
  corrects only the test claim. The earlier message never claimed clippy passed.
- **O5 (carried forward).** `ReleasedFootprint` carries no `dataset_payload_bytes` delta. It can
  be derived from the publication and release records.

## Not verified

- I ran no command beyond `git diff`, `git diff --quiet` and reading source and logs. I
  executed no test or clippy myself, and ran no `make check`, producer, gate or reasoning run.
- I did not run the misspelled-key failure. The conclusion comes from reading purrdf-lex's
  `Index` implementation.
- I did not determine whether any real stage carrier lacks every sidecar that
  `query_retained_bytes` checks (see O3).
- I did not re-run at 9fb7090f5 or main to confirm that the 3 failing tests also fail there. My
  "outside the diff" classification comes from where each test panics and which files the diff
  touches.
- I did not check whether the repository's `make clippy` uses the same lint set as the raw
  `cargo clippy` run in the logs.
- Nothing measures the memory effect at corpus scale.
- No issue.md, plan or `## User rulings` was supplied. I checked scope against the
  coordinator's brief only.
