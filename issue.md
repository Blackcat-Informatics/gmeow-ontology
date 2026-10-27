# Issue #2009: Reconcile the standalone fuzz lock with current path dependencies

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

Scheduled fuzz run 40460536877 fails before fuzzing starts: `cargo metadata --locked --manifest-path fuzz/Cargo.toml --format-version 1` refuses the stale standalone lockfile. The current path-package dependency graph includes edges absent from `fuzz/Cargo.lock`.

Reconcile the standalone lock with Cargo while retaining every selected package version, source, checksum and exact PurRDF revision. Preserve the locked metadata check and native production/fuzz substrate identity checker. Principles 4 and 7.

Acceptance:

- Change only `fuzz/Cargo.lock`, adding the dependency edges required by current path manifests. Inspect the complete machine-readable before/after package graph and retain all existing package identities.
- Verify locked standalone Cargo metadata succeeds without changing either lockfile, then run the real `make fuzz-substrate-check` with its native checker. A supported non-producer developer CLI build/override may execute this metadata-only command; no optimized producer, corpus construction, synchronization or full gate.
- Use normal hooks, source review and sequential delivery. No new parser tests are needed for dependency metadata repair. Do not weaken `--locked`, pin checks or workflow coverage.
- Local fuzz execution is outside this repair. Report it as unverified, along with CI under the standing waiver.

Observed failure: https://github.com/Blackcat-Informatics/gmeow-ontology/actions/runs/40460536877/job/122043799735 . This is a lock consistency failure, not evidence of a parser/fuzzer defect.


## Comments (0)

