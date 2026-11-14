VERDICT: PASS 4d376c7c112c9b7909ac24d3ad0df82b5c011685

# Review of PR 2786 at 4d376c7c1 (issue 2783: scheduled workflows, stale fuzz lock and missing tool installs)

Read-only review from `.worktrees/2783-scheduled-workflows-stale-fuzz-lock-and` (HEAD equals the stated head, one commit, 8 files). Nothing was built. The two dispatched runs and the PR CI run were read through `gh run view` logs.

## Requirement and delivery

| Requirement (issue 2783) | State |
|---|---|
| Re-sync `fuzz/Cargo.lock`, say what made it stale, add a durable locked-resolution check on the required path | Met. One stale edge removed (`gmeow-math-lift` under `gmeow-logic`), the cause is stated in the PR body, `make fuzz-lock-check` is its own target (`cargo metadata --locked --manifest-path fuzz/Cargo.toml`), `fuzz-substrate-check` depends on it, and the `helpers` job in `ci.yml` runs it. In PR CI run 43629478516 that step succeeded. |
| Install ripgrep in coverage and heavy "the way the main workflow does"; scanner test prints captured output on a failed first scan | Met. Same pinned `taiki-e/install-action@c509219cf...` with `tool: ripgrep` that `ci.yml` uses at lines 687-690 and 1035-1038 for the same scanner. The hosted log shows the action falling back to cargo-binstall (ripgrep 15.2.0), and the scanner then passed in both jobs ("corpus scanner regressions OK"). `accept()` prints the log and a status on a clean-fixture rejection. |
| Pinned benchmark runner plus prerequisites in the benchmarks job | Met. Valgrind by apt, runner version read from `Cargo.lock` (the awk extraction yields `0.19.4`, matching the lock entry at line 2118-2119 and the Makefile pin). `set -euo pipefail`, an empty-version guard and `command -v` end the step. `engines_instructions` then ran: "Gungraun result: Ok. 4 without regressions". |
| One dispatched run of each workflow in which the four jobs get past the named steps; what they report afterwards is read and recorded; any failure uncovered is filed | Run: done (43629554831 fuzz, 43629557143 suite-quality). Recorded and filed: NOT done. See R1. |

Non-failing check: the diff adds no `continue-on-error` or `|| true`. The one `continue-on-error: true` in `ci.yml:1564` is outside the diff.

Shell under `set -euo pipefail`: `accept()` uses `scan > log 2>&1 || status=$?`, which is safe under `set -e`, and exits 1 after printing. `gmeow_require_command` (`build-support/common.sh:8-16`) prints "<description> is required" and returns 2 when the command is absent. Evidence log 05 shows both messages and make exit 2. Correct.

## REQUIRED findings

### R1. The issue's evidence clause is unmet: results of the dispatched runs are neither recorded nor filed

The dispatched runs uncovered three new failures after the repaired steps. None is recorded in the PR (the body's "Not shown" section still says no scheduled job has run with this change and that what each job reports afterwards is unread, which is now false). None is filed: the issue list through 2788 has no matching issue, and issue 2783 has no comment.

1. fuzz (run 43629554831): `fuzz.yml:40` sets `timeout-minutes: 60`. The job passed the substrate check ("resolutions agree", log line 1215), then `make fuzz-smoke` was still compiling when the job was cancelled ("exceeded the maximum execution time of 1h0m0s"). The producer build inside `fuzz-substrate-check` took 27m28s locally, so the job cannot reach its fuzzing step inside the limit. The fuzz job therefore stays red or cancelled.
2. coverage and heavy (run 43629557143): after the scanner passes, both fail with `No rule to make target 'generated/projections/lang/gmn-codebook.cbor', needed by 'target/.gmeow-rust-ready.stamp'` (make exit 2).
3. benchmarks (same run): after the instruction benches pass, `conformance_union_cost_alloc` panics at `crates/validate/benches/cost_common/mod.rs:128` with `ShapeFiles(... generated/shapes, reason: MissingGenerated)` (make exit 101).

What would make it right: file one issue per distinct failure (with run id, log line and the failing step), link them from the PR, and replace the stale "Not shown" bullets with what the two runs reported (steps passed, steps failed, and the filed issue numbers in the GitHub body only, never in the repository). No code change is required by the issue for these; the issue asks that they be filed, not fixed here. If the fuzz timeout or the missing generated inputs are judged to be part of "get these jobs working", that is a scope ruling for the issue owner, but then the PR cannot claim the title's "keep ... red" is resolved.

## OPTIONAL findings

- O1. The PR body should state plainly that none of the four jobs is green after this change (fuzz cancelled, coverage and heavy failing at the gmn-codebook rule, benchmarks failing at MissingGenerated shapes), so the merge is read as "named steps repaired", not "jobs repaired".
- O2. The ripgrep install resolves through the cargo-binstall fallback because the pinned install action has no ripgrep manifest. That is the same behaviour `ci.yml` prebuild has, so it matches the issue wording, but its time and network cost are not pinned by version in the workflow (the main workflow's lint and nextest jobs pin `cargo install --locked --version 15.2.0`). Not needed by the contract.
- O3. The bench env `CARGO_PROFILE_BENCH_STRIP`/`DEBUG` is justified in the body as inferred from the maintainer target, not measured. The hosted run did get through `engines_instructions` with it, which is some evidence; record that.

## Limits (only a producer or hosted run can show)

Whether the three uncovered failures are pre-existing on main (not checked here; the issue's own wording says they are to be filed either way); whether fuzz-smoke finishes once the producer build is moved or cached; mutation testing (out of scope, failed in the same run); the `make lint` and scanner results are taken from the saved local logs, not rerun.

## Coverage

Read at the head:
- `.github/workflows/ci.yml` diff (added helpers step, lines 602-610) and grep of every `ripgrep` and `install-action` use (lines 640-646, 678-690, 1028-1038, 1254-1273).
- `.github/workflows/suite-quality.yml` diff and lines 40-60, 110-135, 165-180; `fuzz.yml` timeout (line 40).
- `Makefile` diff for `fuzz-lock-check` and `fuzz-substrate-check`; lines 1356-1367 (runner pin).
- `build-support/common.sh` 8-16; `Cargo.lock` 2118-2120.
- `scripts/test-corpus-producer-scanner.sh` diff (accept/reject), `scripts/lint-test-corpus-producers.sh` diff.
- `fuzz/Cargo.lock` and the two manifest comment diffs.
- Issue 2783 (body, no comments); PR 2786 body and comments (only the automated reviewer's text); issue list through 2788.
- `.stage/issue-2783/` logs 01-07 and 10; hosted logs `/tmp/review-2786-logs/fuzz.log` (1283 lines) and `suite.log` (7663 lines, failed steps).

Not read: the rest of `ci.yml` and `suite-quality.yml`; the mutation job's log beyond its failure; `crates/validate/benches` beyond the panic location; the generated-artifact producer rules for `gmn-codebook.cbor`.

## Recheck 2026-11-14 (same head 4d376c7c1; the PR head is unchanged, confirmed with `gh pr view`)

R1 asked for the results of the dispatched runs to be recorded in the PR and each distinct failure to be filed. Read: issues 2789, 2790, 2791 and the current PR body.

- 2789 (fuzz): names run 43629554831, the passed substrate check, the 60-minute limit in `fuzz.yml`, the roughly 28-minute producer build, and an acceptance (the substrate check stops needing the optimized producer, or takes it from a built artifact; a longer limit alone is called the weakest answer). Matches failure 1.
- 2790 (coverage, heavy): names run 43629557143, the exact `No rule to make target 'generated/projections/lang/gmn-codebook.cbor'` text, make exit 2, both jobs, and an acceptance that forbids running the producer inside a test job. Matches failure 2.
- 2791 (benchmarks): names the run, the panic location `crates/validate/benches/cost_common/mod.rs:128`, `MissingGenerated`, make exit 101, and an acceptance. Matches failure 3.
- PR body: the stale "Not shown" bullets about no job having run are gone. A new section "What the dispatched runs showed" names both run ids, says plainly that none of the four jobs is green and that the change repairs the named steps, not the jobs, and lists the three issues by number. It also records that `make fuzz-lock-check` passed in the helpers job of the PR's own run, which I had verified. The remaining "Not shown" bullets are accurate. Issue numbers appear only in the GitHub body, not in repository files.

Result: R1 is met. No REQUIRED finding remains; the verdict for this head is PASS. O1 is taken. O2 and O3 stay optional. The PR still says "Closes" the issue while three jobs stay red; the issue's own evidence clause accepts filed failures, so that is the owner's call and not a finding.
