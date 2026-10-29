## Summary

Replace the complete diagnostics publication cache and commitment path with owned native CBOR. Preserve all report fields, sparse declaration order, enum spellings, metadata number kinds and negative zero, while retaining producer ownership, graph pins, payload authentication and byte accounting. Principles 4 and 7.

The native reader deliberately rejects unknown or duplicate fields, duplicate metadata keys, unsupported metadata kinds, invalid typed values and inputs beyond the existing 256-container budget. The pipeline Decode kind remains directly downcastable and retains the native cause beneath it. Existing message-only constructors are mechanically updated without changing their messages or grades.

Remove the obsolete aggregate Serde reader and route all selected cache and test callers through the native implementation. Shared Report Serde support remains required by other current owners.

## Validation

- Before replacement, the existing round-trip/accounting control captured five complete original packets with actual raw, payload, graph-pin and commitment identities: 2,286, 4,617, 2,341, 43 and 6,996 bytes.
- Independent complete source/design, error-contract, reference-provenance and selected-refusal reviews passed. Formatting, prose, whitespace and normal signed commit hooks passed.
- Final native round-trip/accounting, evidence-tampering and refusal/binding controls passed: three existing cases, 1,214 excluded, 0.134 second summary. All five original packet/identity rows and the depth/type/cause refusals passed. Hosted exact-head lint and helpers passed. Final pipeline/developer CLI all-target consumer diagnostics with warnings denied and local helper hygiene passed.
- Full CI is waived under the current campaign instruction. No optimized producer, canonical corpus or corpus-fixture construction, sync or full gate was run.

## Checklist

- [x] Complete production writer, reader and commitment, with frozen original bytes.
- [x] Existing accounting, tampering and binding assertions preserved; no new test function.
- [x] Independent source and error-boundary review.
- [x] Normal signed commit hooks.
- [x] Final focused native runtime and hosted lint/helpers.
- [x] Final affected-consumer diagnostics and local helpers.
- [ ] Full gate and CI: explicitly waived for this campaign.

## Notes

Part of #1777; remaining codec owners, dependency removal and full parent acceptance remain open. The separate Report v2 schema and LogicProgram float-policy prerequisite remain outside this unit. No dependencies or cache wire versions change.
