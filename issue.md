# Issue #2026: Use native JSON for complete source-selection and receipt identities

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

Use native PurRDF JSON for the complete source-selection identity preimage and source-document receipt encoding consumed by source_digest. These are bounded source-admission identity operations under parents 1776 and 1777. Principles 4 and 7.

Scope and acceptance:

- Implement the full native writer closure for SourceSelection, SourceGraphSelection, DocumentSelection and RootSelection. Route SourceSelection::digest through native compact encoding while preserving its complete bytes and gmeow-source-selection-v1 domain.
- Reuse the existing complete SourceDocument, SourceBase and SourceBaseOrigin writers for each receipt encoded in source_digest. Preserve sorted encoded-byte order, big-endian receipt lengths and every existing graph, quad, reifier and annotation framing byte in gmeow-prepared-source-v3.
- Capture original compact selection preimages, actual selection digests, complete receipt preimages and actual whole-source digests before production edits. Extend the existing owner root/document controls and retain the actual persisted admission roundtrip control. Cover anonymous and exact document selection, every root-selection arm, empty/nonempty exact sets, raw Unicode/NUL and nonempty source-base receipts using independently authored data. Compare complete frozen preimages and digests literally; no replacement-derived expected values, normalization or hash-version change.
- Retain all shared Serde owners, public digest APIs, admission decisions, authority checks and persisted publication behavior. The separate streaming evidence digest keeps its current streaming contract. Full native admission publication/hydration remains with the parent migration contract.
- Verify complete field ownership and actual callers, independent combined review, strict focused compiler/runtime/pipeline/developer-consumer compilation as affected, the three existing authored controls, helper hygiene, static producer recipe and normal signed hooks. Record exact tested scope.

Standing limits remain: no local optimized producer, canonical corpus or fixture construction, sync or full gate. Tiny independently authored controls are permitted; tests never construct repository corpus. No fabricated shipped asset or authenticated receipt. CI is explicitly waived rather than passed. Parent complete migration, installed consumer, authenticated corpus, browser, production and release acceptance remain open.


## Comments (0)

