# Native content hashing

Replace the third-party BLAKE3 owner throughout GMEOW with the already pinned native hash family. Preserve all preimages, framing, ordering, digest bytes, hexadecimal spelling and algorithm-tagged identities. Principles 4, 7, 12, 17 and 18 apply. This is independent of the wider JSON-record migration. Preserve protected main and unfinished sibling work.

Direct calls span production modules, browser engines, development tools and tests. The pinned native streaming/hash API provides digest bytes; its Digest32 and hex module own byte-array identities and hexadecimal rendering. Use those APIs directly, without a compatibility module, another hash implementation, an upstream edit or a new dependency family. Action-cache currently uses SHA-256 for its own content/action contract and has no direct BLAKE3 dependency; preserve that contract and migrate its callers where they use BLAKE3. Audit every standalone lock rather than assuming historical ownership.

## Task 1: Freeze actual product identities

Retain existing diagnostic snapshot goldens and committed vendored-asset manifests unchanged. Add fixed product-key controls for materialized, counterfactual and hypothetical identities, GMN codebook/content identities and the pipeline's typed payload commitments, including a payload beyond the native stream buffer. Capture their expected values through the current production functions before replacing the owner. These are tiny authored inputs and existing static assets, never corpus production or duplicated hash-algorithm conformance.

## Task 2: Replace the owner completely

Migrate all actual Rust call sites, including browser engines, development tooling and tests. Keep native Hash as an ephemeral digest and Digest32 for existing byte-array formatting; use the native hex API at presentation exits. Preserve every update and byte feed. Remove the third-party manifest declarations and resolve all three tracked locks through the existing Make maintenance target, extended to include the standalone validation workspace. Add the removed dependency to the existing re-entry seal; do not add a parallel policy mechanism.

## Task 3: Qualify the changed boundaries

Run formatting, strict producer-independent Clippy, focused synthetic product-identity/snapshot/asset/cache checks and helper hygiene. Frozen results must remain unchanged. Verify every tracked manifest and lock excludes the removed owner. All normal hooks run without bypass. No local corpus/fixture producer, optimized producer, sync, full make check, host probe, resource cap or profile weakening is authorized. Full hosted producer acceptance and all required wasm parity lanes remain mandatory; local synthetic success alone does not close the issue.

## Task 4: Publish, review and integrate

Publish coherent normally signed commits and actual results through Stage. Use external review when the implementation is reviewable; no independent delegation tool is available, so local assessment is self-review. Address all source findings and finish hosted qualification. Merge only with ghprsq, then remove the completed checkout and branch. Keep only this active plan/setup while unfinished; retain no copied issue, log or review archives. Do not change the qualified performance source while its full cold run is pending unless a real finding requires repair.

Every issue requirement maps to these tasks: representation and all consumers to Task 2; byte-identical persisted/attested outputs to Tasks 1 and 3; dependency removal to Tasks 2 and 3; complete check and wasm acceptance to Tasks 3 and 4. No requirement is dropped or represented as already met.

## Current execution

Implementation is committed, pushed and published as PR 1857. Formatting, strict producer-independent Clippy, helper hygiene and 204 focused tests passed, covering all seven frozen product identities, unchanged diagnostic snapshots, six vendored asset inventories, journal and certificate controls, and actual language-form callers. The old dependency is absent from all three tracked locks and guarded against re-entry. Existing duplicate tagged-text and IRI implementations were consolidated; obsolete helper debt was deleted. Full hosted gate and WASM parity are pending; the issue remains open. External review is requested; local assessment is self-review. No local corpus producer, sync or full gate ran.

The initial hosted producer build rejected the new inline payload-identity test. Its implementation is now external, using the existing cfg(test)/path pattern; its full body and frozen values are unchanged. The relocated test, strict producer-independent Clippy, formatting and normal hooks pass. The correction is committed and pushed. Hosted production and WASM acceptance still require successful qualification on this source.

## Pending squash message

This is draft commit prose only. Finalize the required squash-notes file after completed acceptance and refreshed integration review; it is not merge authorization.

Use native BLAKE3 for GMEOW content identities

Replace the removed third-party hash owner across production, browser engines, developer tools and tests using the existing pinned native implementation. Preserve preimages, framing, digest bytes, hexadecimal spelling and algorithm-tagged identities. Action-cache retains its SHA-256 contract. Share the existing tagged-text and IRI digest implementations, remove obsolete duplicate-helper debt, guard dependency re-entry and maintain all three tracked lockfiles through the existing pin target. Principles 4, 7, 12, 17 and 18.

Seven product identities captured through the previous production implementation, unchanged diagnostic snapshots and six vendored asset inventories passed within 204 focused tests. The subsequently externalized payload-identity test, strict producer-independent Clippy and normal hooks passed unchanged. Complete hosted producer/gate acceptance and WASM parity remain pending; no local corpus producer or full gate ran. Final-source review and actual merge-candidate assessment must be refreshed before integration.

Closes #1778. Defect-Class: none.

Current hosted result: run 39257368143 is terminal failure. Optimized producer and Rust prebuild passed. Fixture-prefix and generation A ended with explicit runner shutdown/Killed/cancellation, without an observed source exception or semantic assertion. Generation B is now terminal canceled; its job-log endpoint returned 404, so the cancellation cause is unverified. Whole-run failure does not qualify corpus, consumer or WASM acceptance. Do not retry unchanged production while shared performance qualification remains active.

## Authorized integration

User authorization: "merge them one by one, ignoring CI." Hosted CI is waived for integration, not reported as passing. All PR review threads are resolved. Preserve original identity bytes and the SHA-256 action-cache contract. Combined-tree and WASM execution remain unverified.

The real base synchronization exposed a Cargo.lock dependency-list conflict and a duplicate purrdf-hash manifest entry. Preserve both purrdf-core and purrdf-hash once. Four newly landed external-profile hash callers also require the same native hash/hex migration; their byte inputs and framing are unchanged. Strict producer-independent Clippy passes on the combined source; focused profile/report tests and helper hygiene are running. No producer runs or CI retries were requested.

The combined integration passes all 12 selected profile/report controls, strict producer-independent Clippy, formatting and helper hygiene. The resolved merge commit passed all normal hooks and is pushed. No third-party BLAKE3 call remains in the combined Rust source. User CI waiver applies; no full corpus/WASM acceptance is claimed.
