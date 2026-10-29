## Summary

Source-catalog identities now stream the complete borrowed native CBOR tuple through the existing BLAKE3 sink. The source-document owner writes every path, digest, role, optional base, origin and directive coordinate; the actual catalog constructor uses the shared tuple writer. Schema tag, field order, receipt order and multiplicity remain unchanged. Principles 4 and 7.

## Validation

- [ ] `make lint` — not invoked locally.
- [ ] `make validate` — not run under the standing producer/full-gate exception.
- [ ] `make reason` — not applicable; ontology sources are unchanged.
- [ ] `make mappings` / `make wikidata` — not applicable; mappings are unchanged.
- [ ] `make rust-test` — full suite not run under the standing exception.
- [x] Native writer API documentation updated.

Original capture passed: ten complete packets and original production identities, including the actual synthetic catalog identity. All original production encoders, callers and lockfile were unchanged during capture. Independent combined review, formatting, prose and whitespace checks passed. Scoped warnings-denied compilation passed for all pipeline and developer CLI targets. Normal signed hooks passed. Final native runtime passed: one existing case, all ten full original packets and actual identities equal, 1,216 other cases skipped. Hosted helper hygiene passed on this revision.

## Checklist

- [x] Scope is intentional and limited to this complete identity operation.
- [x] Existing identity/compilation control extended; sharing, exact-selection and retention assertions preserved.
- [ ] New/changed GMEOW term annotations — not applicable; no terms changed.
- [x] No reference-only axioms copied into the vocabulary.
- [x] No secrets or machine-specific paths added to production code.
- [x] CLA Assistant check passed (`license/cla`).

## Notes

Part of #1777. Other CBOR owners, dependency retirement and full parent acceptance remain open. Full CI is explicitly waived; no local optimized producer, canonical corpus/fixture construction, sync or full gate ran. No new test function, dependency, JSON bridge or persistent catalog decoder was added.
