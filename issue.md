# Issue #2053: Preserve complete law-refusal diagnostics with native JSON

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, logic, dependencies, rust

## Body

Move the complete multi-case correspondence-law refusal JSON producer to native PurRDF JSON under parent #1776. The actual discharge_cases publisher emits an ordered Vec<LawCaseFailure>, whose complete payload is case plus RecordedDiag. The full nested native RecordedDiag owner already exists. Principles 4 and 7.

Acceptance:

- Implement the complete native LawCaseFailure output owner and route the actual multi-refusal rdf:JSON observation through it, reusing native compact output and the existing complete RecordedDiag owner. Preserve field order, case order, every diagnostic root/cause/frame/observation/source context and fingerprint. No Serde fallback, partial diagnostic projection, new generic framework or parallel RDF codec.
- Preserve failure semantics: one unavailable case retains its live diagnostic source and case context; a later real counterexample stays decisive and retains earlier failures; multiple unavailable cases never manufacture a semantic verdict. Keep the existing public APIs and typed prior_failures behavior.
- Before replacement, capture the entire actual original observed JSON in the existing every_unavailable_law_case_survives_the_diagnostic_boundary control using its two real authored law failures. Freeze the complete payload without redaction, field selection, coordinate normalization or fingerprint recomputation.
- The payload includes real Rust source coordinates, which can legitimately change after editing source. Native-reencode the immutable historical typed payload byte-for-byte, preserving every original coordinate and fingerprint. Separately exercise the actual current multi-refusal branch with its real current diagnostics; do not claim historical locations describe current execution. Retain all existing assertions and reuse the existing single-error and later-counterexample controls. No new test function or generic codec matrix.
- This is a publisher operation: there is no production JSON hydrator for this payload. Shared Serde derives and the retained test decoder remain under the parent. Do not add an unnecessary decoder or change input profiles.
- Qualify affected logic and downstream pipeline/validation/developer CLI consumers with warnings denied, the three existing authored behavior cases, independent combined review, formatting, helpers, static recipe and normal signed hooks. Reuse the same selected consumer graph for original and final qualification.
- Readiness is charged from 14:10 UTC October 29; land by 22:10 UTC, including qualification and publication.

No implementation or capture build has started. Independent source/trust assessment confirms the complete existing nested owner and identifies the real source-coordinate caveat. Graphify remains unavailable; current source governs. No local optimized producer, canonical corpus construction, sync or full gate. Authored controls do not establish authenticated corpus, installed/browser/release or performance acceptance. CI is explicitly waived. Parent #1776 and every remaining requirement stay open. Preserve the sibling CBOR work.


## Comments (0)

