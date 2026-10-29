# Issue #2050: Preserve axis-evidence identities with native JSON

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, dependencies, rust

## Body

Replace the remaining AxisEvidence::content_key Serde writer with the existing complete native PurRDF JSON owner. This is a bounded identity operation under #1776, governed by Principles 4 and 7.

Acceptance:

- Route the actual content-key writer through native compact String output of the existing AxisEvidence ToJson implementation. Preserve all three fields, declaration order, explicit nulls and raw source vector order/multiplicity. Do not call the validating/canonicalizing constructor during writing, introduce another codec or change the infallible String API.
- Preserve default-evidence omission and UTF-8 byte-length framing in Correspondence::content_key, complete correspondence/program/LogicProgram identities, and preservation/catalog identity consumers. Keep their algorithms and shared Serde derives unchanged.
- Before replacement, extend only the existing loss_identity_binds_all_literal_components_and_owner_context control to capture complete original axis JSON, full composed correspondence keys and actual preservation targets for default, sources-only and populated evidence, including Unicode and raw duplicate/order distinctions. Retain all original assertions. Freeze literal original references, then replay them through the native writer and actual callers; no replacement-derived expectations or new generic test matrix.
- Qualify affected compiler and downstream logic/pipeline/validation/developer CLI consumers with warnings denied, the affected existing identity case and existing actual pipeline cache controls, independent source review, formatting, helpers, static recipe and normal signed hooks. Original and final captures use the same selected consumer graph.
- Charge readiness from 13:35 UTC October 29 and land within the original eight-hour limit, including qualification and publication.

No source edit or build has started. Existing native codec and all identity consumers were inspected; no missing prerequisite was found. Graphify remains unavailable, so current source governs. Shared Serde and the remaining parent contract stay open. No local optimized producer, canonical corpus construction, sync or full gate. Authored controls do not establish authenticated corpus, installed/browser/release or performance acceptance. CI remains explicitly waived.


## Comments (0)

