# Issue #1761: fix: classify extracted test modules outside the production repo-static census

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

## Problem

`make check` currently fails `crate-check` because the repository-static census treats Rust modules extracted behind `#[cfg(test)]` as production code. The failure reports direct GTS authorship, undeclared `gmeow:GtsProducer` call sites, and generated-path reads from files such as `*.tests.rs`, `run.*_tests.rs`, and `stages/*.tests.rs`.

This blocks every branch after the native source-extraction work even when its own focused checks pass. Adding declarations for synthetic test authors or weakening the production seals would encode the wrong ownership model.

Observed on current `main` descendant `1510a48a3` during `make check`: `crate-check` reported 81 violations. Representative false production classifications include:

- `crates/bundle-import/src/lib.tests.rs`
- `crates/pipeline/src/stages/carrier.logic_graph_golden_tests.rs`
- `crates/pipeline/src/run.dag_profile_tests.rs`
- `crates/validate/src/store.tests.rs`

## Required implementation

Make the repository-static production census use the Rust module/cfg boundary established by source extraction, so code reachable only through `#[cfg(test)]` is excluded from production-call-site seals. Do not rely on a filename-only allowlist and do not relax any seal for code reachable in a production build.

Retain the separate corpus-producer prohibition: excluding synthetic test modules from the production GTS-author census must not allow tests to build the GMEOW corpus.

## Acceptance criteria

- `make crate-check` passes the live repository after the legitimate non-test findings are resolved.
- A synthetic `#[cfg(test)]` extracted module that authors a small test GTS fixture is outside the production GTS-author and producer-declaration census.
- The same call moved into a production-reachable module still fails Seal A/B/C.
- Generated-path reads in test-only extracted modules do not trigger the production stale-disk-fold rule; the same read in production still fails.
- Existing corpus-producer scanner tests continue to prove that tests cannot invoke or indirectly reach corpus production.
- No path/name-specific suppression list is added.

Principles 4, 7, 12, 18.


## Comments (1)

### paudley — 2026-10-15T22:01:12Z

Repair the production census for extracted test modules, preserving every production seal (Principles 4, 7, 12, 18).

Reuse the existing first-party Rust module/cfg resolver in gmeow-build-inputs. Add a separate read-only ownership query, explicitly distinct from authenticated producer or native semantic identity. Resolve actual module declarations and paths across possible non-test cfg branches, retain any file with a production owner even when it also has a test owner, and report resolution failures rather than guess. Do not use filename allowlists, bless synthetic producers, or weaken the corpus-producer scanner.

Route only production-call-site scans through this ownership result: generated disk folds, raw/profile GTS authors, producer declarations, string-error types and diagnostic kinds. Keep unmanaged temporary-directory scanning over all code, including tests. Correct the existing composed-cfg blanker: any(test, feature=...) can run in production and must remain visible; all(test, ...) cannot.

Synthetic controls must prove extracted/nested test exclusion, production files with misleading test-like names, shared test/production ownership, composed cfg, production Seal A/B/C failures and generated-path failures, plus continued corpus-producer refusal. Refresh the two named live seal controls. Full crate-check still requires the separate genuine diagnostic/browser findings to be fixed. No full qualification or completion is inferred from this focused repair.


