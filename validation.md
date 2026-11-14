# Validation at the merging head dcb0e08c5595bfcd99ee1fb4b21f92df513c96c4 (written 2026-11-14 13:25 CST)

## What the head is
`46902b851` plus one merge of `origin/main` at `f71169a78`. `git diff --name-only 46902b851 dcb0e08c5` names no
file under `crates/logic`, `crates/logic-compile`, and not `crates/pipeline/src/stages/meta_findings.rs`: the
merge changed none of the code this pull request changes.

## Review
- Independent reviewer: `VERDICT: PASS 46902b851` (reviews/review-pr2779-at-46902b851.md), coverage
  `04aa28a7e..46902b851`. Earlier verdicts at `fdca3d769` and `23a6f7910` were BLOCKED; their required
  findings are fixed in the later commits.
- Read by me at the merging head: every production line of the change (`frontend.rs`,
  `frontend/source_admission.rs`, `stages/meta_findings.rs`). Not read by me: the two test files.
- Review bot: three comments, each answered; the last correction is in `46902b851`. The bot did not review
  the merge commit (its check reports skipped).

## Run on `46902b851` (logs/*9.log; not rerun on the merge, which changes none of these crates)
- `cargo fmt --check`: 0. Strict clippy on `gmeow-logic-compile`: 0. `producer-recipe`: 0.
- `gmeow-logic-compile` tests: 1094 run, 3 failed, the same three names as the base (`baseline.log`).
- `gmeow-pipeline` tests at `23a6f7910`: 1334 run, 257 failed, identical by name to the base
  (`pipeline-baseline.log`); the new meta-finding test passes.
- `msgfmt -c` on both changed catalogs: 0 (also on the merged tree).

## Hosted checks at the merging head
- Finished and green: `helpers`, `lint`, `producer-build`, `rust-prebuild`, `license/cla`.
- NOT finished when this merge was made: `generation (a)`, `generation (b)`, `fixture-prefix`. These three
  end red on `main` itself at `f71169a78` (`generation (a)`: "The operation was canceled";
  `generation (b)`: "Process completed with exit code 143") and at `04aa28a7e`, because the producer exceeds
  the hosted runner's memory. The merge was not held for them, on coordination's rule of 2026-11-14 13:12 CST
  that a pull request with an independent pass merges at once.

## Not shown
- No producer run on any head of this pull request. The derived artifacts of the changed usage note and the
  effect on the full corpus are unchecked here.
- That the meta-finding test fails without the fix was shown by the reviewer, not by me.
