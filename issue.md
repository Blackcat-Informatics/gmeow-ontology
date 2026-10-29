# Issue #2046: Preserve complete modal evidence transport through native JSON

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, cross-cutting, logic, dependencies, rust, complexity-hard

## Body

Move the complete modal-evaluation RDF transport operation to native PurRDF JSON: ModalEvaluation::to_wire and from_wire, including ModalEvaluation, ModalFrontier, ModalWorldEvidence and ModalOp. This is a bounded child of #1776. Principles 4 and 7.

Acceptance:

- Preserve the exact canonical gmeow-modal-evaluation-v1 NUL-prefixed wire, declaration/member order, Box/Diamond tags, false booleans, empty world vectors and complete string values. Preserve actual evidence/receipt identities and the existing length-framed SHA-256 algorithm; no identity recomputation from a replacement-derived expectation.
- Implement all four native ToJson/FromJson owners and wire the actual producer and selected-prefix reader through them. Reuse native compact output and record/variant primitives. Keep the existing String producer API and Option<Result> dispatch: unrelated prefix returns None, malformed selected payload returns an error. No Serde fallback, generic framework or parallel RDF codec.
- Retain every intrinsic validate check and actual RDF importer validate_axiom/duplicate-evidence safeguards. Hydration remains observational and cannot establish native execution or predecessor completeness by itself. Native parse/record errors retain the Reason category, operation context and original typed cause without replacing an existing diagnostic chain.
- Use a native named-record input profile: require every declared member, including explicit false and empty vectors; refuse known duplicates, positional struct arrays and undeclared record members. The frontier variant's previously ignored extra members are deliberately refused. These noncanonical input changes do not alter any canonical producer bytes.
- Capture complete original wire strings and actual evidence/receipt identifiers before replacing the encoder. Extend only the existing modal_derived_row_round_trips_its_complete_receipt control, retaining all RDF equality/provenance assertions. Cover both operators, present/absent and empty frontiers, and legal Unicode identifiers. Store the complete NUL-bearing wire losslessly as a JSON string alongside its actual IDs and inspect decoded bytes. Add only bounded selected-format, required-member, semantic and axiom-binding refusals to that same control; no new test function or generic JSON/RDF compliance matrix.
- Qualify actual logic and downstream pipeline/validation/developer CLI consumers with warnings denied, the affected existing authored control, independent trust/source review, formatting, helpers, static recipe and normal signed hooks. Charge readiness, qualification and landing to the original eight-hour clock.

Shared Serde derives remain required by other explain/foundation transports and stay under the parent; ModalPremise is outside this wire closure. Existing RDF exporter/importer APIs and semantic algorithms need no rewrite. No local optimized producer, canonical corpus/fixture construction, sync or full gate. Controls use independently authored data, not fabricated authenticated assets. CI remains explicitly waived; authenticated corpus, installed/embedded, browser, release and full parent acceptance stay open. No performance gain is claimed without measurement.


## Comments (0)

