## Summary

Move the complete correspondence handle commitment to borrowed native CBOR. The root writer includes all seven fields and every nested correspondence, law, literal, formula, term, path, physical source, projection profile, binding and support record. The actual pipeline handle uses this writer while retaining its existing identity framing. Principles 4 and 7.

Preserve serialized declaration order, vector order and multiplicity, optional nulls, unsigned indices and both distinct literal tuple policies. Traversal borrows recursive source trees without constructing a parallel value tree. Persistent correspondence payloads retain their established binary codec. Catalog, relational-core and correspondence writers share one sink and finalizer, preserving each original error context.

## Validation

- Original capture passed the existing binary control before replacing any production encoder or caller. Five complete packets and actual raw, payload and graph commitments were frozen directly from that run.
- Original packet sizes: 123, 4,414, 14,984, 14,984 and 20,624 bytes. Cases cover empty and actual mixed programs, every nested wire family, all enum spellings, raw optional states, Unicode, coordinate literals, maximum indices, order and multiplicity.
- Complete writer and capture source/design review passed independently. Formatting and whitespace checks passed.
- The complete helper hygiene gate and normal signed correction hooks passed.
- Four existing correspondence binary/persistent, relational identity and source-catalog identity controls passed against the shared sink: four cases, 1,213 excluded, 0.057 second execution summary.
- Actual pipeline and developer CLI all-target consumer diagnostics passed with warnings denied in 2 minutes 28 seconds.
- Full CI is waived under the current campaign instruction. No optimized producer, canonical corpus or corpus-fixture construction, sync or full gate was run.

## Checklist

- [x] Complete production operation with original-byte provenance.
- [x] Original binary assertions retained; no new test function.
- [x] Independent source/design review and formatting.
- [x] Normal signed commit hooks.
- [x] Final focused runtime and warnings-denied consumer diagnostics.
- [ ] Full gate and CI: explicitly waived for this campaign.

## Notes

Part of #1777; this does not close its remaining owner, dependency-removal or full acceptance requirements. Complete LogicProgram migration separately needs an authorized substrate float-policy change; this correspondence closure has no raw float fields. No weaker state rejection, local float codec or partial program substitute is introduced.
