## Summary

Move complete finite-modal instruction interning keys to native PurRDF JSON under #1776 (Principles 4 and 7). Six private projections cover all ten variants and reuse the full compiler Term writer. Exact tags, fields, numeric addresses, child order and literal metadata are preserved; obsolete Serde derives are removed. The physical plan remains private and has no external reader.

Seventeen literal rows captured before replacement preserve fourteen key shapes, actual lowered keys/addresses and the real root. Existing sharing, proof rollback, canonical operand/world-binder behavior and temporal budget/memo controls remain intact. Synchronization with newer main retains both execution_cbor and instruction_json declarations.

Closes #2061.

## Validation

- PASS: `make clippy-prebuild RUST_PREBUILD_WORKSPACE_ARGS='-p gmeow-logic -p gmeow-pipeline -p gmeow-validate -p gmeow-dev-cli'` — all targets, warnings denied; final integration completed in 1 minute 24 seconds.
- PASS: `make nextest-synthetic` with the same four-library producer-independent graph and the three named controls — 3 passed, 4,496 skipped; final integration build 5 minutes 43 seconds, summary 0.335 second.
- PASS: `make helpers-hygiene`, `make fmt`, `make producer-recipe`, prose/whitespace checks and independent source/reference/integration reviews.
- PASS: configured signed commit/push hooks for original work and base synchronization.
- `make lint`: no separate full invocation; configured touched-file hooks and scoped Rust diagnostics ran.
- `make validate`, `make reason`, `make mappings`, `make wikidata`: not applicable; ontology and mappings are unchanged.
- `make rust-test`: full suite not run under the existing corpus/producer restrictions; the three authored controls above ran on both original and replacement graphs.
- Documentation: public behavior, flags, terms and output format are unchanged; private codec purpose and literal regression references are documented in scope.

## Checklist

- [x] Scope is limited to the complete instruction key operation.
- [x] Existing tests protect the original complete key bytes and actual sharing/budget behavior; no new test function or generic compliance matrix.
- [x] No GMEOW terms changed; term annotation and grounding requirements are not applicable.
- [x] No reference-only or incompatible axioms copied into vocabulary.
- [x] No secrets or local-only paths introduced in the source change or this description.
- [x] CLA Assistant succeeded for this contributor.

## Notes

Parent #1776 remains open. Broader contextual/provenance identities and sibling CBOR work remain separate. No local optimized producer, canonical corpus construction, sync or full gate ran. Authenticated corpus, installed/browser/release and performance acceptance remain unverified; broader CI is explicitly waived.

The hosted source review covered the original head and reported no actionable comments, with comparison-baseline limitations. The final synchronization conflict retained both module declarations and passed fresh independent integration review and the selected checks above. Generic per-method docstring coverage is declined: the private projections implement an already documented trait, and repeating tags and field names would add no semantic explanation beyond the module documentation and exact reference coverage.
