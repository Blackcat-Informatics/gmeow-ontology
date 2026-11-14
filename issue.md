# Issue #2783: Scheduled workflows: stale fuzz lock and missing tool installs keep fuzz, coverage, heavy tests and benchmarks red

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

Four scheduled jobs are red on `main` (`04aa28a7e`) for causes that sit in the lock file and in the workflow, before any test or measurement starts. Read from the failed-step logs of fuzz run 43565106463 and suite-quality run 43567108172 (both 2026-11-14); nothing here was reproduced locally. The oldest failing run listed for both workflows is 2026-11-08; the cause of earlier runs was not read.

## What fails

1. **fuzz** stops at `make fuzz-substrate-check`: `cargo metadata --locked --manifest-path fuzz/Cargo.toml` reports that it cannot update `fuzz/Cargo.lock` because the lock is enforced. The committed lock no longer matches the manifests it resolves. `fuzz/Cargo.lock` was last changed on 2026-11-08.
2. **rust coverage** and **rust heavy off-gate tests** both stop at `make test-corpus-scanner` with status 127 and no message. `scripts/test-corpus-producer-scanner.sh` runs `scripts/lint-test-corpus-producers.sh` with its output redirected to a file under `set -e`; that script calls `rg`. The main workflow installs ripgrep before the same check in three jobs; `suite-quality.yml` does not install it in these two. The cause is inferred from the script and the two workflow files: the log shows only the status.
3. **benchmarks** stops in `make bench` at `benches/engines_instructions.rs`: the instruction-count benchmark cannot start its runner binary ("Is gungraun-runner installed and gungraun-runner in your $PATH?"). The job installs neither that binary nor what it needs to run.

## Required

- Bring `fuzz/Cargo.lock` back in step with its manifests, and say in the change what made it stale so the same edit does not leave it behind again (a check on the required path that runs the same locked resolution is the durable form, if one does not exist).
- Install ripgrep in the coverage and heavy jobs the way the main workflow does, and make the scanner test print the captured output when its first scan fails, so a missing tool is named instead of a bare status.
- Install the benchmark runner (pinned) and its prerequisites in the benchmarks job.
- Evidence: one dispatched run of each workflow on the branch in which these four jobs get past the steps named above. What they report after that point is read and recorded, and any failure it uncovers is filed, not waved through as report-only.

## Not in this unit

The mutation-testing job fails for a different reason (tests of the validate crate fail in the unmutated tree); that is its own issue.


## Comments (0)

