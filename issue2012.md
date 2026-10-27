# Issue #2012: Document the acceptance audit ownership of generated query reads

Source: https://github.com/Blackcat-Informatics/gmeow-ontology/issues/2012

The live generated-read seal reports the acceptance audit's `generated/queries` directory in `scoreboards.rs`. The nearby bundle read already carries an audit annotation, but executable statements separate that comment from the query-directory construction. The seal intentionally requires a trailing marker or contiguous immediately preceding comment.

Place a precise annotation directly at this read after verifying the actual callers. `projection_programs` is reached through `AcceptanceContext::load` by developer acceptance and feedback operations. The products are read from disk, validated against the bundle catalog and expected profile, and prepared for the audit. They do not feed the canonical snapshot DAG. Both CLI operations retain producer admission. Principles 4 and 7.

Acceptance:

- Document the exact acceptance/feedback audit ownership immediately above the query-directory construction using the existing marker contract. Keep required file reads, catalog/profile validation and all callers unchanged. Do not describe the loaded products as fresh in-memory stage outputs.
- Preserve the generated-read seal and its adjacency rule. No path allowlist, parser weakening or new suppression mechanism. No source or test-body change beyond the accurate comment.
- Verify the current source with the existing read-only static checker and normal hooks. Remaining unrelated findings may still fail the whole gate and must be reported accurately. Do not execute acceptance/feedback locally: these admitted operations are outside the standing validation exception.
- No new runtime tests for this comment-only correction. No optimized producer, corpus construction, synchronization or full gate. CI remains waived under the standing instruction.

Remaining contract: #1761, including source-span diagnostic grounding, actual browser attestations and complete gate acceptance.


