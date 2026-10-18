# Issue #1644: gate-profile: promote or ratify the off-gate proof lanes

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: documentation, enhancement, cross-cutting, quality, rust, complexity-medium

## Body

Drained from the local `.deficiencies` ledger (recorded across #930, #1546, #1570, #1580). Nothing reads that file, so these forward paths were never tracked. Re-verified against current `main` before filing.

## Problem

A large set of the repo's most load-bearing proofs do not run on the per-commit gate. `.config/nextest.toml:13-52` excludes **37 selectors** from `default-filter`, including:

| Excluded | What it proves |
|---|---|
| `gmeow-pipeline & binary(fold_parity)` | the sink fold matches the committed per-named-graph bundle (`sink_fold_matches_committed_per_named_graph`) |
| `gmeow-validate & binary(conformance_math_producers)` | the math producers' SHACL conformance (15/15) |
| `gmeow-conformance & binary(full_divergence_gate) & /_heavy_offgate/` | the 154-case soundness scaling proof (`never_a_wrong_decided_verdict_heavy_offgate`, `native_reproduces_the_frozen_gap_heavy_offgate`) |
| `gmeow-conformance & binary(full_decided_gate) & /_heavy_offgate/` | the decided-verdict corpus |
| `gmeow-slicetest & run_structural_file::core/slice-quality-rubric/tests/structural.ttl` | every ratchet-cell well-formedness invariant, incl. the `saProjectionCeilingNotDoubleSliced` MUST-NOT counter-example |
| `gmeow-docs & binary(mdbook_render)` (5 tests) | the emitted mdbook source tree |

**Sharper finding — one audit runs in no lane at all.** `every_real_slice_batch_matches_parallel_scratch` (the exhaustive all-slice batch-vs-scratch reasoner parity audit) is excluded by a Rust attribute, not the filter: `crates/slice-quality/src/reasoner.rs:741` carries `#[ignore = "exhaustive repo audit; ..."]`. `[profile.maint-heavy]` sets `default-filter = 'all()'` (`.config/nextest.toml:59`), but `all()` does **not** un-`#[ignore]` a test. So the exhaustive audit executes in neither `make check` nor `make maint-rust-heavy`. Its only stand-in is the curated 4-slice `curated_real_slices_batch_matches_scratch` (`reasoner.rs:723`).

**Related fragility.** `ACCEPTANCE_MIN_RECALL_PCT = 64.5` (`crates/pipeline/src/scoreboards.rs:766`) sits against a measured corpus-aggregate round-trip recall of 64.57 % — 0.07 pp of headroom, so ordinary corpus drift reds the gate.

## Work

This is a repo-wide gate-placement decision, not a per-PR one. For each excluded lane, choose exactly one and record the choice where the exclusion lives:

- [ ] **Promote on-gate** — if it can be made to fit the budget.
- [ ] **Move to `maint-heavy`** — and for `#[ignore]`d tests, actually make `maint-heavy` run them (an `#[ignore]` is invisible to `default-filter = 'all()'`; it needs `run-ignored = "all"` or the attribute removed under that profile).
- [ ] **Ratify the on-gate proxy in writing** — name the proxy and what it does not cover, next to the exclusion.

- [ ] Decide whether `every_real_slice_batch_matches_parallel_scratch` should run anywhere; today it does not.
- [ ] Re-baseline or widen the headroom on `ACCEPTANCE_MIN_RECALL_PCT`, or document the 0.07 pp margin as intended.

## Acceptance

- Every entry in the `default-filter` exclusion list carries an inline justification naming its on-gate proxy or its off-gate home.
- No test is excluded from every lane.


## Comments (5)

### coderabbitai — 2026-07-27T00:17:38Z

<!-- This is an auto-generated issue plan by CodeRabbit -->
<details>
<summary>🔗 Related PRs</summary>

Blackcat-Informatics/gmeow-ontology#1052 - feat(1045): fast, budget-enforced Rust test suite — shard corpus_parity, 25s gate, off-gate heavy tests [merged]
Blackcat-Informatics/gmeow-ontology#1519 - perf: optimize native reasoning and gate composition [merged]
Blackcat-Informatics/gmeow-ontology#1547 - perf: cut bundle startup and repo-scale gate costs [merged]
Blackcat-Informatics/gmeow-ontology#1583 - ci(logic): remove the downstream PurRDF entailment cross-check [merged]
Blackcat-Informatics/gmeow-ontology#1586 - feat(1459): native ontology-surface authoring gates [merged]
</details>

---
<details>
<summary>📝 Issue Planner</summary>

<sub>Check the box below or use the `@coderabbitai plan` command to generate an implementation plan and prompts that you can use with your favorite coding assistant.</sub>

- [ ] <!-- {"checkboxId": "8d4f2b9c-3e1a-4f7c-a9b2-d5e8f1c4a7b9"} --> Create Plan
</details>


---
<details>
<summary> 🧪 Issue enrichment is currently in open beta.</summary>


You can configure auto-planning by selecting labels in the issue_enrichment configuration.

To disable automatic issue enrichment, add the following to your `.coderabbit.yaml`:
```yaml
issue_enrichment:
  auto_enrich:
    enabled: false
```
</details>

💬 Have feedback or questions? Drop into our [discord](https://discord.gg/coderabbit)!

### paudley — 2026-09-05T03:05:14Z

## Re-verified against `main` @ c2b03aba6 — the "sharper finding" is now stale

Checked while inventorying open issues. Most of this issue verifies exactly; one part does not.

**Still accurate:**

- `.config/nextest.toml` excludes **37** selectors from `default-filter`.
- `ACCEPTANCE_MIN_RECALL_PCT = 64.5` (`crates/pipeline/src/scoreboards.rs`) — unchanged, still 0.07 pp from the measured aggregate.
- `run-ignored` is **not** set anywhere in `.config/nextest.toml`, so `[profile.maint-heavy]`'s `default-filter = 'all()'` still does not un-`#[ignore]` anything. The mechanism this issue identified is intact.

**No longer accurate — the named example is gone:**

> `every_real_slice_batch_matches_parallel_scratch` ... is excluded by a Rust attribute ... `crates/slice-quality/src/reasoner.rs:741`

Neither `every_real_slice_batch_matches_parallel_scratch` nor its curated stand-in `curated_real_slices_batch_matches_scratch` exists anywhere in the tree:

```
$ git grep -l 'batch_matches_parallel_scratch\|batch_matches_scratch' origin/main -- 'crates/**/*.rs'
(no matches)
$ git show origin/main:crates/slice-quality/src/reasoner.rs | grep -c '#\[ignore'
0
```

So the exhaustive all-slice reasoner-parity audit is not "running in no lane" — it is not in the tree at all. Whether it was deliberately retired or lost is worth establishing before this issue is worked, because "restore the audit" and "put the existing audit in a lane" are different tasks.

**The general point survives with different examples.** Five files still carry `#[ignore]`d tests that no profile runs:

```
crates/gmeow-dev-cli/tests/cli_parity.rs
crates/gmn-cost-matrix/src/lib.rs
crates/lang-bridge/tests/conllu.rs
crates/lang-bridge/tests/ontolex.rs
crates/mcp/src/lib.rs
```

Suggest re-anchoring the "sharper finding" section on those, and adding "confirm whether the batch-vs-scratch audit should be restored" as its own line item.

*Method: existence and configuration checks against `origin/main`, not a run of this issue's acceptance criteria.*

### paudley — 2026-09-05T22:36:36Z

## Re-verified against `main` @ `65a23b99c` — most of this issue is already done

I checked every claim in the body before acting. The bulk of the exclusion table has been promoted on-gate since filing; what survives is one structural defect, which PR #1721 fixes.

### The exclusion table is stale — 5 of 6 rows no longer exist

`.config/nextest.toml` now carries **16** exclusion clauses, not 37. Grepping the live config:

| Claimed exclusion | On `main` today |
| --- | --- |
| `gmeow-pipeline & binary(fold_parity)` | **absent** — 0 hits |
| `gmeow-validate & binary(conformance_math_producers)` | **absent** — 0 hits |
| `gmeow-conformance & binary(full_decided_gate) & /_heavy_offgate/` | **absent** — 0 hits |
| `gmeow-slicetest & …/slice-quality-rubric/tests/structural.ttl` | **absent** — 0 hits |
| `gmeow-docs & binary(mdbook_render)` (5 tests) | **not excluded** — it appears only as a `slow-timeout` override for one test |
| `gmeow-conformance & binary(full_divergence_gate) & /_heavy_offgate/` | still excluded (line 19) |

So the fold parity, math-producer conformance, decided-verdict corpus, and ratchet-cell structural invariants all run on the per-commit gate now.

### The "sharper finding" is also stale — but its *mechanism* was real and still live

`every_real_slice_batch_matches_parallel_scratch` **no longer exists anywhere in the workspace** (0 hits), nor does `curated_real_slices_batch_matches_scratch`. That specific audit cannot be laneless because it is gone.

But the underlying mechanism the issue identified was correct and still applied to **every other** `#[ignore]`d test: `default-filter = 'all()'` does not un-`#[ignore]` anything, and **`run-ignored` appeared nowhere in the repository**. Five `#[ignore]`d tests therefore ran in no lane at all, and one of them carried the attribute text `"heavy lane only"` plus a doc comment saying *"run in the heavy lane"* — a false claim about what the build enforces (P11).

### `ACCEPTANCE_MIN_RECALL_PCT` — still 64.5, at `scoreboards.rs:770` (not 766)

Left alone deliberately. Re-baselining a floor is a ratchet move and P10 says ratchets move down only; widening headroom without a measured cause would be exactly the "re-threshold until it stops reporting" that P7 forbids. If the 0.07 pp margin actually reds a build, that red is the evidence needed to act, and it belongs in its own issue with the measurement attached. I have no such measurement.

### What PR #1721 does

- `run-ignored = "all"` on `[profile.maint-heavy]`, plus the 900 s budget the MCP deep test's own comment says it needs — without which enabling it would just red by timeout (P7).
- Two families stay out of the breadth lane, excluded **by name** so the decision is visible: the two lang-bridge corpus sweeps (hard-fail without CC BY-SA / third-party data that deliberately does not ship) and `measurement_is_deterministic_across_two_runs` (calls `run_pipeline_products` twice — a test may never rebuild the corpus, P1). Each gets its own `make` target, so neither is laneless.
- Deleted `wikidata_existence_live_lookup`: `#[ignore]`d **and** opening with `if env::var("GMEOW_RUN_NETWORK") != Ok("1") { return; }`, so it could not fail on either path (P8), and its body duplicated `make maint-wikidata-live`.
- `every_ignored_test_is_reachable_from_some_lane` makes the acceptance criterion *"No test is excluded from every lane"* mechanical rather than aspirational.

### Remaining scope

The first checkbox — *"every entry in the exclusion list carries an inline justification naming its on-gate proxy or its off-gate home"* — is **not** done for the 16 surviving per-commit clauses. They are covered by one block comment at the head of `default-filter` rather than per clause. That is the honest remainder of this issue and I have left it open.

### paudley — 2026-09-06T01:23:08Z

PR #1721 landed (`d53f4e5c7`). The structural defect is fixed: `make maint-rust-heavy` now passes `--run-ignored all`, every `#[ignore]`d test is reachable from a named lane, and `every_ignored_test_is_reachable_from_some_lane` + `the_breadth_lane_actually_runs_ignored_tests` keep it that way.

Worth recording, because it is the same class of bug this issue is about: I first wrote `run-ignored = "all"` into `[profile.maint-heavy]`. It is **not a profile key** — nextest emits `warning: ignoring unknown configuration key` and proceeds — so a full `make check` passed green over a config that changed nothing at all. A warning is not a gate. The contract test now forbids the key in the config and requires the flag on the recipe.

**Still open, deliberately:**

- [ ] Per-clause justification for the 16 surviving `default-filter` exclusions. They share one block comment at the head of the filter; the acceptance criterion asks for an inline justification naming each one's on-gate proxy or off-gate home.
- [ ] `ACCEPTANCE_MIN_RECALL_PCT` headroom (`scoreboards.rs:770`, not 766). Untouched on purpose: re-baselining a floor without a measured cause is the re-thresholding P7 forbids, and P10 says ratchets move down only. If the 0.07 pp margin actually reds a build, that red is the evidence — and it deserves its own issue with the measurement attached.

The exclusion-table rows and the `every_real_slice_batch_matches_parallel_scratch` finding are stale and need no action; details in the comment above.

### paudley — 2026-09-06T11:54:21Z

PR #1726 landed. The first acceptance criterion is now met: every one of the fifteen per-commit exclusions carries its own numbered justification, and `every_default_filter_clause_is_justified` pins each reason to its clause positionally so they cannot drift apart.

One correction to the criterion itself, worth recording: it asks for an **inline** justification, and that is not implementable. A nextest filterset is an expression, not a config table, and rejects `#`:

```
error: for config file .config/nextest.toml, failed to parse profile.default.default-filter
  error: expected expression
```

The numbered list immediately above the filter, plus a test enforcing the pairing, is the closest honest equivalent.

Where a required sibling genuinely exists it is named; where I could not verify one, the entry names the off-gate home instead of inventing a proxy — the criterion allows either, and a fabricated proxy would be exactly the defect this issue exists to remove.

**Remaining on this issue:** only the `ACCEPTANCE_MIN_RECALL_PCT` headroom, which I have deliberately left alone. Re-baselining a floor without a measured cause is the re-thresholding P7 forbids, and P10 says ratchets move down only. If that 0.07 pp margin ever actually reds a build, that red is the evidence, and it deserves its own issue with the measurement attached.

