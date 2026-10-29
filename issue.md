# Issue #2059: Preserve refutation domain identity with native JSON

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, logic, dependencies, rust

## Body

Replace the complete entailment refutation-domain selection identity encoder with native PurRDF JSON under parent #1776. Principles 4 and 7.

Acceptance:

- Move the whole ordered three-field preimage (the default-refutation authority, NonemptyObjectDomainV1 profile and entailment world) to existing native JSON owners. Preserve its exact compact bytes and BLAKE3 selection commitment, the selected-world constructor and all source/context admission and refutation behavior. No missing nested owner, new parser, fallback, public API or generic framework.
- Mechanically extract the original encoder into a private production helper and have reason_refutation call it. Capture the actual original helper bytes and actual selected.selection from the existing refutation-domain ownership control before replacing the encoder. Keep capture in the extracted test source; do not add inline test instrumentation to production.
- Freeze the complete old preimage and actual digest. The settled existing control must compare the real production helper bytes and selected native commitment literally against those references, retaining its nonvacuous selected-world, authority, source-assertion and native-verdict assertions. Do not derive expectations from the replacement or freeze changing code-bound witness IDs.
- Preserve fallible native storage diagnostics under the existing Reason category with operation context and original typed cause. Attach the cause once to a fresh diagnostic. Leave unrelated entailment error behavior unchanged.
- Reuse the existing domain-ownership and public positive-entailment controls on the same original/final library graph; add no test function or generic format matrix. Qualify the actual logic, pipeline, validation and developer CLI consumers with warnings denied, formatting/prose, helpers, static producer recipe, independent review and normal signed hooks.
- Charge readiness from 17:25 UTC October 29 and land by 01:25 UTC October 30, including assessment, qualification and publication. Parent #1776 stays open for every remaining requirement.

No local optimized producer, canonical corpus construction, sync or full gate. Authored controls do not establish authenticated corpus, installed/browser/release or performance acceptance. Broader CI remains explicitly waived. Preserve the sibling native CBOR lane, held LinkML publication review and protected donors.


## Comments (0)

