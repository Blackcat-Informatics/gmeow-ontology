# Independent source review: hosted reviewed-coverage refresh

Disposition: no actionable finding in the single workflow support unit. Read Stage plan, workflow, existing Make target, explicit example producer, native PO coverage owner and frozen-golden consumer. No tests, checks, builds, host changes, catalog sweeps, local refreshes or corpus/producer operations were run by this reviewer.

## Selection and production boundary

The workflow is manual-only (`workflow_dispatch`), with repository `contents: read`. Checkout uses the immutable dispatch `github.sha` and disables persisted credentials. There is no push/schedule/PR trigger, auto-commit, write permission or alternate implementation. Concurrency queues runs rather than canceling an active run.

The selected command is the existing `make maint-refresh-reviewed-coverage-golden`. Its recipe invokes only `cargo run -p gmeow-docs --example refresh-reviewed-coverage-golden`. That example calls `gmeow_docs_model::reviewed_coverage::reviewed_coverage_map`, then writes compact sorted-map JSON plus a newline to the shared golden path. The coverage owner reads every live slice `i18n/*.po`, uses the existing native `parse_po`/language/reviewed-entry policy, and propagates discovery/read/parse/write failures. This workflow does not invoke the authenticated corpus producer or produce a substitute observation inside tests.

## Output and failure behavior

The upload step has ordinary success-only execution. A failed refresh therefore prevents upload of the tracked pre-existing golden; no `always`, `continue-on-error`, or fallback creates a stale-success artifact. `if-no-files-found: error` independently rejects a missing selected output. Only `crates/docs/tests/fixtures/reviewed_coverage_golden.json` is uploaded, under `reviewed-coverage-${{ github.sha }}`, binding the review artifact name to the exact checked-out dispatch source. Successful execution is still an explicit artifact requiring review and normal source-control qualification; no upload commits or blesses it.

Checkout, nightly-toolchain, Cargo-cache and upload actions are full-SHA pinned, matching the repository's existing action choices. The exact nightly date and portable Rust/C/C++ settings match established CI settings; no selected profile, assertion or gate is weakened.

## Consumer contract and acceptance limits

`crates/docs/tests/support/reviewed_coverage.rs` supplies the same golden path to the example and test. `po_parser_parity.rs::corpus_reviewed_coverage_matches_frozen_golden` reads that frozen JSON and compares it with the producer-selected authenticated `pipeline/reviewed-translation-observation.json` coverage map. It does not refresh on a miss. The new workflow leaves that authentication, failure and semantic comparison intact. Template catalogs and the two parser fixtures belong to the broader observation, while the golden map intentionally contains live slice coverage only; this unit preserves that distinction.

Workflow source review does not establish a successful hosted refresh, the correctness of any future artifact against final source, or successful authenticated consumer parity. Root's current lint/hooks are separate qualification and were not run or inferred here. Full issue1656, pending1894 source/golden acceptance, translation floors, corpus and installed acceptance remain open. Only this Stage artifact was written; review frozen.
