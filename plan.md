# Shared GMN verdict JSON

Migrate the three-field GmnValidationVerdict record to native ToJson/FromJson,
and route both production callers through that owner: validate-wasm's compact
browser result and pipeline conformance's sorted path-to-verdict map. Preserve
declaration order conformant/detail/failureClass, omit absent optional strings,
and accept missing/null optionals through native Record. Require the boolean,
refuse known duplicates and mistyped fields, and ignore unknown record fields.
No recursive free-form normalization may reorder the nested record fields.

Remove validate-wasm's complete serde_json dependency, using native readers for
existing authenticated observations without changing selection, codebook pins,
assertions or witness bytes. Include the coherent gmn-wasm dev-only JSON reader
removal after its full source/example audit; preserve its source legend checks.
Other lang-bridge Serde/CBOR records and pipeline JSON consumers stay untouched.

Add external pure owner tests with literal bytes and authored values, plus a
pure production-map adapter control. No test constructs a source corpus or
codebook. Do not edit generated assets, attestations, Cargo.lock, or source
capability policy. Root owns formatting, qualification and publication.

Current source confirms only two callers of validate_gmn_document and no other
GmnValidationVerdict embeddings. The two existing package examples use neither
the verdict type nor its codec. No nested AGENTS instructions were found in the
four affected crates. LANG-GMN governs typed codec failures and pinned codebook
identity; this unit changes only JSON presentation. Memory bank gmeow-ontology
recall timed out after ten seconds; current source and prior queue intake apply.
