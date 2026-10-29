# Issue #2043: Preserve object-level domain identities through native JSON

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: logic, dependencies, rust

## Body

Replace the complete object-level reasoning domain-selection identity operation in reasoning_graphs::object_level_domains with native PurRDF JSON. This is a bounded child of #1776. Principles 4 and 7.

The production operation hashes the original ordered tuple of authority, domain profile and nine graph roles, then supplies that exact selection to pipeline reasoning/release, coherence and validation. Native retained RDF-term writers already exist; LogicalGraph and DomainProfile need their own native output implementations.

Acceptance:

- Preserve the complete original compact three-field preimage and BLAKE3 selection bytes, authority/profile names, default-first declared role order, all nine selected worlds and admission/refusal behavior. Do not reconstruct the preimage from SelectedDomains::worlds(), which sorts the worlds afterward.
- Implement native ToJson for the two owning enums, delegating named RDF values to the existing complete retained-term owner. Remove Serde from this actual identity operation. Retain shared Serde uses needed by other parent operations; no fallback, shadow RDF representation or generic tuple framework.
- Capture the complete actual original preimage and production selection before encoder replacement. Extend the existing boundary control to replay the frozen bytes and verify every actual selected world retains the original selection, profile and authority. Keep the actual pipeline source-role control, including missing/wrong/released-source refusals. No new test function or generic RDF compliance matrix.
- Retain actionable operation context and the native storage cause if compact encoding fails. Preserve the reasoning diagnostic category.
- Qualify actual logic, pipeline and validation/developer CLI consumers with warnings denied; run the two affected existing authored controls, independent source review, formatting, helpers, static recipe and normal signed hooks. Charge readiness, qualification and landing to the same eight-hour delivery clock.

This unit does not close the parent's remaining records, derives, manifests, streaming or authenticated acceptance. No local optimized producer, canonical corpus/fixture construction, sync or full gate. Existing controls use only independently authored data and synthetic stage receipts. CI remains explicitly waived; authenticated corpus, installed/embedded consumers, browser and release acceptance stay open. No performance gain is claimed without measurement.


## Comments (0)

