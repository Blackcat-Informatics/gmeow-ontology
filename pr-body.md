## Summary

Move the complete generic action-receipt persistence path to native PurRDF JSON: receipt digests, envelope publication, typed readers and both garbage-collection reads. Preserve recursively sorted compact digest input, declaration-order pretty envelope bytes, schemas, identity domains, bounds, locks, atomic publication and integrity checks. Principles 4 and 7.

Wire all nine first-party payload types through the same native path. Reuse the existing slice-verdict and build-identity owners; add complete owners for source exports, corpus artifacts, documentation receipts and pipeline receipts. Replace unit payloads with an owned null-only type. Hydration preserves stored fields without constructor normalization. New readers of named records require their declared fields and reject duplicates, unknown fields and positional records; unsigned schema/count tokens reject signed zero and other noninteger forms.

Remove the old generic receipt Serde implementations and recursive JSON sorter. Selector loading, executable receipts, artifact-content codecs and the separate stage-specific digest retain their existing active owners. No alternate native store API, fallback or cache-version change is introduced.

## Validation

Before replacement, two existing controls captured complete original scalar and structured envelopes and their actual digests. The literal 746-byte and 829-byte references are unchanged; the existing publish/read and canonical-GC controls now compare complete stored bytes and actual receipt digests against them.

All-target warnings-denied checks passed for action-cache, bundle-import, docs-model, docs, pipeline, slicetest, validation and the developer CLI. Independent design and complete source review passed. Formatting, whitespace, prose and complete-source publication scans passed. All 25 existing action-cache library controls passed on final source, with no skipped cases and a 0.115 second summary. Helper hygiene passed after replacing the duplicated envelope writer and fixture hex loop with existing shared owners. The two affected owners passed all-target warnings-denied checks again after that cleanup. Normal signed commit hooks passed.

This verifies the inspected first-party payload contract. Arbitrary external numeric-payload parity is unverified. Full CI is explicitly waived; no local optimized producer, canonical corpus or fixture production, synchronization or full gate ran. Authenticated corpus, installed/browser/release and performance acceptance remain open.

Part of #1776. The parent remains open for its remaining JSON owners, selector/executable paths, dependency removal and full acceptance.
