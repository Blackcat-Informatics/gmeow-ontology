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


## Comments (21)

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


### paudley — 2026-10-21T16:49:22Z

PR #1926 migrates the actual selected quality-profile identity writer to the pinned native JSON codec. Declaration/status/reason order, sorted axes, explicit empty fields, string escaping, no trailing newline and existing BLAKE3 identity are preserved. The separate diagnostic metadata exit still requires Serde; this does not complete the dependency migration.

Eight selected tiny-input tests passed, including three new exact-byte/independent-digest controls and five existing profile/refusal/report controls. Strict producer-independent Clippy, helper hygiene, source-input validation, formatting and normal commit/push hooks passed. Initial compilation caught a missing direct codec dependency; the existing workspace dependency and lock edge were added and the complete selected checks rerun successfully. Six source files are committed.

The same unit corrects two stale cost/soak comments identified after an earlier merge; generated artifacts remain ignored and producer-owned. Under the recorded campaign exception, actual corpus/installed/full consumer acceptance remains unrun and the parent stays open. Principles 4 and 7.


### paudley — 2026-10-21T17:42:14Z

PR #1928 replaces the documentation shell context and its private navigation wrapper with native Minijinja values at the actual renderer. It preserves the nine context keys, ordered nested links, language fallback and ordinary string safety. Principles4 and7.

Four tiny authored-model controls passed, including the existing translation/fallback control; strict prebuild, helpers, source-input recipe, formatting and normal signed commit/push hooks passed. Independent source review is clear. The template and dependencies are unchanged. Full-site/installed/authenticated corpus acceptance and the remaining Serde owners are still open; this does not close #1776. Delivery follows the recorded no-local-corpus/full-gate and sequential-merge CI exception.


### paudley — 2026-10-21T19:14:40Z

PR #1931 moves the actual reviewed-coverage maintainer example to the docs-model package it uses, shares canonical paths with the frozen-golden reader, and replaces its Serde writer with the existing native compact writer (Principles4and7). The same Make target and output path remain; no catalog/coverage/profile/flag/cache changes.

Strict producer-independent all-target Clippy, helpers hygiene, source admission, repository lint, formatting, independent source review and normal signed commit/push hooks passed. [Hosted native Make generation](https://github.com/Blackcat-Informatics/gmeow-ontology/actions/runs/39657987753) passed and its entire output matches the unchanged golden byte for byte:144catalogs,27763contexts. No golden edit or local producer was needed. No controlled speedup claim; this removes the renderer package from the maintainer command's build ownership.

The remaining annotation/translation quality, metrics/floors, authenticated corpus/installed acceptance and broader native JSON migration remain open in #1656 and #1776. This campaign retains the exception against local corpus/sync/full-gate runs and permits sequential merges ignoring CI; normal hooks remain mandatory.


### paudley — 2026-10-21T19:54:18Z

PR #1932 landed the complete native agreement-tally record family: actual producer, dashboard reader and authenticated typed-reader compilation now share the native codec. Sorted keys, declared field order, pretty bytes, trailing newline, omitted empty maps, unsigned counts and missing/null behavior are covered. Typed records now deliberately require objects. Rate arithmetic uses wider intermediates, and all displayed aggregates refuse overflow with actionable identity.

All 17 focused authored-data controls passed, along with strict prebuild, helper ownership, source admission, formatting, lint and normal signed commit/push hooks. Independent source review is clear; merge candidate and signed Stage archive were verified and the completed worktree was cleaned.

Principles 4 and 7. This parent stays open: authenticated corpus, installed consumers, remaining Serde families and full qualification are still outstanding. The recorded campaign exception applies to local corpus/full-gate runs and waiting for CI.


### paudley — 2026-10-21T21:33:38Z

[PR #1933](https://github.com/Blackcat-Informatics/gmeow-ontology/pull/1933) is merged: the complete five-record medium winner-table writer/reader family now uses native JSON. It preserves declared pretty bytes and newline, vector order, fraction strings, strict integer/schema admission and ignored unknown members. Typed objects explicitly reject positional arrays. Winning target conversion now refuses an unrepresentable size instead of saturating.

All ten authored controls passed, along with strict prebuild, helpers, source admission, lint, independent source review and normal signing/hooks. The actual merge matched its clean candidate; the signed Stage archive was read back before owned worktree/branch cleanup and main fast-forward. Full dependency migration, authenticated winner-table/corpus/measurements, installed consumers and narrower-platform execution remain open. Principles 4 and 7.

The next coherent unit is the closed diagnostic-node transport through actual stage-cache and public bundle readers. It is source-only work in a fresh isolated checkout. Full Report/Finding metadata remains separate because numeric serialization participates in authenticated content digests; this is not a full parent completion claim.


### paudley — 2026-10-21T22:16:29Z

PR #1935 is merged. The complete lowered diagnostic-node family now uses native records through five stage writers, persistent-cache tuple replay and the distinct public bundle vector reader, preserving fingerprints and producer identity. Twenty-nine distinct authored/in-memory/cache controls, strict producer-independent compilation, helper/source/lint checks, independent reviews and normal hooks passed. Signed Stage archive verified; completed checkout cleaned. Principles 4 and 7.

The next independently complete unit is Finding/RelatedLabel transport through the authenticated validation cache and all four CLI NDJSON events. Report/SARIF remains separate: arbitrary metadata numeric canonicalization participates in attested record digests, and the pinned lossless native reader does not supply the old complete admission/canonicalization policy. No local compatibility decoder, Serde bridge or digest drift will be used. The full migration and authenticated/installed/excluded embedded acceptance remain open; #1776 is not closed by these units.


### paudley — 2026-10-21T22:55:39Z

The complete native Finding/cache/NDJSON operation has landed in [PR #1936](https://github.com/Blackcat-Informatics/gmeow-ontology/pull/1936). Actual authenticated validation cache replay and all four shared CLI NDJSON events now use the native owner, including full-width elapsed values, complete finding payloads and actionable sink failures. CLI core removes its direct Serde dependencies.

Nineteen focused controls, strict all-target prebuild, helper/source checks, lint and normal signed commit/push hooks passed. Integration and signed evidence readback verified; completed checkout cleaned. The recorded no-local-corpus/full-gate and merge-ignoring-CI exception remains explicit. Full Report/SARIF/arbitrary metadata/digests, other migration owners and produced/installed/excluded embedded acceptance remain open; this does not close the parent.


### paudley — 2026-10-21T23:56:40Z

The source-span transport delivery is merged in [PR #1938](https://github.com/Blackcat-Informatics/gmeow-ontology/pull/1938). Both actual production ends now use native JSON records while retaining source coordinates, path-table ordering and sharing, strict integer widths and selected-product commitments. An overflow diagnostic found during qualification now includes the index in its primary message and retains its original conversion cause.

All14focused authored controls, strict producer-independent prebuild, helpers, source admission, lint and normal signing/commit/push hooks passed. The landed tree and signed evidence archive were verified, and the completed worktree was cleaned up. The recorded exception excludes local optimized producer/corpus/fixture/sync/fullgate; hosted checks do not block sequential delivery. Full Report metadata/digests, arbitrary profiles, foundational IRI writers, other Serde owners and complete corpus/embedded/installed acceptance remain open.

The next bounded unit is the catalog-family observation transport through its actual mappings writer and authenticated selected reader.


### paudley — 2026-10-22T00:09:06Z

The catalog-family observation transport is merged in [PR #1939](https://github.com/Blackcat-Informatics/gmeow-ontology/pull/1939). All four records, the actual mappings publisher and authenticated selected reader now use native JSON, preserving compact bytes, vector/set semantics and strict counts. Authentication still precedes decoding; refusals retain native pointers and typed causes. Existing registry admission and floor/exemption rules remain intact.

All9focused authored controls, strict producer-independent prebuild, helpers, source admission, lint, format and normal signing/commit/push hooks passed. The landed tree and signed evidence archive were verified, main advanced and the completed worktree was cleaned. Current hosted checks remain separate under the recorded CI exception; no local optimized producer/corpus/fixture/sync/fullgate was run.

The next bounded unit is projection-profile inventory transport through its actual writer and authenticated consumer. Full remaining dependency removal and authenticated shipped/corpus/embedded/installed acceptance stay open.


### paudley — 2026-10-22T00:50:04Z

Delivered the complete native projection-profile inventory transport in [PR #1941](https://github.com/Blackcat-Informatics/gmeow-ontology/pull/1941), including its actual mappings publisher and authenticated selected reader. Seven authored controls, strict producer-independent compilation, hygiene and normal hooks passed. Inventory checks and artifact authentication remain intact.

The next grouped unit moves norm-claim and source-contract observations, sharing the recorded-diagnostic codec and covering both norm publishers. Its source is independently reviewed and focused qualification is running. Generic execution-cache leaves require their own complete transport migration; this update does not close them or the remaining dependency-removal contract.

The recorded campaign exception still excludes local optimized producer/corpus/fixture/sync/fullgate and allows sequential merges ignoring CI. Produced/authenticated corpus, excluded embedded consumers and installed acceptance remain unverified.


### paudley — 2026-10-22T02:14:24Z

The [complete norm/source-contract observation transport](https://github.com/Blackcat-Informatics/gmeow-ontology/pull/1944) is merged. Both norm publishers and the authenticated norm/source-contract readers now use native JSON, preserving causal diagnostic identities, dispatch variants, strict count admission and supported graph hydration. Catalog and projection count readers share the same native primitive adapter.

All thirty focused authored controls passed, along with helper checks, strict producer-independent compilation, source admission, lint and normal hooks. The landed tree and complete signed evidence archive were verified; the completed checkout was cleaned up. Actual authenticated corpus, embedded consumers, installed acceptance and full dependency removal remain open under the recorded campaign exception.

The next complete unit groups contextual and flagship observations with claim-audit transport, public JSON and query-binding identity. It will preserve exact compact/pretty output profiles, schema admission and diagnostic replay rather than leaving a partial caller migration.


### paudley — 2026-10-22T02:35:12Z

The [complete terminal observation family](https://github.com/Blackcat-Informatics/gmeow-ontology/pull/1945) is merged. Contextual, flagship and claim-audit publishers/readers now use native JSON, including the public claim view and exact query diagnostic identity. Authentication, schema checks, observation computation and causal replay are preserved.

All twenty-two focused authored controls passed, along with helper checks, strict producer-independent compilation, source admission, lint and normal hooks. The landed tree and complete signed evidence archive were verified; the completed checkout was cleaned up. Actual worked/contextual/flagship corpus, excluded embedded consumers, installed acceptance and full dependency removal remain open.

The next grouped operation covers bundle-import receipt digest/publication/replay/GC and slice-spec verdict products. It also repairs an unchecked census sum in the actual replay validator, with explicit overflow refusal. Tests will use inert authored receipts and entries; no local corpus producer or worker will run under the campaign exception.


### paudley — 2026-10-22T03:12:49Z

The [native import-receipt and slice-spec verdict operation](https://github.com/Blackcat-Informatics/gmeow-ontology/pull/1946) is merged. All five standalone products and their publication/replay/GC sites use native JSON, and both crates’ unused direct serde_json dependencies are removed. Genuine enclosing selector and ActionReceipt Serde contracts remain intact.

Receipt digest bytes, field policy and authentication/GC checks are preserved. The actual aggregate replay validator now refuses either census-addition overflow actionably. All seventy-seven authored controls, helper checks, strict producer-independent compilation, source admission, lint and normal hooks passed. The landed tree and complete signed evidence archive were verified, and the completed checkout was cleaned up.

Actual authenticated corpus/imports, repository workers, embedded/installed acceptance and full dependency removal remain open under the campaign exception. The next complete operation covers advice-wing observations with explicit nonfinite confidence refusal and the independent policy-statement snapshot’s native publisher/reader.


### paudley — 2026-10-22T03:41:09Z

The advice observation and independent policy-statement artifact now use native JSON at their real publishers and authenticated readers in [PR #1947](https://github.com/Blackcat-Informatics/gmeow-ontology/pull/1947). Nonfinite public confidence values fail with advisory index/code/field context before bytes return; the live producer retains its exact 1.0 output. RDF projection, range authority, prepared policy and authentication remain with their existing owners.

Eleven focused authored/literal/factory controls passed, along with strict producer-independent caller compilation, full helpers, source admission, lint, formatting and normal signed commit/push hooks. The landing matched the reviewed candidate and the complete evidence archive was verified before checkout cleanup.

The next complete unit is source-medium registry transport: its twelve nested types, real compact publisher and authenticated reader. Generic cache/selector transport, actual authenticated corpus and advice/MCP acceptance, excluded embedded consumers, installed behavior and the full dependency migration remain open. Current CI and bot review were pending at capture under the recorded campaign exception. Principles 4 and 7.


