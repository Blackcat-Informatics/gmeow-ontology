# Issue #1776: deps: replace serde/serde_json with purrdf_lex::json (PurRDF owns JSON)

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, dependencies, rust

## Body

## Problem

PurRDF 3.0 removed serde from every public type and made `purrdf_lex::json` (with `ToJson`/`FromJson` records and `json_record!`) the substrate's one RFC 8259 JSON home. GMEOW still carries its own JSON stack on `serde`/`serde_json`: ~1,370 `serde_json::` uses across ~340 files and ~210 files deriving `Serialize`/`Deserialize`. Under the "where GMEOW and PurRDF conflict in functionality, PurRDF wins" doctrine adopted with the 3.0 upgrade, this is a second JSON implementation.

The 3.0 upgrade crossed the boundary only where PurRDF types meet GMEOW serde (e.g. `docs-model`'s `purrdf_json` serde adapters for `ArtifactRole`/`SliceTier`, YAML instances crossing to `serde_json` through text in `validate::instance`, the research-object `OfflineJsonLdContext`).

## Scope

- Replace `serde_json::Value` and serde derives with `purrdf_lex::json::Value` and `ToJson`/`FromJson` (`json_record!`) crate by crate, in dependency order.
- Retire the boundary shims added for 3.0 (`docs-model` `purrdf_json`, JSON-text crossings in `validate::instance` and the YAML tests).
- Remove `serde`/`serde_json` from the workspace graph when the last use goes.

## Acceptance

- No `serde`/`serde_json` in any GMEOW manifest; `cargo tree -i serde_json` is empty for workspace members.
- Every committed JSON artifact and golden is byte-identical, or each difference is traced to a documented `purrdf_lex::json` rule (lexeme-preserving numbers, member order).
- `make check` passes.


## Comments (7)

### paudley — 2026-10-21T06:19:38Z

# Bounded delivery under #1776

Principles4 and7 govern the single substrate JSON owner, exact evidence bytes and hard-failing admission. This unit uses the existing optimized queue and focused review/qualification; no Stage review panel or repeated portfolio intake applies.

Migrate only gmeow-perf-evidence's sample, acceptance and JUnit JSON boundary to the pinned purrdf_lex::json owner. Preserve sorted, last-wins free-form objects through record::read_document and sorted_last_wins; use Object and ToJson for construction. Preserve atomic publication, synchronization, terminal newline and temporary-file cleanup, including non-UTF-8 path refusals. Retain finite/nonnegative observation checks and strict integer selectors.

Document and test the owner's intentional numeric differences: parsed number lexemes survive publication, paired identities use exact decimal equality, and huge valid number lexemes must still fail finite observation admission. Add external, pure tests with literal legacy byte expectations and existing cache/proof/refusal controls. No corpus, model execution or host-probing tests.

Scope: crates/perf-evidence manifest/source/external tests and docs/rust-test-performance.md. Remove only this crate's serde_json dependency; root owns lock resolution. Full #1776 remains open.

Source-only implementation: root owns formatting, builds, focused Make qualification and all delivery operations. No commits or posts from this lane. Qualification proposed: make nextest-synthetic NEXTEST_FILTER='package(gmeow-perf-evidence)', then applicable root-owned gates.


### paudley — 2026-10-21T06:52:21Z

# Bounded transcode JSON migration

Principles 4 and 7 govern the single JSON substrate, deterministic report contracts and hard-failing static owner invariants. Refs #1776; the completed perf-evidence unit is PR1904 and the full parent remains open.

Replace serde/serde_json only in gmeow-transcode with the pinned purrdf_lex::json record/value/writer owners. Preserve RealizedLoss and MatrixEntry declaration-order members, loss-ledger row ownership, codec filtering, and the existing newline distinction (realized loss has none; matrix outputs have one). Free-form substrate matrix parsing keeps sorted last-wins objects. Infallible owner serialization removes silent empty-array failure fallbacks.

Scope: crates/transcode/Cargo.toml, src/lib.rs, src/lib.tests.rs. Add pure literal byte, ledger merge/order, filtering and refusal controls. Keep the generated-artifact drift test intact for separate producer-qualified acceptance. Preserve public String-returning callers; no downstream migration or generic JSON shim. Root owns formatting, qualification, lock resolution and delivery. Full JSON campaign remains open.


### paudley — 2026-10-21T08:22:17Z

# Affect capture JSON ownership

Principles 4 and 7 govern the single JSON owner and fail-closed capture admission. Refs #1776; previous package units PR1904 and PR1906 are landed, while full workspace migration and production acceptance remain open.

Migrate the five affect-ingest capture types to the pinned purrdf_lex::json enum/record owners, removing this leaf's serde and serde_json dependencies. Migrate actual CLI ingest/recover and validate-conformance decode callers directly to the same owners. Preserve known duplicate-field refusals, ignored unknown fields, enum/threshold tags, optional missing/null acceptance and absent-field omission, declaration-order records, sorted maps and finite f64 decoding. Typed documents must not pass through free-form sorted-last-wins normalization.

Use FromJson/ToJson and Record directly for the selected wire contracts; do not add an adapter or generic JSON engine. Add independent tiny JSON and RDF fixtures for literal bytes, admission/refusal and decode-produce-recover-encode controls; retain existing captured-fixture and semantic refusal coverage in external test ownership. No source corpus compilation, model activity or host measurement.

Root owns formatting, lock resolution, compilation, qualification and delivery. Source-only implementation in the exact assigned checkout. Full JSON migration campaign remains open.

Root confirmed the native owner admission boundary: object records and string enums are required; incidental serde positional arrays and singleton-map unit enums are intentionally refused. The user-facing capture fixture README documents the migration and tests pin actionable errors. No compatibility decoder is introduced.

Root formatting and all37 pure package tests passed. The separate source-ownership repair must qualify native JSON macro capability before publication. Current helper hygiene and remaining qualification are recorded in validation.md; no actual model or local corpus/full-gate execution is claimed.


### paudley — 2026-10-21T13:49:17Z

PR #1920 delivers the complete native perf-gate envelope and Criterion timing/baseline report families under Principles4 and7. The merger retains complete command documents and profile-specific work/observation data. The actual comparison/CLI and committed leaderboard use native admission; invalid required inputs fail, valid regressions remain advisory, and zero-reference percentages are unavailable.

All27 selected authored-input controls passed, along with strict producer-independent Clippy, helper hygiene, source admission, formatting and normal commit/push hooks. Two independent review scopes are clear. Wire docs distinguish free-form last-wins/exact-number behavior from strict unsigned typed baseline records.

The retained campaign exception still applies: no local corpus/fixture production, sync or full gate; sequential merge without waiting for CI. Actual measurements, authenticated/installed/full consumer acceptance and remaining JSON owners are open. This does not close #1776. Next ready unit is the private cost/soak reader family, whose producer already uses native JSON.


### paudley — 2026-10-21T14:19:20Z

PR #1921 delivers the complete native cost-ledger and soak reader family under Principles4 and7. Both actual renderers share thirteen typed record decoders and admission. Missing selected measurements fail; absent proof height stays unavailable. Source-proven schema/count/identity/scope and nonvacuity checks prevent a HELD claim over missing evidence without equating comparison counts to benchmark-case counts.

Fifteen new authored-input controls and two unchanged committed-JSON metadata regressions passed, along with strict producer-independent Clippy, helpers, source admission, formatting and normal commit/push hooks. Independent review is clear after its concrete inherited admission finding was repaired. Report bytes/arithmetic/order, existing ruleparallel validation, gap refusal and exact soak digest framing are retained.

The retained exception applies: no local corpus/fixture production, sync or full gate; sequential merge without waiting for CI. Actual benchmark execution, committed-stage byte parity, authenticated/installed/full consumer acceptance and remaining JSON owners stay open. This does not close #1776. Next ready unit is the complete site-cache envelope and its existing shared digest byte boundary.


### paudley — 2026-10-21T14:48:45Z

PR #1923 delivers the complete native rendered-site/book cache JSON envelope under Principles4 and7. Both reads and the producer write use the native owner. The existing shared digest boundary now receives canonical bytes, preserving model Serde bytes and native sorted site-map bytes, SHA1 framing, the site label and all selector/receipt/tamper checks.

Fifteen authored/in-memory controls passed, including an independent digest golden and four existing regressions. Strict prebuild Clippy, helper hygiene, source admission, formatting and normal commit/push hooks passed; independent review is clear. Helper hygiene caught a duplicate byte wrapper; direct native record::to_vec replaced it without changing expectations. Docs drops production serde_json while retaining dev and other honest separate Serde owners.

The retained campaign exception applies: no local corpus/fixture production, sync or full gate; sequential merge without waiting for CI. Actual selected rendered-cache/authenticated/installed/full consumer acceptance and full #1776 remain open. The hosted schema.org registry failure is separately owned under #1894; its valid quantity mapping and existing ceiling will be preserved while correcting the family/guard scope.


### paudley — 2026-10-21T15:22:10Z

The i18n lint diagnostic identity writer is migrated in PR #1924. It now uses native JSON for the compact `[rule,[subjects...]]` tuple, preserving string escapes, subject order/empty inputs and the surrounding diagnostic context. Four focused controls passed, along with strict producer-independent Clippy, helpers, source admission, formatting and normal hooks; independent review found no actionable issue.

This completes one source family. Repository i18n lint, produced catalog parity, installed/full-consumer validation and the rest of this issue remain open under the recorded campaign exception. Other docs-model Serde owners are unchanged. Principles 4 and 7.


