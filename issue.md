# Issue #1981: Use native JSON for complete internal GTS snapshot receipt

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, cross-cutting, rust

## Body

Migrate the complete internal GTS snapshot receipt operation to PurRDF native JSON. The private PassOneReceipt has five fields: schema_version, algorithm, input_digest, snapshot_content_digest and stratum_digest. Its sole writer follows terminal emission; reusable_pass_one_receipt is its sole typed reader. The sink carries the exact bytes at pipeline/gts-pass-one-receipt.json as internal evidence. This artifact is not shipped inside the bundle or reconciled into generated files.

Replace the record's obsolete Serde derives and both actual JSON boundaries. Preserve compact wire bytes and field order, every required field, unknown-field tolerance and strict u32 schema admission. Fixed records refuse duplicate known members and positional arrays. Reuse the native writer macro, Record reader and strict unsigned helper; no generic JSON engine, source parser or compatibility layer.

Keep all existing reuse rules intact. Absence of the terminal is the initial miss. A present terminal with a missing or malformed receipt fails. Stale input, wrong schema or algorithm, and noncanonical content or stratum digests fail actionably. Never recompute silently after receipt refusal. Canonicalization, digest framing, receipt version, GTS emission, compression and source/action authentication remain with their current owners.

Acceptance:

- Both production boundaries use the intended native record and the sink retains the exact internal artifact.
- Extend the existing authored present_pass_one_receipt_is_typed_and_fail_closed control to pin exact native wire and exercise field, numeric and existing reuse refusals. The control consumes authored digest strings and byte artifacts only; it never constructs GTS or corpus.
- Actual pipeline and developer CLI consumers compile under strict diagnostics. Source admission, helper ownership, independent review, formatting and normal signed hooks pass.

The standing exception excludes local optimized production, corpus construction, sync and the full local gate. Actual terminal emission, authenticated corpus, installed/browser and cold parity acceptance remain unverified. Remaining contracts: #1776, #1962 and #1757. Principles 4, 7, 12 and 18 apply.


## Comments (0)

