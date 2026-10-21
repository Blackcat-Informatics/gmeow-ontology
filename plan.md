# Affect capture JSON ownership

Principles 4 and 7 govern the single JSON owner and fail-closed capture admission. Refs #1776; previous package units PR1904 and PR1906 are landed, while full workspace migration and production acceptance remain open.

Migrate the five affect-ingest capture types to the pinned purrdf_lex::json enum/record owners, removing this leaf's serde and serde_json dependencies. Migrate actual CLI ingest/recover and validate-conformance decode callers directly to the same owners. Preserve known duplicate-field refusals, ignored unknown fields, enum/threshold tags, optional missing/null acceptance and absent-field omission, declaration-order records, sorted maps and finite f64 decoding. Typed documents must not pass through free-form sorted-last-wins normalization.

Use FromJson/ToJson and Record directly for the selected wire contracts; do not add an adapter or generic JSON engine. Add independent tiny JSON and RDF fixtures for literal bytes, admission/refusal and decode-produce-recover-encode controls; retain existing captured-fixture and semantic refusal coverage in external test ownership. No source corpus compilation, model activity or host measurement.

Root owns formatting, lock resolution, compilation, qualification and delivery. Source-only implementation in the exact assigned checkout. Full JSON migration campaign remains open.

Root confirmed the native owner admission boundary: object records and string enums are required; incidental serde positional arrays and singleton-map unit enums are intentionally refused. The user-facing capture fixture README documents the migration and tests pin actionable errors. No compatibility decoder is introduced.

Root formatting and all37 pure package tests passed. The separate source-ownership repair must qualify native JSON macro capability before publication. Current helper hygiene and remaining qualification are recorded in validation.md; no actual model or local corpus/full-gate execution is claimed.
