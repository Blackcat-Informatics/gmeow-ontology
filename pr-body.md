## Summary

Native storage failures while encoding math result identities retain the profile’s `logic.result` code, grade, profile-prefixed message and failure-class policy. The native cause is attached once to a fresh diagnostic, retaining the encoding context. Identity framing and success bytes are unchanged. Principles 4 and 7.

## Validation

- [ ] `make lint` — not invoked locally; hosted lint passed.
- [ ] `make validate` — not run under the standing producer/full-gate exception.
- [ ] `make reason` — not applicable to this source-only change.
- [ ] `make mappings` / `make wikidata` — not applicable; mappings are unchanged.
- [ ] `make rust-test` — full suite not run under the standing exception.
- [x] Applicable Rust API/error-contract documentation updated.

Passed: affected pipeline and developer CLI targets with warnings denied, independent combined source review, formatting, prose, whitespace and normal signed hooks. Hosted helper hygiene passed on this PR revision. Existing complete-byte success-path qualification remains applicable because the encoding path is unchanged. No new test function or artificial failure-injection seam was added; no runtime storage-failure injection was performed.

## Checklist

- [x] Scope is intentional and limited to this change.
- [ ] Tests updated when behaviour changed — no tests changed; existing success-path qualification is reused, and the diagnostic correction was checked by source review and consumer compilation.
- [ ] New/changed GMEOW term annotations — not applicable; no terms changed.
- [x] No reference-only axioms copied into the vocabulary.
- [x] No secrets or machine-specific paths added to production code.
- [ ] CLA Assistant check — no result was reported in the PR check inventory.

The preserved donor was reconciled with main. Stronger financial guards and complete math identity controls remain intact.

## Notes

Part of #1776. Full parent acceptance remains open. Focused compilation is complete. Full CI is explicitly waived; no local optimized producer, canonical corpus/fixture construction, sync or full gate ran. Hosted review supplied a spending-cap notice and no source findings; no hosted review pass is claimed.
