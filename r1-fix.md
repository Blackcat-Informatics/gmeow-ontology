# R1 and O3 fix on PR 2691 (issue 2674)

- Timestamp: Sat 14 Nov 2026 06:04:53 PM CST
- Old head: `750ca179b` (confirmed, worktree clean before editing)
- New head: `c75d1c6e4acbc9e3448254be4e953e25121a36c1` (the forge reports the same head)
- Worktree: `.worktrees/2674-repair-estimate-provenance-model`

## Item 1 (R1): before and after

`slices/grounding/math/tests/competency.ttl:1633`, `cqRationale` of `ex:cqStatisticsEstimateProvenance`:

- Before: "a run that used several models yields a row for each."
- After: "a run that used several models yields a row for every used model that carries both its data (math:fittedToData) and an assumption (math:modelAssumption), and a used model that lacks either yields no row."

`slices/grounding/math/tests/competency.ttl:1665`, `cqRationale` of `ex:cqStatisticsEstimateProvenanceAmbiguity`:

- Before: "runB used modelM1 and modelM2, so estimateE2 yields a row for each,"
- After: "runB used modelM1 and modelM2, each carrying its data and an assumption, so estimateE2 yields a row for each,"
- The old sentence was true of the fixture. The added clause states the condition the general lead-in ("a run that used several models keeps its ambiguity") depends on.

Other places searched with `git grep` (patterns: "row for each", "rows for all", "several models", "multi-model", "multiple models", "uniquely produced", "declared as used", "each model it"):

- The `.rq` header already says the true thing (lines 14-19). Unchanged.
- `MATHEMATICS-STATISTICS.md:142-145`, `module.ttl:8241` and the catalog entries say "records each model it applied". That is a statement about the run, not about query rows. Unchanged.
- `statistics-estimate-provenance-ambiguity.ttl:13` ("both models are reported") is about that fixture, whose header says each model has its own assumption. Unchanged.
- `docs.md` and the examples do not repeat the claim.

Catalogs: `cqRationale` literals have no entry in `slices/grounding/math/i18n/fr.po` or `zh.po` (0 matches for the old sentence, and no `cqRationale` text in the catalogs). No catalog entry was added or changed.

## Item 2 (O3): decision and reasoning

Decision: made both changes. No expected result changes.

- `slices/grounding/math/tests/fixtures/statistics-result-lineage.ttl:25`: added `gmeow:used ex:trialDataset ;` on `ex:analysisRun`. Line 23 comment now reads "the analysis run that read the dataset (gmeow:used) and produced the estimate."
- `slices/grounding/math/design/MATHEMATICS-STATISTICS.md:253`: added `gmeow:used ex:trialDatasetV3 ;` on `ex:analysisActivity` in the example.

Query by query:

- `statistics-result-lineage.rq` is the only consumer of the lineage fixture (`competency.ttl:1819-1820`; no other reference in the repository). Its body binds the estimate, estimand, estimator, value, run, observation and vantage. It reads neither `gmeow:used` nor any `math:DatasetMatrix`, and the new triple adds no second binding for any projected variable. The single expected row is unchanged.
- `statistics-estimate-provenance.rq` is the only `.rq` file in the repository that reads `gmeow:used`. It never sees the lineage fixture: `crates/slicetest/src/exec.rs:329-333` overlays each question's own `cqDataFile` on a fresh union and leaves the shared base unchanged. Even if it did see it, `ex:trialDataset` is not typed `math:FittedModel` and has no `math:fittedToData` or `math:modelAssumption`, so it could yield no row. The ambiguity fixture already proves that case with `sharedData`.
- The lineage question runs in the `reasoningNone` lane, so no entailment from the new triple is involved.
- The design-note example is prose. `ex:analysisActivity` and `ex:trialDatasetV3` occur nowhere else in the repository, so no query or fixture reads it.
- The pattern (an inference run `gmeow:used` a `math:DatasetMatrix`) is the one already in `tests/conformance-fixtures/pvalue-tri-slice.ttl:116-117`.

Limit: these are readings of the queries. No competency run, SPARQL engine, producer or gate was run.

## `git diff --stat 750ca179b..c75d1c6e4`

```
 slices/grounding/math/design/MATHEMATICS-STATISTICS.md             | 1 +
 slices/grounding/math/tests/competency.ttl                         | 4 ++--
 slices/grounding/math/tests/fixtures/statistics-result-lineage.ttl | 3 ++-
 3 files changed, 5 insertions(+), 3 deletions(-)
```

## Hook, push, PR body

- Commit under `flock .stage/build.lock`: exit 0. It waited for the other lane's lock. Hooks that ran on the staged files passed (whitespace, end of files, merge and case conflicts, large files, line endings, codespell, markdownlint, secrets, issue/PR reference lint); the Rust, YAML and corpus-producer hooks had no files to check. Log: `.stage/issue-2674/r1-commit.log`.
- `prose-check`: exit 0 on the design note, the commit message and the PR body file.
- `git push`: exit 0, `750ca179b..c75d1c6e4`. Log: `.stage/issue-2674/r1-push.log`.
- `gh pr edit 2691 --body-file .stage/issue-2674/pr-body-r1.md`: exit 0. Original body kept at `pr-body-r1.orig.md`.

PR body sentences changed:

1. Line 9. Before: "and a run that used several models yields rows for all of them." After: "and a run that used several models yields rows for every used model that carries both its data and an assumption. A used model with no `math:fittedToData` or no `math:modelAssumption` yields no row."
2. Line 23. Before: "removed from `statistics-result-lineage.ttl` (its query does not read the triple, so rows are unchanged) and from the run example in `design/MATHEMATICS-STATISTICS.md`." After: "in `statistics-result-lineage.ttl` and in the run example in `design/MATHEMATICS-STATISTICS.md`, the run now `gmeow:used` the dataset instead, the same way as the p-value files. The lineage query reads neither `gmeow:used` nor the dataset, so its row is unchanged." This sentence became false with this commit, so it was updated too.

## Not done

- The PR body's verification section still describes earlier heads (`5c55e409e`, `2c015ee2d`); it was not rewritten for `c75d1c6e4`.
- Hosted checks for `c75d1c6e4` were not read.
- Review items O1, O2, O4 and O5 are outside this task and untouched.
