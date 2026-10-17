# Native anyURI cutover

This is the next coherent ownership unit of issue 1803, following merged PR 1837. The larger issue remains open: native DL delegation, chase ownership and other datatype families are not completed by this unit. Principles 4 and 17 require one mechanism owner and preservation of canonical reasoning semantics.

## Task 1: Qualify the native replacement

PurRDF issue 572 implements anyURI as a native primitive. Its authoritative plan and execution evidence are in the native issue worktree. Require ordinary/admitted parsing, XML whitespace collapse, character validity, equality and primitive disjointness, length ranges, conservative capacity, entailment consumer checks and actual WASM parity. No corpus production, machine probing, dependency fallback or duplicate normalizer.

## Task 2: Delete superseded GMEOW paths

Pin both manifests and lockfiles to the qualified native implementation. Delete special named membership, opaque equality and length normalization, and the hand-maintained infinite-capacity entry. Preserve the original lexical input before the native parser so Unicode whitespace outside XML whitespace is not stripped. Use the ordinary native interpretation and range paths. Update enumeration expectations from unknown to proved false for distinct admitted URI values; foreign and malformed datatypes remain unknown.

Acceptance runs through NativeValues and DatatypeCache production consumers using synthetic tests, including native literal validity, whitespace aliases, Unicode boundary whitespace, primitive disjointness, enumeration, length facets and capacity. Tests must consume no real corpus. Use make nextest-synthetic with an explicit filter and the normal workspace graph. The full corpus producer remains hosted only.

The cutover exposed a provenance gap in the prior substrate identity: datatype implementation changes do not necessarily move its canonical rule hashes. Bind the immutable upstream Git URL and full revision from the declared workspace dependency as well. Parse this source-only input structurally and fail closed on a missing/mutable pin; no compiler, hardware or process discovery is allowed. Fold it identically on native/WASM while leaving operation limits in their operation receipts. Verify distinct pins yield distinct source identities without changing a rule program. Refresh the two tiny parity attestations through the explicit maintainer producer, then run their native and Node/WASM consumers read-only. Remove stale references to the retired conjecture-only refresher.

## Task 3: Publish and integrate

Record actual validation, review feedback, hooks and hosted CI separately. Merge using ghprsq only, applying existing user authorization for pending CI. Independent local review tooling is unavailable; record this honestly and use attributable CodeRabbit feedback instead of inventing review reports. Do not close 1803 as a consequence of this incremental PR. Preserve sibling branches and historical authenticated evidence.
