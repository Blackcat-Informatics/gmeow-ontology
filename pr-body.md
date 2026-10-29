## Summary

Move the complete native execution receipt writer, bounded borrowed reader, exact byte replay and failure-size accounting to owned native CBOR. Preserve the selected schema, all nested evidence, record order, enum forms, nullable fields, integer-array identities, large unsigned counts and identity framing for the same recorded evidence. Principles 4 and 7.

Fix a confirmed publication boundary defect: the original encoder admitted a directional term its typed decoder refused. A borrowed depth preflight now runs before recursive emission and refuses unreadable output without allocating a second receipt. Successfully emitted packets retain their original bytes.

Keep the directly downcastable logic.result diagnostic and retain native causes beneath it. Eleven existing message-only constructors keep their messages and grades, including four mechanical consumer CLI updates. Shared Serde support remains needed by other current owners. Common record, sequence and unit-depth implementations remove duplication; diagnostic construction shares one definition emitter without changing the declaration census or its policy.

## Validation

- Before replacement, two existing controls passed and captured the complete 26,200-byte original receipt, historical RDF result identity and twelve depth observations; 2,186 cases excluded, 0.243 second summary.
- Independent complete source review passed after correcting the preflight order. All thirteen frozen rows match original output; reviewed record fields, map counts and enum spellings match their source owners.
- Affected logic, pipeline and developer CLI production diagnostics passed with warnings denied. Formatting, whitespace, prose and normal signed commit hooks passed. Final current-source consumer diagnostics passed with warnings denied. All eight selected controls passed: five receipt and three diagnostics cases, 3,397 excluded, 0.184 second summary. Final helper confirmation and signed correction hooks passed.
- Historical parity uses complete decoded original records, retaining their original engine contracts. Fresh producer RDF/closure tests use current source-bound authority; the historical identity is not promoted to a current execution.
- Public consumer CLI compilation is unverified: its authenticated embedded bundle is absent locally and the producer remains prohibited for this campaign. Its four constructor changes were source-reviewed.
- Full CI remains explicitly waived. No local optimized producer, production of the canonical corpus or its fixtures, sync or full gate was run.

## Checklist

- [x] Complete writer, reader, replay and accounting; original references frozen first.
- [x] Independent source review and bounded emission correction.
- [x] Eight selected controls and final affected-consumer diagnostics.
- [x] Final helper confirmation and signed correction hooks.
- [ ] Public consumer CLI build and full CI: unverified under the campaign constraints.

## Notes

Part of #1777; dependency removal, remaining codec owners and full parent acceptance remain open. No dependency or receipt wire-version change. The separate raw-float prerequisite still needs its own upstream authority.
